//! Golden-hash render regression: a fixed DSP chain, fixed commands, fixed
//! input must produce bit-identical output forever. Any change to a kernel —
//! intended or accidental — flips the hash and must be acknowledged by
//! updating the constant in the same commit that changes the sound.
//!
//! (Hash is over IEEE-754 bit patterns, so this is exact, not approximate.
//! CI and dev machines share the x86_64 target; if a new target ever joins,
//! give it its own constant.)

use nullherz_traits::{AudioConfig, ProcessContext, SignalProcessor, Transport, AudioProcessor};

fn fnv1a(acc: u64, bytes: &[u8]) -> u64 {
    let mut h = acc;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn hash_block(acc: u64, block: &[f32]) -> u64 {
    let mut h = acc;
    for &s in block {
        h = fnv1a(h, &s.to_bits().to_le_bytes());
    }
    h
}

/// Deterministic pseudo-noise input (LCG), same sequence every run.
fn test_signal(seed: &mut u64) -> f32 {
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    (((*seed >> 33) as u32) as f32 / u32::MAX as f32) * 2.0 - 1.0
}

fn render_chain_hash() -> u64 {
    const SR: f32 = 44100.0;
    const BLOCK: usize = 256;
    const BLOCKS: usize = 64;

    let mut osc = crate::WavetableProcessor::new(1, SR);
    let mut filt = crate::SimdBiquadProcessor::new(2, audio_dsp::BiquadCoefficients::linkwitz_riley_lp(1000.0, SR));
    let mut gain = crate::GainProcessor::new(3, 0.5);
    for p in [&mut osc as &mut dyn AudioProcessor, &mut filt, &mut gain] {
        p.setup(AudioConfig { sample_rate: SR, block_size: BLOCK });
    }
    // Fixed parameter changes mid-render exercise the command path too.
    osc.set_parameter(0, 220.0, 0);

    let mut transport = Transport {
        bpm: 120.0,
        beat_position: 0.0,
        is_playing: true,
        sample_rate: SR,
        absolute_samples: 0,
        system_time_ns: 0,
        device_time_ns: 0,
    };

    let mut seed = 0x6e756c6c6865727au64; // "nullherz"
    let mut hash = 0xcbf29ce484222325u64; // FNV offset basis

    let mut buf_a = [0.0f32; BLOCK];
    let mut buf_b = [0.0f32; BLOCK];
    for block_idx in 0..BLOCKS {
        if block_idx == 32 {
            gain.set_parameter(0, 0.25, 0);
            osc.set_parameter(0, 440.0, 0);
        }
        for s in buf_a.iter_mut() {
            *s = test_signal(&mut seed);
        }
        let mut ctx = ProcessContext {
            transport: Some(&transport),
            host: None,
            sub_block_offset: 0,
            is_last_sub_block: true,
        };
        // osc renders over the noise (its own output), then filter, then gain.
        {
            let inp: [&[f32]; 1] = [&buf_a];
            let mut out: [&mut [f32]; 1] = [&mut buf_b];
            osc.process(&inp, &mut out, &mut ctx);
        }
        {
            let inp: [&[f32]; 1] = [&buf_b];
            let mut out: [&mut [f32]; 1] = [&mut buf_a];
            filt.process(&inp, &mut out, &mut ctx);
        }
        {
            let inp: [&[f32]; 1] = [&buf_a];
            let mut out: [&mut [f32]; 1] = [&mut buf_b];
            gain.process(&inp, &mut out, &mut ctx);
        }
        hash = hash_block(hash, &buf_b);
        transport.absolute_samples += BLOCK as u64;
    }
    hash
}

#[test]
fn golden_render_is_bit_stable() {
    let h = render_chain_hash();
    // To regenerate after an INTENTIONAL sound change:
    //   cargo test -p nullherz-processors golden_render -- --nocapture
    // and copy the printed value here, in the same commit as the DSP change.
    println!("golden render hash: {:#018x}", h);
    const GOLDEN: u64 = 0x5dbc9e3eb4d51f2d;
    assert_eq!(
        h, GOLDEN,
        "DSP output changed bit-for-bit. If intentional, update GOLDEN in this commit; if not, you just caught a regression."
    );
}
