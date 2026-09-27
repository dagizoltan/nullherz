use nullherz_traits::{
    AudioProcessor, Command, ProcessContext, ProcessorCapability, ProcessorCommand,
    ProcessorFactory, ProcessorMetadata, ProcessorTypeId, SignalProcessor,
};
use audio_dsp::{AlignedBuffer, Filter, ZdfSvf};

/// Deterministic 32-bit PRNG (`mulberry32`) for bit-exact seed reproduction in `#![no_std]` / RT safety.
#[inline]
pub fn mulberry32_step(state: &mut u32) -> f32 {
    *state = state.wrapping_add(0x6D2B79F5);
    let mut t = (*state ^ (*state >> 15)).wrapping_mul(1 | *state);
    t = (t ^ (t >> 7)).wrapping_mul(61 | t) ^ t;
    let res = (t ^ (t >> 14)) as f32 / 4294967296.0;
    res
}

/// Fast, rational Padé approximant soft-clipper (saturates smoothly without transcendentals).
#[inline]
pub fn pade_saturate(x: f32) -> f32 {
    let x2 = x * x;
    (x * (1.0 + 0.3333333 * x2)) / (1.0 + x2)
}

pub struct MutatorProcessor {
    pub id: u64,
    sample_rate: f32,

    // Mutation Macros (0.0..1.0)
    pub flesh: f32,
    pub bone: f32,
    pub teeth: f32,
    pub parasite: f32,
    pub asymmetry: f32,
    pub abomination: f32,

    // Controls
    pub seed: u32,
    pub dry_wet: f32,
    pub transient_lock: bool,
    pub flux_lock: bool,
    pub sub_mono_safety: bool,
    pub input_trim_db: f32,
    pub output_trim_db: f32,
    pub character: u32, // 0: Manual, 1: Diseased, 2: Mechanical, 3: Possessed, 4: Abomination

    // Internal State
    prng_state: u32,

    // FLESH Engine state (Formant filters)
    flesh_filter1_l: ZdfSvf,
    flesh_filter1_r: ZdfSvf,
    flesh_filter2_l: ZdfSvf,
    flesh_filter2_r: ZdfSvf,
    flesh_lfo_phase: f32,

    // BONE Engine state (Resonators)
    bone_delay_l: AlignedBuffer,
    bone_delay_r: AlignedBuffer,
    bone_write_ptr: usize,

    // TEETH Engine state (Envelope followers)
    fast_env_l: f32,
    slow_env_l: f32,
    fast_env_r: f32,
    slow_env_r: f32,

    // PARASITE Engine state
    parasite_filter_l: ZdfSvf,
    parasite_filter_r: ZdfSvf,

    // ASYMMETRY Engine state (Sub-mono safety filter & micro-delay)
    asym_delay_r: AlignedBuffer,
    asym_write_ptr: usize,
    sub_safety_filter_l: ZdfSvf,
    sub_safety_filter_r: ZdfSvf,
}

impl MutatorProcessor {
    pub fn new(id: u64, sample_rate: f32) -> Self {
        let sr = if sample_rate > 0.0 { sample_rate } else { 48000.0 };

        let mut f1_l = ZdfSvf::new(sr);
        let mut f1_r = ZdfSvf::new(sr);
        let mut f2_l = ZdfSvf::new(sr);
        let mut f2_r = ZdfSvf::new(sr);
        f1_l.set_params(500.0, 3.0);
        f1_r.set_params(500.0, 3.0);
        f2_l.set_params(1500.0, 3.0);
        f2_r.set_params(1500.0, 3.0);

        let mut p_l = ZdfSvf::new(sr);
        let mut p_r = ZdfSvf::new(sr);
        p_l.set_params(2000.0, 1.0);
        p_r.set_params(2000.0, 1.0);

        let mut safe_l = ZdfSvf::new(sr);
        let mut safe_r = ZdfSvf::new(sr);
        safe_l.set_params(200.0, 0.707);
        safe_r.set_params(200.0, 0.707);

        Self {
            id,
            sample_rate: sr,
            flesh: 0.0,
            bone: 0.0,
            teeth: 0.0,
            parasite: 0.0,
            asymmetry: 0.0,
            abomination: 0.0,
            seed: 1337,
            dry_wet: 0.5,
            transient_lock: false,
            flux_lock: false,
            sub_mono_safety: true,
            input_trim_db: 0.0,
            output_trim_db: 0.0,
            character: 0,
            prng_state: 1337,
            flesh_filter1_l: f1_l,
            flesh_filter1_r: f1_r,
            flesh_filter2_l: f2_l,
            flesh_filter2_r: f2_r,
            flesh_lfo_phase: 0.0,
            bone_delay_l: AlignedBuffer::new(2048),
            bone_delay_r: AlignedBuffer::new(2048),
            bone_write_ptr: 0,
            fast_env_l: 0.0,
            slow_env_l: 0.0,
            fast_env_r: 0.0,
            slow_env_r: 0.0,
            parasite_filter_l: p_l,
            parasite_filter_r: p_r,
            asym_delay_r: AlignedBuffer::new(1024), // Max ~20ms micro-delay
            asym_write_ptr: 0,
            sub_safety_filter_l: safe_l,
            sub_safety_filter_r: safe_r,
        }
    }

