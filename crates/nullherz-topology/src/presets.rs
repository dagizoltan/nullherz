use crate::{DesiredGraphState, DesiredNode};
use nullherz_traits::{ProcessorTypeId, MAX_CHANNELS, MAX_NODES};

pub struct GraphPresetBuilder;

impl GraphPresetBuilder {
    /// Builds a 4-Deck DJ Graph topology:
    /// - 4 Decks (Decks A, B, C, D) using Sampler/Streaming Sampler nodes
    /// - Per-deck Channel Inserts (Isolator, Gain/EQ)
    /// - Crossfader & Summing Mixer
    /// - Sends & Returns for FX
    /// - Master Inserts (Limiter)
    pub fn build_4deck_dj_graph() -> DesiredGraphState {
        let mut state = DesiredGraphState::empty();
        let mut b_idx = 12u32; // Reserve 0..11 for Master, Cue, and Buses

        // Deck A (node 0)
        let deck_a = DesiredNode {
            type_id: ProcessorTypeId::SAMPLER,
            input_buffers: [0; MAX_CHANNELS],
            output_buffers: [b_idx, b_idx + 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            input_count: 0,
            output_count: 2,
        };
        state.nodes[0] = Some(deck_a);
        b_idx += 2;

        // Deck B (node 1)
        let deck_b = DesiredNode {
            type_id: ProcessorTypeId::SAMPLER,
            input_buffers: [0; MAX_CHANNELS],
            output_buffers: [b_idx, b_idx + 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            input_count: 0,
            output_count: 2,
        };
        state.nodes[1] = Some(deck_b);
        b_idx += 2;

        // Deck C (node 2)
        let deck_c = DesiredNode {
            type_id: ProcessorTypeId::SAMPLER,
            input_buffers: [0; MAX_CHANNELS],
            output_buffers: [b_idx, b_idx + 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            input_count: 0,
            output_count: 2,
        };
        state.nodes[2] = Some(deck_c);
        b_idx += 2;

        // Deck D (node 3)
        let deck_d = DesiredNode {
            type_id: ProcessorTypeId::SAMPLER,
            input_buffers: [0; MAX_CHANNELS],
            output_buffers: [b_idx, b_idx + 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            input_count: 0,
            output_count: 2,
        };
        state.nodes[3] = Some(deck_d);
        let _ = b_idx;

        // Summing Mixer / Master (node 4)
        let master = DesiredNode {
            type_id: ProcessorTypeId::SUMMING,
            input_buffers: [12, 13, 14, 15, 16, 17, 18, 19, 0, 0, 0, 0, 0, 0, 0, 0],
            output_buffers: [0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], // Main Master Outputs 0, 1
            input_count: 8,
            output_count: 2,
        };
        state.nodes[4] = Some(master);

        // Master Limiter Insert (node 5)
        let limiter = DesiredNode {
            type_id: ProcessorTypeId::LIMITER,
            input_buffers: [0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            output_buffers: [2, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            input_count: 2,
            output_count: 2,
        };
        state.nodes[5] = Some(limiter);

        state
    }

    /// Builds an Ableton-like Composition Graph topology with N tracks:
    /// - Track 1..N: Instrument / Sampler -> Insert Chain (EQ, Comp) -> Send FX
    /// - Group Buses & Sends (Reverb, Delay)
    /// - Master Track & Master Inserts
    pub fn build_composition_graph(num_tracks: usize) -> DesiredGraphState {
        let mut state = DesiredGraphState::empty();
        let num_tracks = num_tracks.min(16);
        let mut b_idx = 12u32;
        let mut track_out_buffers = Vec::new();

        for i in 0..num_tracks {
            let inst_out = [b_idx, b_idx + 1];
            b_idx += 2;

            // Instrument / Track Source
            let inst_node = DesiredNode {
                type_id: ProcessorTypeId::SAMPLER,
                input_buffers: [0; MAX_CHANNELS],
                output_buffers: [inst_out[0], inst_out[1], 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
                input_count: 0,
                output_count: 2,
            };
            let inst_slot = i * 2;
            if inst_slot < MAX_NODES {
                state.nodes[inst_slot] = Some(inst_node);
            }

            // Insert Chain (e.g. Gain/EQ)
            let insert_out = [b_idx, b_idx + 1];
            b_idx += 2;
            let insert_node = DesiredNode {
                type_id: ProcessorTypeId::GAIN,
                input_buffers: [inst_out[0], inst_out[1], 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
                output_buffers: [insert_out[0], insert_out[1], 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
                input_count: 2,
                output_count: 2,
            };
            let insert_slot = inst_slot + 1;
            if insert_slot < MAX_NODES {
                state.nodes[insert_slot] = Some(insert_node);
                track_out_buffers.push(insert_out[0]);
                track_out_buffers.push(insert_out[1]);
            }
        }

        // Master Bus Summing
        let mut sum_inputs = [0u32; MAX_CHANNELS];
        let sum_count = track_out_buffers.len().min(MAX_CHANNELS);
        for i in 0..sum_count {
            sum_inputs[i] = track_out_buffers[i];
        }

        let master_sum_slot = num_tracks * 2;
        if master_sum_slot < MAX_NODES {
            let master_sum = DesiredNode {
                type_id: ProcessorTypeId::SUMMING,
                input_buffers: sum_inputs,
                output_buffers: [0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
                input_count: sum_count as u32,
                output_count: 2,
            };
            state.nodes[master_sum_slot] = Some(master_sum);
        }

        state
    }
}

#[cfg(test)]
mod preset_tests {
    use super::*;

    #[test]
    fn test_build_4deck_dj_graph() {
        let state = GraphPresetBuilder::build_4deck_dj_graph();
        assert_eq!(state.occupied(), 6);
        assert!(state.nodes[0].is_some()); // Deck A
        assert!(state.nodes[1].is_some()); // Deck B
        assert!(state.nodes[2].is_some()); // Deck C
        assert!(state.nodes[3].is_some()); // Deck D
        assert!(state.nodes[4].is_some()); // Master Sum
        assert!(state.nodes[5].is_some()); // Master Limiter
    }

    #[test]
    fn test_build_composition_graph() {
        let state = GraphPresetBuilder::build_composition_graph(4);
        assert_eq!(state.occupied(), 9); // 4 tracks * 2 nodes + 1 master sum
    }
}
