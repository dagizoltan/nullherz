use control_plane::Command;

pub const BUF_MASTER_L: usize = 0;
pub const BUF_MASTER_R: usize = 1;
pub const BUF_CUE_L: usize = 2;
pub const BUF_CUE_R: usize = 3;
pub const BUF_BROADCAST_L: usize = 4;
pub const BUF_BROADCAST_R: usize = 5;

pub struct MixerManager {
    next_node_id: u32,
    next_buffer_id: usize,
}

impl MixerManager {
    pub fn new() -> Self {
        Self {
            next_node_id: 1000,
            next_buffer_id: 8,
        }
    }

    pub fn create_studio_strip(&mut self, name: &str) -> Vec<Command> {
        let mut commands = Vec::new();
        let input_buf = self.next_buffer_id;
        self.next_buffer_id += 2; // Stereo input pair

        println!("Creating Studio Strip: {} with Input Buffers {}-{}", name, input_buf, input_buf + 1);

        // 1. Gain/Trim Node
        let gain_id = self.next_node_id;
        self.next_node_id += 1;
        commands.push(Command::SwapProcessor { node_idx: gain_id, processor_type_id: 2 }); // GainProcessor

        // 2. Fader Node (Routing to Master)
        // In a real implementation, we would send commands to create nodes in the graph
        // and wire them. Since we are in design/prototype phase, we mock the command list.

        commands
    }

    pub fn create_dj_deck(&mut self, deck_id: char) -> Vec<Command> {
        let commands = Vec::new();
        println!("Creating DJ Deck: {}", deck_id);
        // Resampler -> DJ EQ -> Fader -> Crossfader logic
        commands
    }
}
