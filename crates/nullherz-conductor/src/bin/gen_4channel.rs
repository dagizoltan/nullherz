use nullherz_mixer::MixerManager;
use control_plane::TimestampedCommand;

fn main() {
    let mut mixer = MixerManager::new();
    let commands = mixer.create_4channel_mixer();

    let mut nodes_json = Vec::new();
    for cmd in &commands {
        match cmd {
            control_plane::Command::AddNode { .. } => {
                nodes_json.push(serde_json::json!({ "inputs": [], "outputs": [] }));
            }
            control_plane::Command::UpdateEdge { node_idx, new_buffer_idx, .. } => {
                let node = nodes_json.get_mut(*node_idx as usize).unwrap();
                node["inputs"].as_array_mut().unwrap().push(serde_json::json!(new_buffer_idx));
            }
            control_plane::Command::UpdateOutputEdge { node_idx, new_buffer_idx, .. } => {
                let node = nodes_json.get_mut(*node_idx as usize).unwrap();
                node["outputs"].as_array_mut().unwrap().push(serde_json::json!(new_buffer_idx));
            }
            _ => {}
        }
    }

    let graph = serde_json::json!({ "nodes": nodes_json });
    println!("{}", serde_json::to_string_pretty(&graph).unwrap());
    std::fs::write("4channel_graph.json", serde_json::to_string_pretty(&graph).unwrap()).unwrap();
}
