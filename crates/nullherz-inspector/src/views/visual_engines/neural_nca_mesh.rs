//! Engine 7: neural_nca_mesh (3D Neural Cellular Automata Mesh Engine)
//! Applies Neural Cellular Automata to 3D mesh topologies (Sphere, Torus, Voxel Architecture, Crystalline Sprout).
//! Each vertex maintains a local state vector and interacts with immediate neighbors via a lightweight neural update step.
//! Environmental audio triggers drive morphogenesis:
//! - Low frequencies (bass) force crystallization and structural cohesion.
//! - High frequencies and fast transients cause mesh fragmentation and crystal sprouting.
//! - Beat phase drives rhythmic mutation and self-healing back to baseline geometry.

use eframe::egui;
use crate::state::{AudioNervousSystem, VisualGenome};
use super::NeuralVisualEngine;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NcaMeshTopology {
    SphereMesh,
    TorusGrid,
    VoxelArchitecture,
    CrystallineSprout,
}

impl NcaMeshTopology {
    pub fn all() -> &'static [NcaMeshTopology] {
        &[
            NcaMeshTopology::SphereMesh,
            NcaMeshTopology::TorusGrid,
            NcaMeshTopology::VoxelArchitecture,
            NcaMeshTopology::CrystallineSprout,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            NcaMeshTopology::SphereMesh => "3D Sphere NCA Mesh (Self-Organizing Sphere)",
            NcaMeshTopology::TorusGrid => "3D Torus NCA Grid (Circal Ring Growth)",
            NcaMeshTopology::VoxelArchitecture => "3D Voxel Architecture (Dynamic Voxel Building)",
            NcaMeshTopology::CrystallineSprout => "3D Crystalline Sprout (Audio-Triggered Crystal Sprouting)",
        }
    }
}

#[derive(Clone, Debug)]
pub struct NcaVertex {
    pub base_pos: [f32; 3],        // Original baseline 3D coordinate
    pub state: [f32; 4],           // Local NCA state vector [density, energy, crystal_growth, health]
    pub neighbors: Vec<usize>,      // Indices of connected immediate neighbor vertices
    pub normal: [f32; 3],          // Surface normal vector
    pub displacement: [f32; 3],    // Dynamic 3D displacement vector
    pub sprout_len: f32,           // Outward crystal sprout length
}

#[derive(Clone, Debug)]
pub struct NeuralNcaMeshEngine {
    pub topology: NcaMeshTopology,
    pub vertices: Vec<NcaVertex>,
    pub weights_local: [[f32; 4]; 4],
    pub weights_neighbor: [[f32; 4]; 4],
}

impl NeuralNcaMeshEngine {
    pub fn new() -> Self {
        let mut engine = Self {
            topology: NcaMeshTopology::SphereMesh,
            vertices: Vec::new(),
            weights_local: [
                [0.8, -0.1, 0.2, 0.1],
                [0.1, 0.85, 0.1, -0.2],
                [0.2, 0.3, 0.7, 0.0],
                [0.0, 0.1, -0.1, 0.9],
            ],
            weights_neighbor: [
                [0.15, 0.1, -0.05, 0.0],
                [0.05, 0.2, 0.1, -0.1],
                [0.1, 0.15, 0.25, 0.0],
                [0.0, 0.05, 0.0, 0.15],
            ],
        };
        engine.rebuild_mesh();
        engine
    }

    #[allow(dead_code)]
    pub fn cycle_topology(&mut self) {
        let topologies = NcaMeshTopology::all();
        if let Some(idx) = topologies.iter().position(|t| *t == self.topology) {
            self.topology = topologies[(idx + 1) % topologies.len()];
            self.rebuild_mesh();
        }
    }

    #[allow(dead_code)]
    pub fn set_topology(&mut self, topology: NcaMeshTopology) {
        if self.topology != topology {
            self.topology = topology;
            self.rebuild_mesh();
        }
    }

