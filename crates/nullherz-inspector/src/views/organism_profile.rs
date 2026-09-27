//! Visual Organism Profile Schema & YAML Loader Architecture
//! Defines composable 8-part organism specifications: Field, Topology, Geometry, Motion, Behavior, Render, Genome, AudioMapping.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct OrganismProfile {
    pub id: String,
    pub name: String,
    pub family: String,
    pub description: String,
    pub audio: AudioMappingSpec,
    pub genome: GenomeSpec,
    pub behavior: BehaviorSpec,
    pub geometry: GeometrySpec,
    pub motion: MotionSpec,
    pub field: FieldSpec,
    pub render: RenderSpec,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AudioMappingSpec {
    pub low_energy: String,
    pub mid_energy: String,
    pub high_energy: String,
    pub transient_strength: String,
    pub spectral_centroid: String,
    pub beat_phase: String,
    pub stereo_width: String,
}

impl Default for AudioMappingSpec {
    fn default() -> Self {
        Self {
            low_energy: "growth".to_string(),
            mid_energy: "alignment".to_string(),
            high_energy: "detail".to_string(),
            transient_strength: "rupture".to_string(),
            spectral_centroid: "filament_thickness".to_string(),
            beat_phase: "expansion_pulse".to_string(),
            stereo_width: "spatial_spread".to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GenomeSpec {
    pub branching: f32,
    pub cohesion: f32,
    pub symmetry: f32,
    pub mutation: f32,
    pub persistence: f32,
    pub turbulence: f32,
    pub rigidity: f32,
    pub viscosity: f32,
}

impl Default for GenomeSpec {
    fn default() -> Self {
        Self {
            branching: 0.72,
            cohesion: 0.81,
            symmetry: 0.18,
            mutation: 0.46,
            persistence: 0.91,
            turbulence: 0.37,
            rigidity: 0.50,
            viscosity: 0.52,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BehaviorSpec {
    pub topology: String,
    pub growth: String,
    pub decay: String,
    pub mutation: String,
    pub memory: String,
}

impl Default for BehaviorSpec {
    fn default() -> Self {
        Self {
            topology: "adaptive-graph".to_string(),
            growth: "vector-field".to_string(),
            decay: "slow".to_string(),
            mutation: "transient-triggered".to_string(),
            memory: "strong".to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GeometrySpec {
    pub primitives: Vec<String>,
    pub dimensionality: String,
    pub thickness: String,
    pub smoothing: String,
}

impl Default for GeometrySpec {
    fn default() -> Self {
        Self {
            primitives: vec!["filaments".to_string(), "nodes".to_string(), "ribbons".to_string()],
            dimensionality: "3d".to_string(),
            thickness: "adaptive".to_string(),
            smoothing: "organic".to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MotionSpec {
    pub flow: String,
    pub inertia: f32,
    pub turbulence: f32,
    pub collective_motion: f32,
}

impl Default for MotionSpec {
    fn default() -> Self {
        Self {
            flow: "curl-field".to_string(),
            inertia: 0.83,
            turbulence: 0.35,
            collective_motion: 0.77,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FieldSpec {
    pub field_type: String,
    pub dimensions: u32,
    pub divergence: f32,
}

impl Default for FieldSpec {
    fn default() -> Self {
        Self {
            field_type: "curl-noise".to_string(),
            dimensions: 3,
            divergence: 0.1,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RenderSpec {
    pub trails: bool,
    pub feedback: String,
    pub depth: String,
    pub bloom: String,
    pub wireframe: bool,
}

impl Default for RenderSpec {
    fn default() -> Self {
        Self {
            trails: true,
            feedback: "medium".to_string(),
            depth: "high".to_string(),
            bloom: "subtle".to_string(),
            wireframe: false,
        }
    }
}

impl OrganismProfile {
    pub fn mycelial_bloom() -> Self {
        Self {
            id: "mycelial-bloom".to_string(),
            name: "Mycelial Bloom".to_string(),
            family: "organic".to_string(),
            description: "A branching living network that grows, reconnects and accumulates structural history across bars.".to_string(),
            audio: AudioMappingSpec {
                low_energy: "growth".to_string(),
                mid_energy: "branching".to_string(),
                high_energy: "filament_detail".to_string(),
                transient_strength: "rupture".to_string(),
                spectral_centroid: "filament_thickness".to_string(),
                beat_phase: "expansion_pulse".to_string(),
                stereo_width: "spatial_spread".to_string(),
            },
            genome: GenomeSpec {
                branching: 0.72,
                cohesion: 0.81,
                symmetry: 0.18,
                mutation: 0.46,
                persistence: 0.91,
                turbulence: 0.37,
                rigidity: 0.20,
                viscosity: 0.60,
            },
            behavior: BehaviorSpec {
                topology: "adaptive-graph".to_string(),
                growth: "vector-field".to_string(),
                decay: "slow".to_string(),
                mutation: "transient-triggered".to_string(),
                memory: "strong".to_string(),
            },
            geometry: GeometrySpec {
                primitives: vec!["filaments".to_string(), "nodes".to_string(), "ribbons".to_string()],
                dimensionality: "3d".to_string(),
                thickness: "adaptive".to_string(),
                smoothing: "organic".to_string(),
            },
            motion: MotionSpec {
                flow: "curl-field".to_string(),
                inertia: 0.85,
                turbulence: 0.40,
                collective_motion: 0.80,
            },
            field: FieldSpec {
                field_type: "curl-noise".to_string(),
                dimensions: 3,
                divergence: 0.05,
            },
            render: RenderSpec {
                trails: true,
                feedback: "medium".to_string(),
                depth: "high".to_string(),
                bloom: "subtle".to_string(),
                wireframe: false,
            },
        }
    }

    pub fn fracture_bloom() -> Self {
        Self {
            id: "fracture-bloom".to_string(),
            name: "Fracture Bloom".to_string(),
            family: "crystalline".to_string(),
            description: "A crystalline structure that grows until musical transients break it apart.".to_string(),
            audio: AudioMappingSpec {
                low_energy: "structural_scale".to_string(),
                mid_energy: "subdivision".to_string(),
                high_energy: "surface_detail".to_string(),
                transient_strength: "fracture".to_string(),
                spectral_centroid: "refraction".to_string(),
                beat_phase: "expansion_pulse".to_string(),
                stereo_width: "spatial_spread".to_string(),
            },
            genome: GenomeSpec {
                branching: 0.40,
                cohesion: 0.90,
                symmetry: 0.84,
                mutation: 0.54,
                persistence: 0.80,
                turbulence: 0.20,
                rigidity: 0.88,
                viscosity: 0.10,
            },
            behavior: BehaviorSpec {
                topology: "recursive".to_string(),
                growth: "subdivision".to_string(),
                decay: "fast".to_string(),
                mutation: "transient-triggered".to_string(),
                memory: "medium".to_string(),
            },
            geometry: GeometrySpec {
                primitives: vec!["shards".to_string(), "polyhedra".to_string(), "planes".to_string()],
                dimensionality: "3d".to_string(),
                thickness: "crystalline".to_string(),
                smoothing: "sharp".to_string(),
            },
            motion: MotionSpec {
                flow: "subdivision-expand".to_string(),
                inertia: 0.60,
                turbulence: 0.20,
                collective_motion: 0.90,
            },
            field: FieldSpec {
                field_type: "crystalline-grid".to_string(),
                dimensions: 3,
                divergence: 0.0,
            },
            render: RenderSpec {
                trails: false,
                feedback: "low".to_string(),
                depth: "high".to_string(),
                bloom: "sharp".to_string(),
                wireframe: true,
            },
        }
    }

    pub fn fluid_geometry() -> Self {
        Self {
            id: "fluid-geometry".to_string(),
            name: "Fluid Geometry".to_string(),
            family: "field".to_string(),
            description: "Geometry that behaves like a fluid, morphing continuously between liquid blobs and flowing surfaces.".to_string(),
            audio: AudioMappingSpec {
                low_energy: "field_amplitude".to_string(),
                mid_energy: "pressure_wave".to_string(),
                high_energy: "surface_ripples".to_string(),
                transient_strength: "divergence_impulse".to_string(),
                spectral_centroid: "viscosity".to_string(),
                beat_phase: "pressure_wave".to_string(),
                stereo_width: "flow_divergence".to_string(),
            },
            genome: GenomeSpec {
                branching: 0.20,
                cohesion: 0.71,
                symmetry: 0.21,
                mutation: 0.35,
                persistence: 0.85,
                turbulence: 0.64,
                rigidity: 0.10,
                viscosity: 0.52,
            },
            behavior: BehaviorSpec {
                topology: "fluid-implicit".to_string(),
                growth: "fluid-flow".to_string(),
                decay: "medium".to_string(),
                mutation: "continuous".to_string(),
                memory: "medium".to_string(),
            },
            geometry: GeometrySpec {
                primitives: vec!["membranes".to_string(), "ribbons".to_string()],
                dimensionality: "3d".to_string(),
                thickness: "fluid".to_string(),
                smoothing: "ultra-smooth".to_string(),
            },
            motion: MotionSpec {
                flow: "curl-field".to_string(),
                inertia: 0.70,
                turbulence: 0.65,
                collective_motion: 0.85,
            },
            field: FieldSpec {
                field_type: "curl-noise".to_string(),
                dimensions: 3,
                divergence: 0.12,
            },
            render: RenderSpec {
                trails: true,
                feedback: "high".to_string(),
                depth: "high".to_string(),
                bloom: "medium".to_string(),
                wireframe: false,
            },
        }
    }

    pub fn geometric_swarm() -> Self {
        Self {
            id: "geometric-swarm".to_string(),
            name: "Geometric Swarm".to_string(),
            family: "emergent".to_string(),
            description: "An emergent collective boids lattice forming dynamic spheres, spirals, and lattices based on audio alignment.".to_string(),
            audio: AudioMappingSpec {
                low_energy: "attraction".to_string(),
                mid_energy: "alignment".to_string(),
                high_energy: "separation".to_string(),
                transient_strength: "impulse".to_string(),
                spectral_centroid: "cluster_density".to_string(),
                beat_phase: "synchronization".to_string(),
                stereo_width: "dimensional_spread".to_string(),
            },
            genome: GenomeSpec {
                branching: 0.30,
                cohesion: 0.61,
                symmetry: 0.70,
                mutation: 0.51,
                persistence: 0.75,
                turbulence: 0.31,
                rigidity: 0.40,
                viscosity: 0.30,
            },
            behavior: BehaviorSpec {
                topology: "emergent".to_string(),
                growth: "boids-flocking".to_string(),
                decay: "medium".to_string(),
                mutation: "beat-aware".to_string(),
                memory: "medium".to_string(),
            },
            geometry: GeometrySpec {
                primitives: vec!["nodes".to_string(), "points".to_string(), "filaments".to_string()],
                dimensionality: "3d".to_string(),
                thickness: "instanced".to_string(),
                smoothing: "sharp".to_string(),
            },
            motion: MotionSpec {
                flow: "boids-hybrid".to_string(),
                inertia: 0.83,
                turbulence: 0.31,
                collective_motion: 0.92,
            },
            field: FieldSpec {
                field_type: "boids-field".to_string(),
                dimensions: 3,
                divergence: 0.20,
            },
            render: RenderSpec {
                trails: true,
                feedback: "medium".to_string(),
                depth: "high".to_string(),
                bloom: "medium".to_string(),
                wireframe: false,
            },
        }
    }

    pub fn neural_garden() -> Self {
        Self {
            id: "neural-garden".to_string(),
            name: "Neural Garden".to_string(),
            family: "neural-organic".to_string(),
            description: "Recurrent latent network dreaming up the behavior and growth of a procedural organic garden.".to_string(),
            audio: AudioMappingSpec {
                low_energy: "latent_drive".to_string(),
                mid_energy: "morphology".to_string(),
                high_energy: "growth_rate".to_string(),
                transient_strength: "mutation".to_string(),
                spectral_centroid: "curvature".to_string(),
                beat_phase: "rhythm".to_string(),
                stereo_width: "spatial_spread".to_string(),
            },
            genome: GenomeSpec {
                branching: 0.80,
                cohesion: 0.75,
                symmetry: 0.45,
                mutation: 0.65,
                persistence: 0.86,
                turbulence: 0.50,
                rigidity: 0.30,
                viscosity: 0.40,
            },
            behavior: BehaviorSpec {
                topology: "neural-guided".to_string(),
                growth: "recurrent-evolution".to_string(),
                decay: "slow".to_string(),
                mutation: "phrase-triggered".to_string(),
                memory: "recurrent".to_string(),
            },
            geometry: GeometrySpec {
                primitives: vec!["branches".to_string(), "leaves".to_string(), "membranes".to_string()],
                dimensionality: "3d".to_string(),
                thickness: "adaptive".to_string(),
                smoothing: "organic".to_string(),
            },
            motion: MotionSpec {
                flow: "recurrent-evolution".to_string(),
                inertia: 0.86,
                turbulence: 0.45,
                collective_motion: 0.75,
            },
            field: FieldSpec {
                field_type: "neural-latent".to_string(),
                dimensions: 3,
                divergence: 0.08,
            },
            render: RenderSpec {
                trails: true,
                feedback: "high".to_string(),
                depth: "high".to_string(),
                bloom: "subtle".to_string(),
                wireframe: false,
            },
        }
    }

    pub fn impossible_machine() -> Self {
        Self {
            id: "impossible-machine".to_string(),
            name: "Impossible Machine".to_string(),
            family: "architectural".to_string(),
            description: "Modular architectural beams, rings and lattices reconfiguring mechanically in event-driven sync.".to_string(),
            audio: AudioMappingSpec {
                low_energy: "mass".to_string(),
                mid_energy: "articulation".to_string(),
                high_energy: "mechanical_detail".to_string(),
                transient_strength: "assembly_event".to_string(),
                spectral_centroid: "reconfiguration".to_string(),
                beat_phase: "mechanism_cycle".to_string(),
                stereo_width: "spatial_spread".to_string(),
            },
            genome: GenomeSpec {
                branching: 0.50,
                cohesion: 0.85,
                symmetry: 0.64,
                mutation: 0.61,
                persistence: 0.80,
                turbulence: 0.15,
                rigidity: 0.95,
                viscosity: 0.05,
            },
            behavior: BehaviorSpec {
                topology: "modular".to_string(),
                growth: "assembly-event".to_string(),
                decay: "quantized".to_string(),
                mutation: "transient-driven".to_string(),
                memory: "high".to_string(),
            },
            geometry: GeometrySpec {
                primitives: vec!["beams".to_string(), "rings".to_string(), "planes".to_string(), "lattices".to_string()],
                dimensionality: "3d".to_string(),
                thickness: "quantized".to_string(),
                smoothing: "hard-edge".to_string(),
            },
            motion: MotionSpec {
                flow: "mechanical".to_string(),
                inertia: 0.90,
                turbulence: 0.10,
                collective_motion: 0.95,
            },
            field: FieldSpec {
                field_type: "architectural-grid".to_string(),
                dimensions: 3,
                divergence: 0.0,
            },
            render: RenderSpec {
                trails: false,
                feedback: "subtle".to_string(),
                depth: "high".to_string(),
                bloom: "none".to_string(),
                wireframe: true,
            },
        }
    }

    pub fn void_organism() -> Self {
        Self {
            id: "void-organism".to_string(),
            name: "Void Organism".to_string(),
            family: "minimal".to_string(),
            description: "Sparse, elegant dark-space geometry with extreme depth and subtle, slow emergence.".to_string(),
            audio: AudioMappingSpec {
                low_energy: "presence".to_string(),
                mid_energy: "surface_detail".to_string(),
                high_energy: "faint_shimmer".to_string(),
                transient_strength: "emergence".to_string(),
                spectral_centroid: "surface_detail".to_string(),
                beat_phase: "subtle_pulse".to_string(),
                stereo_width: "depth_spread".to_string(),
            },
            genome: GenomeSpec {
                branching: 0.20,
                cohesion: 0.92,
                symmetry: 0.51,
                mutation: 0.18,
                persistence: 0.94,
                turbulence: 0.10,
                rigidity: 0.40,
                viscosity: 0.80,
            },
            behavior: BehaviorSpec {
                topology: "sparse".to_string(),
                growth: "slow-emergence".to_string(),
                decay: "extremely-slow".to_string(),
                mutation: "rare".to_string(),
                memory: "very-high".to_string(),
            },
            geometry: GeometrySpec {
                primitives: vec!["points".to_string(), "thin-curves".to_string(), "minimal-surfaces".to_string()],
                dimensionality: "3d".to_string(),
                thickness: "ultra-thin".to_string(),
                smoothing: "minimal".to_string(),
            },
            motion: MotionSpec {
                flow: "minimal-void".to_string(),
                inertia: 0.98,
                turbulence: 0.05,
                collective_motion: 0.60,
            },
            field: FieldSpec {
                field_type: "minimal-void".to_string(),
                dimensions: 3,
                divergence: 0.01,
            },
            render: RenderSpec {
                trails: true,
                feedback: "subtle".to_string(),
                depth: "extreme".to_string(),
                bloom: "minimal".to_string(),
                wireframe: false,
            },
        }
    }

    pub fn fractal_pulse() -> Self {
        Self {
            id: "fractal-pulse".to_string(),
            name: "Fractal Pulse".to_string(),
            family: "recursive".to_string(),
            description: "Recursive transform geometry scaling and branching in mathematical self-similarity.".to_string(),
            audio: AudioMappingSpec {
                low_energy: "recursion_scale".to_string(),
                mid_energy: "branch_angle".to_string(),
                high_energy: "iteration_detail".to_string(),
                transient_strength: "recursion_reset".to_string(),
                spectral_centroid: "iteration_detail".to_string(),
                beat_phase: "pulse".to_string(),
                stereo_width: "spatial_spread".to_string(),
            },
            genome: GenomeSpec {
                branching: 0.83,
                cohesion: 0.70,
                symmetry: 0.76,
                mutation: 0.38,
                persistence: 0.82,
                turbulence: 0.42,
                rigidity: 0.60,
                viscosity: 0.30,
            },
            behavior: BehaviorSpec {
                topology: "recursive".to_string(),
                growth: "recursive-transform".to_string(),
                decay: "medium".to_string(),
                mutation: "transient-driven".to_string(),
                memory: "medium".to_string(),
            },
            geometry: GeometrySpec {
                primitives: vec!["curves".to_string(), "polygons".to_string(), "surfaces".to_string()],
                dimensionality: "3d".to_string(),
                thickness: "recursive".to_string(),
                smoothing: "mathematical".to_string(),
            },
            motion: MotionSpec {
                flow: "recursive-transform".to_string(),
                inertia: 0.80,
                turbulence: 0.30,
                collective_motion: 0.85,
            },
            field: FieldSpec {
                field_type: "recursive-transform".to_string(),
                dimensions: 3,
                divergence: 0.05,
            },
            render: RenderSpec {
                trails: true,
                feedback: "medium".to_string(),
                depth: "recursive".to_string(),
                bloom: "medium".to_string(),
                wireframe: false,
            },
        }
    }

    pub fn all_defaults() -> Vec<Self> {
        vec![
            Self::mycelial_bloom(),
            Self::fracture_bloom(),
            Self::fluid_geometry(),
            Self::geometric_swarm(),
            Self::neural_garden(),
            Self::impossible_machine(),
            Self::void_organism(),
            Self::fractal_pulse(),
        ]
    }
}
