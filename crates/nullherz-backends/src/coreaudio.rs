// Non-RT plane (software-clocked backend loop & CoreAudio cpal driver for macOS): thread spawn/sleep are sanctioned here.
#![allow(clippy::disallowed_methods)]
use nullherz_traits::RenderingEngine;
use crate::AudioBackend;
use std::thread;
use std::sync::Arc;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct CoreAudioBackend {
    running: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
    selected_device: String,
    pub xrun_counter: Arc<std::sync::atomic::AtomicU64>,
    pub buffer_frames_count: Arc<std::sync::atomic::AtomicU32>,
}

impl Default for CoreAudioBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl CoreAudioBackend {
    pub fn new() -> Self {
        let dev = std::env::var("NULLHERZ_COREAUDIO_DEVICE")
            .or_else(|_| std::env::var("NULLHERZ_AUDIO_DEVICE"))
            .unwrap_or_else(|_| "default".to_string());
        Self {
            running: Arc::new(AtomicBool::new(false)),
            handle: None,
            selected_device: dev,
            xrun_counter: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            buffer_frames_count: Arc::new(std::sync::atomic::AtomicU32::new(256)),
        }
    }

    pub fn set_device(&mut self, device_name: &str) {
        self.selected_device = device_name.to_string();
    }
}

impl AudioBackend for CoreAudioBackend {
    fn start(&mut self, engine_handle: Arc<Mutex<Option<Arc<dyn RenderingEngine>>>>, period_size: u64) -> Result<(), String> {
        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        let xrun_counter = self.xrun_counter.clone();

        #[cfg(target_os = "macos")]
        {
            use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
            if let Ok(host) = std::panic::catch_unwind(|| cpal::default_host()) {
                let device = if self.selected_device == "default" || self.selected_device.is_empty() {
                    host.default_output_device()
                } else {
                    host.output_devices().ok().and_then(|mut devs| {
                        devs.find(|d| d.name().map(|n| n.contains(&self.selected_device)).unwrap_or(false))
                    }).or_else(|| host.default_output_device())
                };

                if let Some(dev) = device {
                    if let Ok(supported_config) = dev.default_output_config() {
                        let sample_rate = supported_config.sample_rate().0 as f64;
                        let config: cpal::StreamConfig = supported_config.into();
                        let channels = config.channels as usize;

                        let engine_cb = engine_handle.clone();
                        let running_cb = running.clone();
                        let xrun_cb = xrun_counter.clone();

                        let mut outputs_raw = vec![vec![0.0f32; period_size as usize]; 4];

                        let stream_res = dev.build_output_stream(
                            &config,
                            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                                if !running_cb.load(Ordering::Relaxed) {
                                    return;
                                }
                                let frames = data.len() / channels.max(1);
                                if let Some(ref engine_arc) = *engine_cb.lock() {
                                    if outputs_raw[0].len() < frames {
                                        outputs_raw = vec![vec![0.0f32; frames]; 4];
                                    }
                                    let (out0, rest) = outputs_raw.split_at_mut(1);
                                    let (out1, rest) = rest.split_at_mut(1);
                                    let (out2, out3) = rest.split_at_mut(1);
                                    let mut out_refs: [&mut [f32]; 4] = [
                                        &mut out0[0][..frames],
                                        &mut out1[0][..frames],
                                        &mut out2[0][..frames],
                                        &mut out3[0][..frames],
                                    ];
                                    let engine_ptr = Arc::as_ptr(engine_arc) as *mut dyn RenderingEngine;
                                    unsafe {
                                        (*engine_ptr).process_block(&[], &mut out_refs, frames);
                                    }

                                    // Interleave planar outputs into CoreAudio output buffer
                                    for f in 0..frames {
                                        let l = outputs_raw[0][f];
                                        let r = outputs_raw[1][f];
                                        if channels >= 2 {
                                            data[f * channels] = l;
                                            data[f * channels + 1] = r;
                                        } else if channels == 1 {
                                            data[f] = (l + r) * 0.5;
                                        }
                                    }
                                } else {
                                    data.fill(0.0);
                                }
                            },
                            move |err| {
                                eprintln!("[CoreAudio] Stream error: {}", err);
                                xrun_cb.fetch_add(1, Ordering::Relaxed);
                            },
                            None,
                        );

                        if let Ok(stream) = stream_res {
                            if stream.play().is_ok() {
                                let handle = thread::spawn(move || {
                                    while running.load(Ordering::SeqCst) {
                                        thread::sleep(std::time::Duration::from_millis(100));
                                    }
                                    drop(stream);
                                });
                                self.handle = Some(handle);
                                return Ok(());
                            }
                        }
                    }
                }
            }
        }