    pub fn rebuild_mesh(&mut self) {
        self.vertices.clear();

        match self.topology {
            NcaMeshTopology::SphereMesh => {
                let lat_steps = 12;
                let lon_steps = 20;
                let radius = 1.0f32;

                for i in 0..=lat_steps {
                    let lat = (i as f32 / lat_steps as f32) * std::f32::consts::PI - std::f32::consts::FRAC_PI_2;
                    let z = lat.sin() * radius;
                    let xy_r = lat.cos() * radius;

                    for j in 0..lon_steps {
                        let lon = (j as f32 / lon_steps as f32) * std::f32::consts::TAU;
                        let x = lon.cos() * xy_r;
                        let y = lon.sin() * xy_r;

                        let norm_len = (x * x + y * y + z * z).sqrt().max(0.001);
                        let normal = [x / norm_len, y / norm_len, z / norm_len];

                        self.vertices.push(NcaVertex {
                            base_pos: [x, y, z],
                            state: [0.5, 0.2, 0.1, 1.0],
                            neighbors: Vec::new(),
                            normal,
                            displacement: [0.0, 0.0, 0.0],
                            sprout_len: 0.0,
                        });
                    }
                }

                // Connect spherical mesh grid neighbors
                let total_vertices = self.vertices.len();
                for i in 0..total_vertices {
                    let lat_idx = i / lon_steps;
                    let lon_idx = i % lon_steps;

                    let mut neighbors = Vec::new();
                    // Left and Right lon neighbors
                    neighbors.push(lat_idx * lon_steps + (lon_idx + lon_steps - 1) % lon_steps);
                    neighbors.push(lat_idx * lon_steps + (lon_idx + 1) % lon_steps);

                    // Up and Down lat neighbors
                    if lat_idx > 0 {
                        neighbors.push((lat_idx - 1) * lon_steps + lon_idx);
                    }
                    if lat_idx < lat_steps {
                        neighbors.push((lat_idx + 1) * lon_steps + lon_idx);
                    }

                    self.vertices[i].neighbors = neighbors;
                }
            }
            NcaMeshTopology::TorusGrid => {
                let r_major = 1.0f32;
                let r_minor = 0.4f32;
                let segs_major = 24;
                let segs_minor = 12;

                for i in 0..segs_major {
                    let u = (i as f32 / segs_major as f32) * std::f32::consts::TAU;
                    let cos_u = u.cos();
                    let sin_u = u.sin();

                    for j in 0..segs_minor {
                        let v = (j as f32 / segs_minor as f32) * std::f32::consts::TAU;
                        let cos_v = v.cos();
                        let sin_v = v.sin();

                        let x = (r_major + r_minor * cos_v) * cos_u;
                        let y = (r_major + r_minor * cos_v) * sin_u;
                        let z = r_minor * sin_v;

                        let nx = cos_v * cos_u;
                        let ny = cos_v * sin_u;
                        let nz = sin_v;

                        self.vertices.push(NcaVertex {
                            base_pos: [x, y, z],
                            state: [0.4, 0.3, 0.2, 1.0],
                            neighbors: Vec::new(),
                            normal: [nx, ny, nz],
                            displacement: [0.0, 0.0, 0.0],
                            sprout_len: 0.0,
                        });
                    }
                }

                // Connect torus grid neighbors
                let total_vertices = self.vertices.len();
                for i in 0..total_vertices {
                    let maj = i / segs_minor;
                    let min = i % segs_minor;

                    let neighbors = vec![
                        maj * segs_minor + (min + segs_minor - 1) % segs_minor,
                        maj * segs_minor + (min + 1) % segs_minor,
                        ((maj + segs_major - 1) % segs_major) * segs_minor + min,
                        ((maj + 1) % segs_major) * segs_minor + min,
                    ];
                    self.vertices[i].neighbors = neighbors;
                }
            }
            NcaMeshTopology::VoxelArchitecture | NcaMeshTopology::CrystallineSprout => {
                let dim = 5;
                let spacing = 0.45f32;
                let offset = (dim as f32 - 1.0) * spacing * 0.5;

                for x_i in 0..dim {
                    let x = x_i as f32 * spacing - offset;
                    for y_i in 0..dim {
                        let y = y_i as f32 * spacing - offset;
                        for z_i in 0..dim {
                            let z = z_i as f32 * spacing - offset;

                            let norm_len = (x * x + y * y + z * z).sqrt().max(0.001);
                            let normal = [x / norm_len, y / norm_len, z / norm_len];

                            self.vertices.push(NcaVertex {
                                base_pos: [x, y, z],
                                state: [0.6, 0.1, 0.0, 1.0],
                                neighbors: Vec::new(),
                                normal,
                                displacement: [0.0, 0.0, 0.0],
                                sprout_len: 0.0,
                            });
                        }
                    }
                }

                // Connect 3D voxel grid 6-way neighbors
                let total_vertices = self.vertices.len();
                for i in 0..total_vertices {
                    let x_i = i / (dim * dim);
                    let y_i = (i / dim) % dim;
                    let z_i = i % dim;

                    let mut neighbors = Vec::new();
                    if x_i > 0 { neighbors.push((x_i - 1) * dim * dim + y_i * dim + z_i); }
                    if x_i < dim - 1 { neighbors.push((x_i + 1) * dim * dim + y_i * dim + z_i); }
                    if y_i > 0 { neighbors.push(x_i * dim * dim + (y_i - 1) * dim + z_i); }
                    if y_i < dim - 1 { neighbors.push(x_i * dim * dim + (y_i + 1) * dim + z_i); }
                    if z_i > 0 { neighbors.push(x_i * dim * dim + y_i * dim + (z_i - 1)); }
                    if z_i < dim - 1 { neighbors.push(x_i * dim * dim + y_i * dim + (z_i + 1)); }

                    self.vertices[i].neighbors = neighbors;
                }
            }
        }
    }

