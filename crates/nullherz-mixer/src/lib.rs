use control_plane::Command;

pub const BUF_MASTER_L: usize = 0;
pub const BUF_MASTER_R: usize = 1;
pub const BUF_CUE_L: usize = 2;
pub const BUF_CUE_R: usize = 3;
pub const BUF_BROADCAST_L: usize = 4;
pub const BUF_BROADCAST_R: usize = 5;
pub const BUF_DJ_A_L: usize = 8;
pub const BUF_DJ_A_R: usize = 9;
pub const BUF_DJ_B_L: usize = 10;
pub const BUF_DJ_B_R: usize = 11;

pub struct MixerManager {
    next_node_id: usize,
    next_buffer_id: usize,
}

impl MixerManager {
    pub fn new() -> Self {
        Self {
            next_node_id: 1000,
            next_buffer_id: 12,
        }
    }

    pub fn create_studio_strip(&mut self, name: &str, fx_ids: &[u32]) -> Vec<Command> {
        let mut commands = Vec::new();
        let input_buf = self.next_buffer_id;
        self.next_buffer_id += 2;

        println!("Creating Studio Strip: {} (Input: {}-{})", name, input_buf, input_buf + 1);

        let gain_id = self.next_node_id;
        self.next_node_id += 1;
        commands.push(Command::SwapProcessor { node_idx: gain_id as u32, processor_type_id: 2 });

        for &fx_type in fx_ids {
            let fx_id = self.next_node_id;
            self.next_node_id += 1;
            commands.push(Command::SwapProcessor { node_idx: fx_id as u32, processor_type_id: fx_type });
        }

        let fader_id = self.next_node_id;
        self.next_node_id += 1;
        commands.push(Command::SwapProcessor { node_idx: fader_id as u32, processor_type_id: 2 });

        commands
    }

    pub fn create_dj_deck(&mut self, deck_id: char, fx_ids: &[u32], bus_assignment: char) -> Vec<Command> {
        let mut commands = Vec::new();
        println!("Creating DJ Deck: {} assigned to Bus {}", deck_id, bus_assignment);

        let resample_id = self.next_node_id;
        self.next_node_id += 1;
        commands.push(Command::SwapProcessor { node_idx: resample_id as u32, processor_type_id: 10 });

        for &fx_type in fx_ids {
            let fx_id = self.next_node_id;
            self.next_node_id += 1;
            commands.push(Command::SwapProcessor { node_idx: fx_id as u32, processor_type_id: fx_type });
        }

        let eq_id = self.next_node_id;
        self.next_node_id += 1;
        commands.push(Command::SwapProcessor { node_idx: eq_id as u32, processor_type_id: 11 });

        let target_l = if bus_assignment == 'A' { BUF_DJ_A_L } else { BUF_DJ_B_L };
        println!("Routing Deck {} to Buffer {}", deck_id, target_l);

        commands
    }

    pub fn create_crossfader(&mut self) -> Vec<Command> {
        let mut commands = Vec::new();
        let cf_id = self.next_node_id;
        self.next_node_id += 1;
        println!("Creating Master Crossfader (Node {})", cf_id);
        commands.push(Command::SwapProcessor { node_idx: cf_id as u32, processor_type_id: 20 }); // Crossfader type
        commands
    }
}