    fn apply_character_mappings(&mut self) {
        match self.character {
            1 => { // Diseased
                self.flesh = 1.00;
                self.bone = 0.65;
                self.teeth = 0.45;
                self.parasite = 0.75;
                self.asymmetry = 0.35;
            }
            2 => { // Mechanical
                self.flesh = 0.30;
                self.bone = 1.00;
                self.teeth = 0.80;
                self.parasite = 0.55;
                self.asymmetry = 0.60;
            }
            3 => { // Possessed
                self.flesh = 0.65;
                self.bone = 0.45;
                self.teeth = 0.55;
                self.parasite = 1.00;
                self.asymmetry = 0.85;
            }
            4 => { // Abomination
                self.flesh = 0.90;
                self.bone = 0.90;
                self.teeth = 0.85;
                self.parasite = 0.95;
                self.asymmetry = 1.00;
            }
            _ => {}
        }
    }
}

impl nullherz_traits::RtSafe for MutatorProcessor {}

impl SignalProcessor for MutatorProcessor {
    fn setup(&mut self, config: nullherz_traits::AudioConfig) {
        if config.sample_rate > 0.0 {
            self.sample_rate = config.sample_rate;
            self.flesh_filter1_l = ZdfSvf::new(self.sample_rate);
            self.flesh_filter1_r = ZdfSvf::new(self.sample_rate);
            self.flesh_filter2_l = ZdfSvf::new(self.sample_rate);
            self.flesh_filter2_r = ZdfSvf::new(self.sample_rate);
            self.parasite_filter_l = ZdfSvf::new(self.sample_rate);
            self.parasite_filter_r = ZdfSvf::new(self.sample_rate);
            self.sub_safety_filter_l = ZdfSvf::new(self.sample_rate);
            self.sub_safety_filter_r = ZdfSvf::new(self.sample_rate);
        }
    }

    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        if inputs.is_empty() || outputs.is_empty() { return; }

        let in_l = inputs[0];
        let in_r = if inputs.len() > 1 { inputs[1] } else { inputs[0] };

        let is_stereo_out = outputs.len() > 1;
        let (first_out, second_out) = outputs.split_at_mut(1);
        let out_l = &mut *first_out[0];

        let mut len = in_l.len().min(in_r.len()).min(out_l.len());
        if is_stereo_out {
            len = len.min(second_out[0].len());
        }
        if len == 0 { return; }

        self.apply_character_mappings();

        let in_gain = 10.0f32.powf(self.input_trim_db / 20.0);
        let out_gain = 10.0f32.powf(self.output_trim_db / 20.0);

        // ABOMINATION meta-macro non-linear coupling factor
        let abomination = self.abomination.clamp(0.0, 1.0);
        let abom_boost = 1.0 + abomination * 1.5;

        // Effective macro weights with Abomination trajectory coupling
        let eff_flesh = (self.flesh * abom_boost).clamp(0.0, 1.0);
        let eff_bone = (self.bone * abom_boost).clamp(0.0, 1.0);
        let eff_teeth = (self.teeth * abom_boost).clamp(0.0, 1.0);
        let eff_parasite = (self.parasite * abom_boost).clamp(0.0, 1.0);
        let eff_asym = (self.asymmetry * abom_boost).clamp(0.0, 1.0);