    /// Step Neural Cellular Automata (NCA) update across the 3D vertex mesh
    pub fn step_nca(&mut self, nervous: &AudioNervousSystem) {
        let num_vertices = self.vertices.len();
        if num_vertices == 0 {
            return;
        }

        let bass = nervous.low_band;
        let high = nervous.high_band;
        let transient = nervous.fast_transient_spike;
        let beat_phase = nervous.beat_phase;

        let mut next_states = Vec::with_capacity(num_vertices);
        let mut next_sprouts = Vec::with_capacity(num_vertices);
        let mut next_displacements = Vec::with_capacity(num_vertices);

        for i in 0..num_vertices {
            let v = &self.vertices[i];

            // 1. Calculate Average Neighbor State
            let mut neighbor_avg = [0.0f32; 4];
            if !v.neighbors.is_empty() {
                for &n_idx in &v.neighbors {
                    let n_v = &self.vertices[n_idx];
                    for c in 0..4 {
                        neighbor_avg[c] += n_v.state[c];
                    }
                }
                for c in 0..4 {
                    neighbor_avg[c] /= v.neighbors.len() as f32;
                }
            }

            // 2. Neural Cellular Automata Local State Matrix Multiplication
            let mut next_state = [0.0f32; 4];
            for r in 0..4 {
                let mut sum = 0.0f32;
                for c in 0..4 {
                    sum += self.weights_local[r][c] * v.state[c];
                    sum += self.weights_neighbor[r][c] * neighbor_avg[c];
                }
                // Padé SIMD rational activation function: tanh_approx(x)
                let x2 = sum * sum;
                let act = (sum * (27.0 + x2) / (27.0 + 9.0 * x2)).clamp(-1.0, 1.0);
                next_state[r] = (act * 0.5 + 0.5).clamp(0.0, 1.0);
            }

            // 3. Audio Environmental Triggers & Morphogenesis Dynamics
            // - Low Frequencies (Bass): Force crystallization & structural cohesion (alignment along normals)
            // - High Frequencies & Transients: Cause mesh fragmentation & crystal sprouting
            // - Beat Phase: Rhythmic healing towards baseline geometry
            let crystal_target = (next_state[2] + bass * 0.5).clamp(0.0, 1.0);
            next_state[2] += (crystal_target - next_state[2]) * 0.2;

            // Sprout length driven by high frequency transients
            let sprout_target = if transient > 0.4 || high > 0.6 {
                (v.sprout_len + (transient + high) * 0.3).min(1.2)
            } else {
                v.sprout_len * (0.92 - (1.0 - beat_phase) * 0.05) // Rhythmic healing
            };

            // 3D Displacement offset along surface normal + jitter on transients
            let fragmentation = (transient * 0.25 + high * 0.15) * (i as f32 * 1.37).sin();
            let cohesion = (1.0 - bass * 0.5).clamp(0.2, 1.0);

            let disp_x = v.normal[0] * (sprout_target * 0.4 + next_state[2] * 0.2) + fragmentation * cohesion;
            let disp_y = v.normal[1] * (sprout_target * 0.4 + next_state[2] * 0.2) + (fragmentation * 1.2) * cohesion;
            let disp_z = v.normal[2] * (sprout_target * 0.4 + next_state[2] * 0.2) + (fragmentation * 0.8) * cohesion;

            next_states.push(next_state);
            next_sprouts.push(sprout_target);
            next_displacements.push([disp_x, disp_y, disp_z]);
        }

        // Apply updated states
        for i in 0..num_vertices {
            self.vertices[i].state = next_states[i];
            self.vertices[i].sprout_len = next_sprouts[i];
            self.vertices[i].displacement = next_displacements[i];
        }
    }
}

impl Default for NeuralNcaMeshEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl NeuralVisualEngine for NeuralNcaMeshEngine {
    fn prepare_tensor_inputs(&mut self, nervous: &AudioNervousSystem, _genome: &VisualGenome) -> Vec<f32> {
        self.step_nca(nervous);
        vec![nervous.low_band, nervous.high_band, nervous.beat_phase]
    }

