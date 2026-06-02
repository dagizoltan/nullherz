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
    next_node_id: u32,
    next_buffer_id: u32,
}

impl MixerManager {
    pub fn new() -> Self {
        Self {
            next_node_id: 0,
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
        commands.push(Command::AddNode { node_idx: gain_id, processor_type_id: 2 });
        commands.push(Command::UpdateEdge { node_idx: gain_id, input_idx: 0, new_buffer_idx: input_buf as u32 });

        let mut prev_node = gain_id;
        for &fx_type in fx_ids {
            let fx_id = self.next_node_id;
            self.next_node_id += 1;
            let fx_buf = self.next_buffer_id;
            self.next_buffer_id += 1;

            commands.push(Command::AddNode { node_idx: fx_id, processor_type_id: fx_type });
            commands.push(Command::UpdateOutputEdge { node_idx: prev_node, output_idx: 0, new_buffer_idx: fx_buf });
            commands.push(Command::UpdateEdge { node_idx: fx_id, input_idx: 0, new_buffer_idx: fx_buf });
            prev_node = fx_id;
        }

        let fader_id = self.next_node_id;
        self.next_node_id += 1;
        commands.push(Command::AddNode { node_idx: fader_id, processor_type_id: 2 });
        commands.push(Command::UpdateEdge { node_idx: fader_id, input_idx: 0, new_buffer_idx: self.next_buffer_id });
        commands.push(Command::UpdateOutputEdge { node_idx: fader_id, output_idx: 0, new_buffer_idx: BUF_MASTER_L as u32 });

        commands
    }

    pub fn create_dj_deck(&mut self, deck_id: char, fx_ids: &[u32], bus_assignment: char) -> Vec<Command> {
        let mut commands = Vec::new();
        println!("Creating DJ Deck: {} assigned to Bus {}", deck_id, bus_assignment);

        let resample_id = self.next_node_id;
        self.next_node_id += 1;
        commands.push(Command::AddNode { node_idx: resample_id as u32, processor_type_id: 10 });

        for &fx_type in fx_ids {
            let fx_id = self.next_node_id;
            self.next_node_id += 1;
            commands.push(Command::AddNode { node_idx: fx_id as u32, processor_type_id: fx_type });
        }

        let eq_id = self.next_node_id;
        self.next_node_id += 1;
        commands.push(Command::AddNode { node_idx: eq_id as u32, processor_type_id: 11 });

        let target_l = if bus_assignment == 'A' { BUF_DJ_A_L } else { BUF_DJ_B_L };
        println!("Routing Deck {} to Buffer {}", deck_id, target_l);

        commands
    }

    pub fn create_crossfader(&mut self) -> Vec<Command> {
        let mut commands = Vec::new();
        let cf_id = self.next_node_id;
        self.next_node_id += 1;
        println!("Creating Master Crossfader (Node {})", cf_id);
        commands.push(Command::AddNode { node_idx: cf_id as u32, processor_type_id: 20 }); // Crossfader type
        commands
    }

    pub fn create_4channel_mixer(&mut self) -> Vec<Command> {
        let mut commands = Vec::new();
        println!("Building 4-Channel Mixer Architecture...");

        // Create 4 DJ Decks (A, B, C, D)
        let decks = ['A', 'B', 'C', 'D'];
        for &deck in &decks {
            let bus = if deck == 'A' || deck == 'C' { 'A' } else { 'B' };
            commands.extend(self.create_dj_deck(deck, &[1], bus));
        }

        // Create a Summing Node to mix everything to Master
        let sum_id = self.next_node_id;
        self.next_node_id += 1;
        commands.push(Command::AddNode { node_idx: sum_id, processor_type_id: 30 }); // Summing processor

        // Route buses to Summing Node
        commands.push(Command::UpdateEdge { node_idx: sum_id, input_idx: 0, new_buffer_idx: BUF_DJ_A_L as u32 });
        commands.push(Command::UpdateEdge { node_idx: sum_id, input_idx: 1, new_buffer_idx: BUF_DJ_B_L as u32 });

        // Output Sum to Master
        commands.push(Command::UpdateOutputEdge { node_idx: sum_id, output_idx: 0, new_buffer_idx: BUF_MASTER_L as u32 });

        commands
    }
}