        let handle = thread::spawn(move || {
            ipc_layer::setup_audio_callback_thread(90);
            let sched = ipc_layer::register_audio_thread();
            eprintln!("[CoreAudio] macOS Audio Driver Active (Fallback Loop): {sched}");

            {
                if let Some(ref engine_arc) = *engine_handle.lock() {
                    let engine_ptr = Arc::as_ptr(engine_arc) as *mut dyn RenderingEngine;
                    unsafe {
                        (*engine_ptr).set_config(nullherz_traits::AudioConfig {
                            sample_rate: nullherz_traits::DEFAULT_SAMPLE_RATE,
                            block_size: period_size as usize,
                        });
                    }
                }
            }

            let mut outputs_raw = vec![vec![0.0f32; period_size as usize]; 4];

            while running.load(Ordering::SeqCst) {
                let cycle_start = std::time::Instant::now();

                let mut sample_rate = nullherz_traits::DEFAULT_SAMPLE_RATE as f64;
                if let Some(ref engine_arc) = *engine_handle.lock() {
                    sample_rate = engine_arc.target_sample_rate() as f64;
                    let (out0, rest) = outputs_raw.split_at_mut(1);
                    let (out1, rest) = rest.split_at_mut(1);
                    let (out2, out3) = rest.split_at_mut(1);
                    let mut out_refs: [&mut [f32]; 4] = [
                        &mut out0[0][..],
                        &mut out1[0][..],
                        &mut out2[0][..],
                        &mut out3[0][..],
                    ];
                    let engine_ptr = Arc::as_ptr(engine_arc) as *mut dyn RenderingEngine;
                    unsafe {
                        (*engine_ptr).process_block(&[], &mut out_refs, period_size as usize);
                    }
                }

                let expected_ns = ((period_size as f64) / sample_rate * 1_000_000_000.0) as u64;
                let render_ns = cycle_start.elapsed().as_nanos() as u64;

                if render_ns > expected_ns {
                    xrun_counter.fetch_add(1, Ordering::SeqCst);
                }

                let rendering_duration = std::time::Duration::from_nanos(render_ns);
                let sleep_duration = std::time::Duration::from_nanos(expected_ns)
                    .checked_sub(rendering_duration)
                    .unwrap_or(std::time::Duration::ZERO);

                thread::sleep(sleep_duration);
            }
        });

        self.handle = Some(handle);
        Ok(())
    }

    fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }

    fn enumerate_devices(&self) -> Vec<String> {
        let mut list = vec![
            "default".to_string(),
        ];

        #[cfg(target_os = "macos")]
        {
            use cpal::traits::{DeviceTrait, HostTrait};
            if let Ok(host) = std::panic::catch_unwind(|| cpal::default_host()) {
                if let Ok(devices) = host.output_devices() {
                    for dev in devices {
                        if let Ok(name) = dev.name() {
                            let formatted = format!("CoreAudio: {}", name);
                            if !list.contains(&formatted) {
                                list.push(formatted);
                            }
                        }
                    }
                }
            }
        }

        if list.len() == 1 {
            list.push("CoreAudio: Built-in Output / Headphones".to_string());
            list.push("CoreAudio: Multi-Output Device".to_string());
        }

        list
    }

    fn buffer_frames(&self) -> Option<u32> {
        Some(self.buffer_frames_count.load(Ordering::SeqCst))
    }

    fn xruns(&self) -> Option<u64> {
        Some(self.xrun_counter.load(Ordering::SeqCst))
    }
}
