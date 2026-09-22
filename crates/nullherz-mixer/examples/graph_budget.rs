//! What a console costs in the two address spaces, and which one runs out first.
//!
//! `MAX_NODES` (128) and `MAX_BUFFERS` (240) are separate ceilings, and which
//! one binds depends entirely on the STRIP SHAPE:
//!
//! * A chain that processes IN PLACE — the DJ deck strip — spends many nodes
//!   against few buffers.
//! * A chain that gives every node its own output pair spends buffers fast.
//!
//! `bench_studio_scale` uses the second shape (to reach a high node count inside
//! the buffer ceiling) and concluded buffers bind first. That conclusion belongs
//! to that shape, not to the engine. This prints the real console's ratio so the
//! extrapolation is done against something the product actually builds.
//!
//! Run: cargo run -p nullherz-mixer --example graph_budget

use nullherz_mixer::MixerManager;
use nullherz_traits::{MAX_BUFFERS, MAX_NODES};

fn report(label: &str, nodes: u32, buffers: u32) {
    // Buffer ids start at 12 (0..11 are the reserved master/cue/bus pairs).
    let per_node = buffers as f64 / nodes as f64;
    let nodes_at_buffer_cap = (MAX_BUFFERS as f64 / per_node) as u32;
    println!("{label}");
    println!("  nodes                 : {nodes} of {MAX_NODES}");
    println!("  buffers               : {buffers} of {MAX_BUFFERS}");
    println!("  buffers per node      : {per_node:.2}");
    if nodes_at_buffer_cap >= MAX_NODES as u32 {
        println!("  binding ceiling       : MAX_NODES — buffers would allow {nodes_at_buffer_cap} nodes");
    } else {
        println!("  binding ceiling       : MAX_BUFFERS — it stops the graph at ~{nodes_at_buffer_cap} nodes");
    }
    let scale = (MAX_NODES as f64 / nodes as f64).min(MAX_BUFFERS as f64 / buffers as f64);
    println!("  room for              : {scale:.1}x this console\n");
}

fn main() {
    let mut mixer = MixerManager::new();
    let cmds = mixer.create_4channel_mixer();
    let nodes = mixer.id_allocator.current_node_id();
    let buffers = mixer.id_allocator.current_buffer_id();
    println!("\n{} topology commands\n", cmds.len());
    report("4-deck DJ console (real strips, processed in place)", nodes, buffers);

    // One more deck's worth, to isolate the marginal cost of a strip.
    let mut m2 = MixerManager::new();
    let _ = m2.create_4channel_mixer();
    let n0 = m2.id_allocator.current_node_id();
    let b0 = m2.id_allocator.current_buffer_id();
    let _ = m2.create_dj_deck('E', &[1], 'A');
    let dn = m2.id_allocator.current_node_id() - n0;
    let db = m2.id_allocator.current_buffer_id() - b0;
    println!("marginal cost of ONE more deck strip: {dn} nodes, {db} buffers");
    if db > 0 {
        println!("  -> {} more strips before MAX_NODES, {} before MAX_BUFFERS",
            (MAX_NODES as u32 - n0) / dn.max(1),
            (MAX_BUFFERS as u32 - b0) / db.max(1));
    }
}
