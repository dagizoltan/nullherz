use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct WaveformGlobals {
    scroll_offset: f32,
    zoom: f32,
    // WGSL uniform layout: vec4<f32> is 16-byte aligned, so the shader-side
    // struct is 32 bytes. Without this padding Rust packs 24 bytes and wgpu
    // rejects the bind group at draw time ("bound with size 24, expects 32").
    _pad: [f32; 2],
    accent_color: [f32; 4],
}

// Compile-time guard: must match the WGSL Globals struct in waveform.wgsl.
const _: () = assert!(std::mem::size_of::<WaveformGlobals>() == 32);

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct WaveformVertex {
    position: [f32; 2],
    /// Per-vertex color: frequency-band tint for colored waveforms, the
    /// accent color for the mono fallback.
    color: [f32; 4],
}

pub struct WaveformRenderer {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    num_vertices: u32,
    globals_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    _max_peaks: usize,
}

use std::sync::Arc;
use parking_lot::Mutex;

pub struct WaveformCallback {
    pub renderer: Arc<Mutex<WaveformRenderer>>,
}

impl egui_wgpu::CallbackTrait for WaveformCallback {
    fn paint<'a>(&'a self, _info: egui::PaintCallbackInfo, render_pass: &mut wgpu::RenderPass<'a>, _resources: &egui_wgpu::CallbackResources) {
        let wf = self.renderer.lock();
        let wf_ptr: *const WaveformRenderer = &*wf;
        unsafe { (*wf_ptr).render(render_pass); }
    }
}

pub fn ui_paint_waveform(ui: &mut egui::Ui, rect: egui::Rect, renderer: Arc<Mutex<WaveformRenderer>>) {
    ui.painter().add(egui_wgpu::Callback::new_paint_callback(rect, WaveformCallback { renderer }));
}