        let lfo_inc = 2.0 * core::f32::consts::PI * (0.2 + eff_flesh * 1.5) / self.sample_rate;
        let bone_buf_len = self.bone_delay_l.len();
        let asym_buf_len = self.asym_delay_r.len();

        for i in 0..len {
            let dry_l = in_l[i] * in_gain;
            let dry_r = in_r[i] * in_gain;

            // 1. FLESH Engine (Parallel Formant Filtering)
            let mut flesh_out_l = 0.0;
            let mut flesh_out_r = 0.0;
            if eff_flesh > 0.001 {
                self.flesh_lfo_phase += lfo_inc;
                if self.flesh_lfo_phase >= 2.0 * core::f32::consts::PI {
                    self.flesh_lfo_phase -= 2.0 * core::f32::consts::PI;
                }
                let lfo_val = self.flesh_lfo_phase.sin();
                let f1_freq = (400.0 + lfo_val * 200.0 * eff_flesh).clamp(100.0, 4000.0);
                let f2_freq = (1500.0 - lfo_val * 400.0 * eff_flesh).clamp(300.0, 8000.0);

                self.flesh_filter1_l.set_params(f1_freq, 2.0 + eff_flesh * 4.0);
                self.flesh_filter1_r.set_params(f1_freq * 1.05, 2.0 + eff_flesh * 4.0);
                self.flesh_filter2_l.set_params(f2_freq, 2.0 + eff_flesh * 4.0);
                self.flesh_filter2_r.set_params(f2_freq * 0.95, 2.0 + eff_flesh * 4.0);

                flesh_out_l = self.flesh_filter1_l.process_bp(dry_l) + self.flesh_filter2_l.process_bp(dry_l);
                flesh_out_r = self.flesh_filter1_r.process_bp(dry_r) + self.flesh_filter2_r.process_bp(dry_r);
            }

            // 2. BONE Engine (Resonator Bank with Inharmonic Offsets)
            let mut bone_out_l = 0.0;
            let mut bone_out_r = 0.0;
            if eff_bone > 0.001 {
                let delay_samples_l = (200.0 - eff_bone * 150.0).clamp(10.0, 500.0);
                let delay_samples_r = delay_samples_l * (1.0 + eff_bone * 0.15);

                let read_pos_l = (self.bone_write_ptr as f32 + bone_buf_len as f32 - delay_samples_l) % bone_buf_len as f32;
                let read_pos_r = (self.bone_write_ptr as f32 + bone_buf_len as f32 - delay_samples_r) % bone_buf_len as f32;

                let idx_l = read_pos_l as usize % bone_buf_len;
                let idx_r = read_pos_r as usize % bone_buf_len;

                let res_l = self.bone_delay_l[idx_l];
                let res_r = self.bone_delay_r[idx_r];

                let fb = (0.3 + eff_bone * 0.55).clamp(0.0, 0.92);
                self.bone_delay_l[self.bone_write_ptr] = dry_l + res_l * fb;
                self.bone_delay_r[self.bone_write_ptr] = dry_r + res_r * fb;

                bone_out_l = res_l;
                bone_out_r = res_r;
            }
            self.bone_write_ptr = (self.bone_write_ptr + 1) % bone_buf_len;

            // 3. TEETH Engine (Transient Followers + Padé Soft Clipping)
            let mut teeth_out_l = 0.0;
            let mut teeth_out_r = 0.0;
            if eff_teeth > 0.001 {
                let abs_l = dry_l.abs();
                let abs_r = dry_r.abs();

                self.fast_env_l += (abs_l - self.fast_env_l) * 0.1;
                self.slow_env_l += (abs_l - self.slow_env_l) * 0.005;
                self.fast_env_r += (abs_r - self.fast_env_r) * 0.1;
                self.slow_env_r += (abs_r - self.slow_env_r) * 0.005;

                let transient_l = (self.fast_env_l - self.slow_env_l).max(0.0);
                let transient_r = (self.fast_env_r - self.slow_env_r).max(0.0);

                let drive_l = 1.0 + eff_teeth * 15.0 * (if self.transient_lock { transient_l * 5.0 } else { 1.0 });
                let drive_r = 1.0 + eff_teeth * 15.0 * (if self.transient_lock { transient_r * 5.0 } else { 1.0 });

                teeth_out_l = pade_saturate(dry_l * drive_l);
                teeth_out_r = pade_saturate(dry_r * drive_r);
            }

            // 4. PARASITE Engine (Input-Gated Filtered Noise & Micro AM/FM)
            let mut parasite_out_l = 0.0;
            let mut parasite_out_r = 0.0;
            if eff_parasite > 0.001 {
                let noise_l = (mulberry32_step(&mut self.prng_state) - 0.5) * 2.0;
                let noise_r = (mulberry32_step(&mut self.prng_state) - 0.5) * 2.0;

                let env_gate = ((dry_l.abs() + dry_r.abs()) * 2.0).clamp(0.0, 1.0);
                if env_gate > 0.001 {
                    let cutoff = (1000.0 + eff_parasite * 6000.0).clamp(200.0, 12000.0);
                    self.parasite_filter_l.set_params(cutoff, 1.0);
                    self.parasite_filter_r.set_params(cutoff, 1.0);

                    let filtered_n_l = self.parasite_filter_l.process_bp(noise_l);
                    let filtered_n_r = self.parasite_filter_r.process_bp(noise_r);

                    parasite_out_l = filtered_n_l * env_gate * eff_parasite * 0.5;
                    parasite_out_r = filtered_n_r * env_gate * eff_parasite * 0.5;
                }
            }

            // Parallel Recombination of Mutations
            let wet_l_sum = dry_l + flesh_out_l * eff_flesh + bone_out_l * eff_bone * 0.5 + teeth_out_l * eff_teeth + parasite_out_l;
            let wet_r_sum = dry_r + flesh_out_r * eff_flesh + bone_out_r * eff_bone * 0.5 + teeth_out_r * eff_teeth + parasite_out_r;

            // 5. ASYMMETRY Engine (L/R Micro-Delay & Sub-mono Safety Filtering)
            let mut final_wet_l = wet_l_sum;
            let mut final_wet_r = wet_r_sum;

            if eff_asym > 0.001 {
                let micro_delay_samples = (eff_asym * 400.0).clamp(0.0, 960.0); // Up to ~20ms
                self.asym_delay_r[self.asym_write_ptr] = wet_r_sum;

                let read_pos_r = (self.asym_write_ptr as f32 + asym_buf_len as f32 - micro_delay_samples) % asym_buf_len as f32;
                let idx_r = read_pos_r as usize % asym_buf_len;

                final_wet_r = self.asym_delay_r[idx_r];
                self.asym_write_ptr = (self.asym_write_ptr + 1) % asym_buf_len;

                // Sub-mono safety: High-pass phase difference under 200 Hz
                if self.sub_mono_safety {
                    let mid = (final_wet_l + final_wet_r) * 0.5;
                    let side = (final_wet_l - final_wet_r) * 0.5;

                    self.sub_safety_filter_r.set_params(200.0, 0.707);
                    let safe_side = self.sub_safety_filter_r.process_hp(side);

                    final_wet_l = mid + safe_side;
                    final_wet_r = mid - safe_side;
                }
            }

            // Dry/Wet Recombination
            let mix = self.dry_wet.clamp(0.0, 1.0);
            let out_sample_l = (dry_l * (1.0 - mix) + final_wet_l * mix) * out_gain;
            let out_sample_r = (dry_r * (1.0 - mix) + final_wet_r * mix) * out_gain;

            // Safety clipping & NaN/Inf protection
            out_l[i] = if out_sample_l.is_finite() { out_sample_l.clamp(-2.0, 2.0) } else { 0.0 };
            if is_stereo_out {
                second_out[0][i] = if out_sample_r.is_finite() { out_sample_r.clamp(-2.0, 2.0) } else { 0.0 };
            }
        }
    }

    fn reset(&mut self) {
        self.flesh_filter1_l.reset();
        self.flesh_filter1_r.reset();
        self.flesh_filter2_l.reset();
        self.flesh_filter2_r.reset();
        self.parasite_filter_l.reset();
        self.parasite_filter_r.reset();
        self.sub_safety_filter_l.reset();
        self.sub_safety_filter_r.reset();
        self.bone_delay_l.fill(0.0);
        self.bone_delay_r.fill(0.0);
        self.asym_delay_r.fill(0.0);
        self.bone_write_ptr = 0;
        self.asym_write_ptr = 0;
        self.fast_env_l = 0.0;
        self.slow_env_l = 0.0;
        self.fast_env_r = 0.0;
        self.slow_env_r = 0.0;
        self.flesh_lfo_phase = 0.0;
        self.prng_state = self.seed;
    }
}

