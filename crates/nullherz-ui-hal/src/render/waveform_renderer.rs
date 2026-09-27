use wgpu::util::DeviceExt;
use std::sync::Arc;
use parking_lot::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WaveformStyle {
    #[default]
    MultiBand,      // Tri-Color Frequency Band (Bass = Amber/Red, Mid = Green, High = Cyan/Blue)
    Mono,           // Solid Accent Silhouette
    PhonLoudness,   // Perceptual Loudness (ISO 226 equal-loudness weighting)
    SpectrumHeatmap,// Thermal Energy Heatmap (Red/Yellow peak -> Indigo/Blue low)
    Outline,        // Vector Boundary Contour
}

impl WaveformStyle {
    pub fn name(&self) -> &'static str {
        match self {
            Self::MultiBand => "Multi-Band RGB",
            Self::Mono => "Mono Silhouette",
            Self::PhonLoudness => "Phon Loudness (ISO 226)",
            Self::SpectrumHeatmap => "Spectral Heatmap",
            Self::Outline => "Vector Outline",
        }
    }

    pub fn all() -> &'static [Self] {
        &[
            Self::MultiBand,
            Self::Mono,
            Self::PhonLoudness,
            Self::SpectrumHeatmap,
            Self::Outline,
        ]
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct WaveformGlobals {
    scroll_offset: f32,
    zoom: f32,
    is_vertical: u32,
    waveform_style: u32,
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

fn compute_sample_color(
    style: WaveformStyle,
    l: f32,
    m: f32,
    h: f32,
    top: f32,
    bot: f32,
    accent_color: [f32; 4],
    is_edge: bool,
) -> [f32; 4] {
    let sum = (l + m + h).max(1e-6);
    let amp = top.max(-bot).clamp(0.0, 1.0);
    let bright = 0.55 + 0.45 * amp.sqrt();

    match style {
        WaveformStyle::MultiBand => {
            let mix = |k: usize| {
                (l * WaveformRenderer::LOW_COLOR[k] + m * WaveformRenderer::MID_COLOR[k] + h * WaveformRenderer::HIGH_COLOR[k]) / sum * bright
            };
            [mix(0), mix(1), mix(2), 1.0]
        }
        WaveformStyle::Mono => {
            let factor = 0.45 + 0.55 * bright;
            [
                accent_color[0] * factor,
                accent_color[1] * factor,
                accent_color[2] * factor,
                accent_color[3],
            ]
        }
        WaveformStyle::PhonLoudness => {
            // ISO 226 perceptual loudness weighting: mids/highs carry higher perceptual loudness (phon) per unit energy
            let phon_weight = ((m * 1.3 + h * 1.1 + l * 0.5) / sum).clamp(0.0, 1.0) * bright;
            // Magenta/purple low phon -> Bright gold/yellow high phon
            let r = 0.4 + 0.6 * phon_weight;
            let g = 0.1 + 0.85 * phon_weight.powf(1.5);
            let b = (0.7 * (1.0 - phon_weight)).clamp(0.0, 0.8);
            [r, g, b, 1.0]
        }
        WaveformStyle::SpectrumHeatmap => {
            // Thermal heatmap energy: low amp (indigo/blue) -> mid amp (yellow/green) -> high amp (red/orange)
            let e = amp;
            let r = (e * 1.8).clamp(0.1, 1.0);
            let g = ((1.0 - (e - 0.5).abs() * 2.0) * 0.9).clamp(0.1, 0.9);
            let b = ((1.0 - e * 1.5) * 0.9).clamp(0.1, 0.8);
            [r, g, b, 1.0]
        }
        WaveformStyle::Outline => {
            if is_edge {
                // High contrast accent outline for top & bottom bounds
                accent_color
            } else {
                // Semi-transparent interior fill
                [accent_color[0], accent_color[1], accent_color[2], 0.25]
            }
        }
    }
}

impl WaveformRenderer {
    /// Frequency-band colors: low = warm amber, mid = green-teal,
    /// high = icy white-blue. Tuned for dark backgrounds.
    pub const LOW_COLOR: [f32; 3] = [0.98, 0.45, 0.16];
    pub const MID_COLOR: [f32; 3] = [0.18, 0.85, 0.55];
    pub const HIGH_COLOR: [f32; 3] = [0.75, 0.87, 1.0];

    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat, max_peaks: usize) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("waveform.wgsl"));

        let globals_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Waveform Globals Buffer"),
            contents: bytemuck::cast_slice(&[WaveformGlobals {
                scroll_offset: 0.0,
                zoom: 1.0,
                is_vertical: 0,
                waveform_style: 0,
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

        let peak_count = peaks.len().min(self._max_peaks);
        let mut vertices = Vec::with_capacity(peak_count * 2);
        for i in 0..peak_count {
            let start = i * peaks.len() / peak_count;
            let end = (((i + 1) * peaks.len()) / peak_count).max(start + 1);
            let peak = peaks[start..end].iter().fold(0.0f32, |a, &v| a.max(v));
            let x = (i as f32 / peak_count as f32) * 2.0;
            vertices.push(WaveformVertex { position: [x, peak], color });
            vertices.push(WaveformVertex { position: [x, -peak], color });
        }
        self.num_vertices = vertices.len() as u32;
        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
    }

    /// Upload only a WINDOW of the band waveform — `[start_ratio, end_ratio)`
    /// of the track — remapped to the full x range.
    pub fn update_from_band_window(
        &mut self,
        queue: &wgpu::Queue,
        band: &nullherz_traits::BandWaveform,
        start_ratio: f32,
        end_ratio: f32,
        display_pixel_width: u32,
        style: WaveformStyle,
        accent_color: [f32; 4],
    ) {
        if band.is_empty() || end_ratio <= start_ratio { return; }

        let target = (display_pixel_width.max(1) as f32 * 2.0) as usize;
        let span = (end_ratio - start_ratio).clamp(1e-6, 1.0);
        let mut level_idx = 0;
        for (i, level) in band.env_max.levels.iter().enumerate() {
            level_idx = i;
            if (level.len() as f32 * span) as usize <= target * 2 {
                break;
            }
        }
        let level_idx = level_idx.min(band.env_max.levels.len().saturating_sub(1));

        let get = |m: &nullherz_traits::MipWaveform| m.levels.get(level_idx).cloned();
        let (Some(low), Some(mid), Some(high), Some(env_min), Some(env_max)) = (
            get(&band.low), get(&band.mid), get(&band.high), get(&band.env_min), get(&band.env_max),
        ) else { return; };
        let n = low.len().min(mid.len()).min(high.len()).min(env_min.len()).min(env_max.len());
        if n == 0 { return; }

        let f_start = start_ratio * n as f32;
        let f_span = span * n as f32;
        let count = ((f_span as usize).max(2)).min(self._max_peaks);

        let mut vertices = Vec::with_capacity(count * 2);
        for i in 0..count {
            let idx_f = f_start + (i as f32 / count as f32) * f_span;
            let x = (i as f32 / count as f32) * 2.0;
            if idx_f < 0.0 || idx_f >= n as f32 {
                vertices.push(WaveformVertex { position: [x, 0.0], color: [0.0, 0.0, 0.0, 0.0] });
                vertices.push(WaveformVertex { position: [x, 0.0], color: [0.0, 0.0, 0.0, 0.0] });
                continue;
            }
            let idx = idx_f as usize;
            let (l, m, h) = (low[idx], mid[idx], high[idx]);
            let top = env_max[idx].clamp(-1.0, 1.0);
            let bot = env_min[idx].clamp(-1.0, 1.0);

            let top_col = compute_sample_color(style, l, m, h, top, bot, accent_color, true);
            let bot_col = compute_sample_color(style, l, m, h, top, bot, accent_color, false);

            vertices.push(WaveformVertex { position: [x, top], color: top_col });
            vertices.push(WaveformVertex { position: [x, bot], color: bot_col });
        }
        self.num_vertices = vertices.len() as u32;
        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
    }

    /// Mono fallback of `update_from_band_window` for pre-band library rows.
    pub fn update_from_mip_window(
        &mut self,
        queue: &wgpu::Queue,
        mip: &nullherz_traits::MipWaveform,
        start_ratio: f32,
        end_ratio: f32,
        display_pixel_width: u32,
        color: [f32; 4],
    ) {
        if mip.levels.is_empty() || end_ratio <= start_ratio { return; }
        let target = (display_pixel_width.max(1) as f32 * 2.0) as usize;
        let span = (end_ratio - start_ratio).clamp(1e-6, 1.0);
        let mut level_idx = 0;
        for (i, level) in mip.levels.iter().enumerate() {
            level_idx = i;
            if (level.len() as f32 * span) as usize <= target * 2 {
                break;
            }
        }
        let Some(peaks) = mip.levels.get(level_idx.min(mip.levels.len() - 1)) else { return; };
        let n = peaks.len();
        if n == 0 { return; }
        let f_start = start_ratio * n as f32;
        let f_span = span * n as f32;
        let count = ((f_span as usize).max(2)).min(self._max_peaks);
        let mut vertices = Vec::with_capacity(count * 2);
        for i in 0..count {
            let idx_f = f_start + (i as f32 / count as f32) * f_span;
            let x = (i as f32 / count as f32) * 2.0;
            let peak = if idx_f < 0.0 || idx_f >= n as f32 { 0.0 } else { peaks[idx_f as usize] };
            vertices.push(WaveformVertex { position: [x, peak], color });
            vertices.push(WaveformVertex { position: [x, -peak], color });
        }
        self.num_vertices = vertices.len() as u32;
        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
    }

    pub fn update_from_band_waveform(
        &mut self,
        queue: &wgpu::Queue,
        band: &nullherz_traits::BandWaveform,
        zoom: f32,
        display_pixel_width: u32,
        style: WaveformStyle,
        accent_color: [f32; 4],
    ) {
        if band.is_empty() { return; }

        let mut level_idx = 0;
        if display_pixel_width > 0 {
            let target_peaks = display_pixel_width as f32 * 2.0 * zoom.max(1.0);
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

            let top_col = compute_sample_color(style, l, m, h, top, bot, accent_color, true);
            let bot_col = compute_sample_color(style, l, m, h, top, bot, accent_color, false);

            let x = (i as f32 / peak_count as f32) * 2.0;
            vertices.push(WaveformVertex { position: [x, top], color: top_col });
            vertices.push(WaveformVertex { position: [x, bot], color: bot_col });
        }
        self.num_vertices = vertices.len() as u32;
        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
    }

    pub fn update_from_mip_waveform(&mut self, queue: &wgpu::Queue, mip_waveform: &nullherz_traits::MipWaveform, zoom: f32, display_pixel_width: u32, color: [f32; 4]) {
        let mut level_idx = 0;
        if display_pixel_width > 0 && !mip_waveform.levels.is_empty() {
            let target_peaks = display_pixel_width as f32 * 2.0 * zoom.max(1.0);

            for (i, level) in mip_waveform.levels.iter().enumerate() {
                level_idx = i;
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

    pub fn update_globals(&mut self, queue: &wgpu::Queue, scroll: f32, zoom: f32, is_vertical: bool, style: WaveformStyle, color: [f32; 4]) {
        let globals = WaveformGlobals {
            scroll_offset: scroll,
            zoom,
            is_vertical: if is_vertical { 1 } else { 0 },
            waveform_style: style as u32,
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
