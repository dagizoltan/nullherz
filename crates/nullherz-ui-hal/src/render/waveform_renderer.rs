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
    mip_buffers: [wgpu::Buffer; 3], // 0.5x, 0.25x, 0.125x
    num_vertices: u32,
    mip_vertex_counts: [u32; 3],
    globals_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    max_peaks: usize,
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

        let mip_buffers = std::array::from_fn(|i| {
            let scale = 1.0 / (2.0f32.powi((i + 1) as i32));
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&format!("Waveform MIP Buffer {}", i)),
                size: ((max_peaks as f32 * scale) as usize * std::mem::size_of::<WaveformVertex>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });

        Self {
            pipeline,
            vertex_buffer,
            mip_buffers,
            num_vertices: 0,
            mip_vertex_counts: [0; 3],
            globals_buffer,
            bind_group,
            max_peaks,
        }
    }

    pub fn update_peaks(&mut self, queue: &wgpu::Queue, peaks: &[f32]) {
        let mut vertices = Vec::with_capacity(peaks.len() * 2);
        for (i, &peak) in peaks.iter().enumerate() {
            let x = (i as f32 / self.max_peaks as f32) * 2.0 - 1.0;
            vertices.push(WaveformVertex { position: [x, peak] });
            vertices.push(WaveformVertex { position: [x, -peak] });
        }
        self.num_vertices = vertices.len() as u32;
        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));

        // Update MIP levels
        for mip in 0..3 {
            let step = 2usize.pow((mip + 1) as u32);
            let mut mip_vertices = Vec::new();
            for i in (0..peaks.len()).step_by(step) {
                let x = (i as f32 / self.max_peaks as f32) * 2.0 - 1.0;
                let mut max_p = 0.0f32;
                for j in 0..step {
                    if let Some(&p) = peaks.get(i + j) { max_p = max_p.max(p); }
                }
                mip_vertices.push(WaveformVertex { position: [x, max_p] });
                mip_vertices.push(WaveformVertex { position: [x, -max_p] });
            }
            self.mip_vertex_counts[mip] = mip_vertices.len() as u32;
            queue.write_buffer(&self.mip_buffers[mip], 0, bytemuck::cast_slice(&mip_vertices));
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

    pub fn render<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>, zoom: f32) {
        let (buf, count) = if zoom > 0.5 {
            (&self.vertex_buffer, self.num_vertices)
        } else if zoom > 0.2 {
            (&self.mip_buffers[0], self.mip_vertex_counts[0])
        } else if zoom > 0.05 {
            (&self.mip_buffers[1], self.mip_vertex_counts[1])
        } else {
            (&self.mip_buffers[2], self.mip_vertex_counts[2])
        };

        if count > 0 {
            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_bind_group(0, &self.bind_group, &[]);
            render_pass.set_vertex_buffer(0, buf.slice(..));
            render_pass.draw(0..count, 0..1);
        }
    }
}