impl nullherz_traits::MidiResponder for MutatorProcessor {
    fn apply_midi(&mut self, _event: nullherz_traits::MidiEvent, _context: Option<&ProcessContext>) {}
}

impl nullherz_traits::SnapshotProvider for MutatorProcessor {}

impl AudioProcessor for MutatorProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn apply_command(&mut self, command: &ProcessorCommand) {
        if let Command::Mixer(nullherz_traits::MixerCommand::SetParam { target_id, param_id, value, .. }) = command
            && *target_id == self.id {
            self.set_parameter(*param_id, *value, 0);
        }
    }

    fn set_parameter(&mut self, param_id: u32, mut value: f32, _ramp_duration_samples: u32) {
        if !value.is_finite() { value = 0.0; }
        match param_id {
            0 => self.flesh = value.clamp(0.0, 1.0),
            1 => self.bone = value.clamp(0.0, 1.0),
            2 => self.teeth = value.clamp(0.0, 1.0),
            3 => self.parasite = value.clamp(0.0, 1.0),
            4 => self.asymmetry = value.clamp(0.0, 1.0),
            5 => self.abomination = value.clamp(0.0, 1.0),
            6 => {
                self.seed = value as u32;
                self.prng_state = self.seed;
            }
            7 => self.dry_wet = value.clamp(0.0, 1.0),
            8 => self.transient_lock = value > 0.5,
            9 => self.flux_lock = value > 0.5,
            10 => self.sub_mono_safety = value > 0.5,
            11 => self.input_trim_db = value.clamp(-24.0, 12.0),
            12 => self.output_trim_db = value.clamp(-24.0, 12.0),
            13 => self.character = (value as u32).clamp(0, 4),
            _ => {}
        }
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        match param_id {
            0 => self.flesh,
            1 => self.bone,
            2 => self.teeth,
            3 => self.parasite,
            4 => self.asymmetry,
            5 => self.abomination,
            6 => self.seed as f32,
            7 => self.dry_wet,
            8 => if self.transient_lock { 1.0 } else { 0.0 },
            9 => if self.flux_lock { 1.0 } else { 0.0 },
            10 => if self.sub_mono_safety { 1.0 } else { 0.0 },
            11 => self.input_trim_db,
            12 => self.output_trim_db,
            13 => self.character as f32,
            _ => 0.0,
        }
    }

    fn metadata(&self) -> Option<ProcessorMetadata> {
        let mut parameters = [nullherz_traits::ParameterMetadata {
            id: 0,
            name: [0; 32],
            min: 0.0,
            max: 1.0,
            default: 0.0,
        }; 16];

        let names: &[&[u8]] = &[
            b"FLESH", b"BONE", b"TEETH", b"PARASITE", b"ASYMMETRY", b"ABOMINATION",
            b"Seed", b"DryWet", b"TransientLock", b"FluxLock", b"SubMonoSafety",
            b"InputTrim", b"OutputTrim", b"Character",
        ];
        let mins = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -24.0, -24.0, 0.0];
        let maxs = [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 4294967295.0, 1.0, 1.0, 1.0, 1.0, 12.0, 12.0, 4.0];
        let defs = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1337.0, 0.5, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0];

        for (i, &name) in names.iter().enumerate() {
            parameters[i].id = i as u32;
            parameters[i].name[..name.len()].copy_from_slice(name);
            parameters[i].min = mins[i];
            parameters[i].max = maxs[i];
            parameters[i].default = defs[i];
        }

        Some(ProcessorMetadata {
            processor_id: self.id,
            num_parameters: 14,
            parameters,
        })
    }
}

pub struct MutatorFactory;

impl ProcessorFactory for MutatorFactory {
    fn type_id(&self) -> ProcessorTypeId {
        ProcessorTypeId::MUTATOR
    }

    fn name(&self) -> &'static str {
        "Mutator"
    }

    fn create_processor(&self, node_idx: u32, sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        Some(Box::new(MutatorProcessor::new(node_idx as u64, sample_rate)))
    }

    fn capabilities(&self) -> ProcessorCapability {
        ProcessorCapability {
            supports_parallel: true,
            is_instrument: false,
            has_midi_input: false,
            has_audio_input: true,
            has_audio_output: true,
        }
    }
}