impl WaveformRenderer {
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat, max_peaks: usize) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("waveform.wgsl"));

        let globals_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Waveform Globals Buffer"),
            contents: bytemuck::cast_slice(&[WaveformGlobals {
                scroll_offset: 0.0,
                zoom: 1.0,
                _pad: [0.0; 2],
                accent_color: [0.0, 1.0, 0.8, 1.0],
            }]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<WaveformGlobals>() as u64),
                },
                count: None,
            }],
            label: Some("Waveform Bind Group Layout"),
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer.as_entire_binding(),
            }],
            label: Some("Waveform Bind Group"),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Waveform Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Waveform Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<WaveformVertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            offset: 0,
                            shader_location: 0,
                            format: wgpu::VertexFormat::Float32x2,
                        },
                        wgpu::VertexAttribute {
                            offset: 8,
                            shader_location: 1,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Waveform Vertex Buffer"),
            size: (max_peaks * 2 * std::mem::size_of::<WaveformVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            vertex_buffer,
            num_vertices: 0,
            globals_buffer,
            bind_group,
            _max_peaks: max_peaks,
        }
    }

    pub fn update_peaks(&mut self, queue: &wgpu::Queue, peaks: &[f32], color: [f32; 4]) {
        if peaks.is_empty() { return; }

        // If the level is denser than the vertex buffer, DOWNSAMPLE across
        // the whole track (max of each stride window). Truncating with
        // `.take()` here used to display only the first max_peaks points
        // stretched to full width — the waveform showed the track's opening
        // seconds as if they were the whole file.
        let peak_count = peaks.len().min(self._max_peaks);
        let mut vertices = Vec::with_capacity(peak_count * 2);
        for i in 0..peak_count {
            let start = i * peaks.len() / peak_count;
            let end = (((i + 1) * peaks.len()) / peak_count).max(start + 1);
            let peak = peaks[start..end].iter().fold(0.0f32, |a, &v| a.max(v));
            // Normalized X in range [0, 2] instead of [-1, 1] to allow easier zooming from start
            let x = (i as f32 / peak_count as f32) * 2.0;
            vertices.push(WaveformVertex { position: [x, peak], color });
            vertices.push(WaveformVertex { position: [x, -peak], color });
        }
        self.num_vertices = vertices.len() as u32;
        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
    }

    /// Frequency-band colors: low = warm amber, mid = green-teal,
    /// high = icy white-blue. Tuned for dark backgrounds.
    const LOW_COLOR: [f32; 3] = [0.98, 0.45, 0.16];
    const MID_COLOR: [f32; 3] = [0.18, 0.85, 0.55];
    const HIGH_COLOR: [f32; 3] = [0.75, 0.87, 1.0];

    /// Upload a frequency-colored waveform: asymmetric min/max envelope for
    /// the SHAPE, per-point color mixed from the three band peaks. Falls
    /// back to nothing (caller should use `update_from_mip_waveform`) when
    /// the band data is empty.
    pub fn update_from_band_waveform(
        &mut self,
        queue: &wgpu::Queue,
        band: &nullherz_traits::BandWaveform,
        zoom: f32,
        display_pixel_width: u32,
    ) {
        if band.is_empty() { return; }

        // LOD selection identical to the mono path, driven by the envelope.
        let mut level_idx = 0;
        if display_pixel_width > 0 {
            let target_peaks = (display_pixel_width as f32 * 2.0) / zoom.max(0.001);
            for (i, level) in band.env_max.levels.iter().enumerate() {
                level_idx = i;
                if level.len() as f32 <= target_peaks * 1.2 {
                    break;
                }
            }
        }
        let level_idx = level_idx.min(band.env_max.levels.len().saturating_sub(1));

        let get = |m: &nullherz_traits::MipWaveform| m.levels.get(level_idx).cloned();
        let (Some(low), Some(mid), Some(high), Some(env_min), Some(env_max)) = (
            get(&band.low), get(&band.mid), get(&band.high), get(&band.env_min), get(&band.env_max),
        ) else { return; };

        // All series share lengths per level; min() guards a malformed row.
        let n = low.len().min(mid.len()).min(high.len()).min(env_min.len()).min(env_max.len());
        if n == 0 { return; }
        let peak_count = n.min(self._max_peaks);

        let mut vertices = Vec::with_capacity(peak_count * 2);
        for i in 0..peak_count {
            let start = i * n / peak_count;
            let end = (((i + 1) * n) / peak_count).max(start + 1);
            let seg_max = |s: &[f32]| s[start..end].iter().fold(0.0f32, |a, &v| a.max(v));
            let l = seg_max(&low);
            let m = seg_max(&mid);
            let h = seg_max(&high);
            let top = env_max[start..end].iter().fold(f32::MIN, |a, &v| a.max(v)).clamp(-1.0, 1.0);
            let bot = env_min[start..end].iter().fold(f32::MAX, |a, &v| a.min(v)).clamp(-1.0, 1.0);

            let sum = (l + m + h).max(1e-6);
            let amp = top.max(-bot).clamp(0.0, 1.0);
            // Quiet sections dim slightly so loud hits pop.
            let bright = 0.55 + 0.45 * amp.sqrt();
            let mix = |k: usize| {
                (l * Self::LOW_COLOR[k] + m * Self::MID_COLOR[k] + h * Self::HIGH_COLOR[k]) / sum * bright
            };
            let color = [mix(0), mix(1), mix(2), 1.0];

            let x = (i as f32 / peak_count as f32) * 2.0;
            vertices.push(WaveformVertex { position: [x, top], color });
            vertices.push(WaveformVertex { position: [x, bot], color });
        }
        self.num_vertices = vertices.len() as u32;
        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
    }

    pub fn update_from_mip_waveform(&mut self, queue: &wgpu::Queue, mip_waveform: &nullherz_traits::MipWaveform, zoom: f32, display_pixel_width: u32, color: [f32; 4]) {
        // Advanced LOD selection logic:
        // We aim for approximately 2 peaks per display pixel for optimal visual density
        // without overloading the GPU with redundant geometry.

        let mut level_idx = 0;
        if display_pixel_width > 0 && !mip_waveform.levels.is_empty() {
            let target_peaks = (display_pixel_width as f32 * 2.0) / zoom;

            for (i, level) in mip_waveform.levels.iter().enumerate() {
                level_idx = i;
                // Since levels are power-of-2 downsampled, we find the first level
                // that has enough density to satisfy our target.
                if level.len() as f32 <= target_peaks * 1.2 {
                    break;
                }
            }
        }

        let level_idx = level_idx.min(mip_waveform.levels.len().saturating_sub(1));
        if let Some(peaks) = mip_waveform.levels.get(level_idx) {
            self.update_peaks(queue, peaks, color);
        }
    }

    pub fn update_globals(&mut self, queue: &wgpu::Queue, scroll: f32, zoom: f32, color: [f32; 4]) {
        let globals = WaveformGlobals {
            scroll_offset: scroll,
            zoom,
            _pad: [0.0; 2],
            accent_color: color,
        };
        queue.write_buffer(&self.globals_buffer, 0, bytemuck::cast_slice(&[globals]));
    }

    pub fn render<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>) {
        if self.num_vertices > 0 {
            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_bind_group(0, &self.bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.draw(0..self.num_vertices, 0..1);
        }
    }
}