    fn render(
        &mut self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        nervous: &AudioNervousSystem,
        genome: &VisualGenome,
        _telemetry: &Option<audio_core::Telemetry>,
        time: f32,
    ) {
        self.prepare_tensor_inputs(nervous, genome);

        let center = rect.center();
        let scale = (rect.width().min(rect.height())) * 0.32;

        // 3D Rotation Matrix Camera (Yaw & Pitch)
        let yaw = time * 0.4;
        let pitch = time * 0.25;

        let cos_y = yaw.cos();
        let sin_y = yaw.sin();
        let cos_p = pitch.cos();
        let sin_p = pitch.sin();

        let rotate_3d = |pos: [f32; 3]| -> (f32, f32, f32) {
            let x = pos[0];
            let y = pos[1];
            let z = pos[2];

            // Yaw around Y axis
            let x1 = x * cos_y + z * sin_y;
            let z1 = -x * sin_y + z * cos_y;

            // Pitch around X axis
            let y2 = y * cos_p - z1 * sin_p;
            let z2 = y * sin_p + z1 * cos_p;

            (x1, y2, z2)
        };

        // Project and depth sort 3D vertices
        let mut proj_vertices = Vec::with_capacity(self.vertices.len());

        for (idx, v) in self.vertices.iter().enumerate() {
            let current_pos = [
                v.base_pos[0] + v.displacement[0],
                v.base_pos[1] + v.displacement[1],
                v.base_pos[2] + v.displacement[2],
            ];

            let (rx, ry, rz) = rotate_3d(current_pos);
            let depth = rz + 3.0; // Camera z-offset
            let inv_depth = 1.0 / depth.max(0.1);

            let px = center.x + rx * scale * inv_depth;
            let py = center.y + ry * scale * inv_depth;

            proj_vertices.push((idx, egui::pos2(px, py), depth, v));
        }

        // Depth sort back to front (farthest depth first)
        proj_vertices.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

        // Render 3D Wireframe Adjacency Edges & Crystal Outgrowths
        for &(_idx, p1, depth, v) in &proj_vertices {
            let norm_depth = ((depth - 2.0) / 2.0).clamp(0.0, 1.0);
            let alpha = 0.85 * (1.0 - norm_depth * 0.5);

            // Audio-reactive NCA state color field (Cyan/Blue -> Emerald/Gold -> Crystal Magenta)
            let density = v.state[0];
            let crystal = v.state[2];
            let hue = (0.55 + crystal * 0.35 + nervous.low_band * 0.1) % 1.0;
            let color = hsva_to_color32(hue, 0.85, 0.95, alpha);

            // Draw wireframe neighbor connection lines
            for &n_idx in &v.neighbors {
                if let Some(&(_, p2, _, _)) = proj_vertices.iter().find(|item| item.0 == n_idx) {
                    ui.painter().line_segment([p1, p2], egui::Stroke::new(1.0, color.linear_multiply(0.4)));
                }
            }

            // Draw Outward Crystal Sprout Tip Facet / Polygon
            if v.sprout_len > 0.05 {
                let sprout_tip_pos = [
                    v.base_pos[0] + v.displacement[0] + v.normal[0] * v.sprout_len * 0.5,
                    v.base_pos[1] + v.displacement[1] + v.normal[1] * v.sprout_len * 0.5,
                    v.base_pos[2] + v.displacement[2] + v.normal[2] * v.sprout_len * 0.5,
                ];
                let (sx, sy, sz) = rotate_3d(sprout_tip_pos);
                let s_depth = sz + 3.0;
                let s_inv = 1.0 / s_depth.max(0.1);
                let p_sprout = egui::pos2(center.x + sx * scale * s_inv, center.y + sy * scale * s_inv);

                let crystal_color = egui::Color32::from_rgba_unmultiplied(255, 120, 240, (alpha * 220.0) as u8);
                ui.painter().line_segment([p1, p_sprout], egui::Stroke::new(1.8, crystal_color));
                ui.painter().circle_filled(p_sprout, 2.5 + v.sprout_len * 2.0, crystal_color);
            }

            // Render Vertex Cell / Voxel Node
            let pt_radius = (2.0 + density * 3.5 + v.sprout_len * 2.0) * (1.0 - norm_depth * 0.4);
            ui.painter().circle_filled(p1, pt_radius, color);
        }
    }
}

fn hsva_to_color32(h: f32, s: f32, v: f32, a: f32) -> egui::Color32 {
    let h_deg = (h.fract() + 1.0).fract() * 6.0;
    let c = v * s;
    let x = c * (1.0 - (h_deg % 2.0 - 1.0).abs());
    let m = v - c;

    let (r_p, g_p, b_p) = match h_deg as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };

    let r = ((r_p + m) * 255.0).clamp(0.0, 255.0) as u8;
    let g = ((g_p + m) * 255.0).clamp(0.0, 255.0) as u8;
    let b = ((b_p + m) * 255.0).clamp(0.0, 255.0) as u8;
    let alpha = (a * 255.0).clamp(0.0, 255.0) as u8;

    egui::Color32::from_rgba_unmultiplied(r, g, b, alpha)
}
