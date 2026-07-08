use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct WaveformGlobals {
    scroll_offset: f32,
    zoom: f32,
    accent_color: [f32; 4],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct WaveformVertex {
    position: [f32; 2],
}

pub struct WaveformRenderer {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    num_vertices: u32,
    globals_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    _max_peaks: usize,
}

use std::sync::{Arc, Mutex};

pub struct WaveformCallback {
    pub renderer: Arc<Mutex<WaveformRenderer>>,
}

impl egui_wgpu::CallbackTrait for WaveformCallback {
    fn paint<'a>(&'a self, _info: egui::PaintCallbackInfo, render_pass: &mut wgpu::RenderPass<'a>, _resources: &egui_wgpu::CallbackResources) {
        if let Ok(wf) = self.renderer.lock() {
            let wf_ptr: *const WaveformRenderer = &*wf;
            unsafe { (*wf_ptr).render(render_pass); }
        }
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
                    min_binding_size: None,
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
                    attributes: &[wgpu::VertexAttribute {
                        offset: 0,
                        shader_location: 0,
                        format: wgpu::VertexFormat::Float32x2,
                    }],
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
                topology: wgpu::PrimitiveTopology::LineStrip,
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
            size: (max_peaks * std::mem::size_of::<WaveformVertex>()) as u64,
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

    pub fn update_peaks(&mut self, queue: &wgpu::Queue, peaks: &[f32]) {
        let mut vertices = Vec::with_capacity(peaks.len() * 2);
        let peak_count = peaks.len();
        if peak_count == 0 { return; }

        for (i, &peak) in peaks.iter().enumerate() {
            // Normalized X in range [0, 2] instead of [-1, 1] to allow easier zooming from start
            let x = (i as f32 / peak_count as f32) * 2.0;
            vertices.push(WaveformVertex { position: [x, peak] });
            vertices.push(WaveformVertex { position: [x, -peak] });
        }
        self.num_vertices = vertices.len() as u32;
        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
    }

    pub fn update_from_mip_waveform(&mut self, queue: &wgpu::Queue, mip_waveform: &nullherz_traits::MipWaveform, zoom: f32, display_pixel_width: u32) {
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
            self.update_peaks(queue, peaks);
        }
    }

    pub fn update_globals(&mut self, queue: &wgpu::Queue, scroll: f32, zoom: f32, color: [f32; 4]) {
        let globals = WaveformGlobals {
            scroll_offset: scroll,
            zoom,
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
