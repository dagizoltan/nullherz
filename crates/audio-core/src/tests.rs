use crate::graph::{ProcessorGraph, ConstantProcessor};
use crate::engine::AudioEngine;
use crate::backends::{ThreadedBackend, AudioBackend};
use crate::processors::GainProcessor;
use control_plane::{Command, TimestampedCommand};
use ipc_layer::RingBuffer;
use std::time::Duration;
use crate::traits::AudioProcessor;

#[test]
fn test_node_limit() {
    let mut graph = ProcessorGraph::new();
    struct Pass { }
    impl AudioProcessor for Pass { fn process(&mut self, _: &[&[f32]], _: &mut [&mut [f32]]) {} }
    for _ in 0..100 {
        graph.add_node(Box::new(Pass {}), vec![], vec![]);
    }
    assert!(graph.nodes.len() <= 64);
}

#[test]
fn test_sample_accurate_rewiring() {
    let rb = RingBuffer::new(1024);
    let (mut prod, cons) = rb.split();
    let garbage_rb = RingBuffer::new(32);
    let (garbage_prod, _) = garbage_rb.split();
    let tel_rb = RingBuffer::new(1024);
    let (tel_prod, _) = tel_rb.split();

    let mut graph = ProcessorGraph::new();
    graph.pool = None;
    graph.add_node(Box::new(ConstantProcessor { val: 1.0 }), vec![], vec![2]);
    graph.add_node(Box::new(ConstantProcessor { val: 2.0 }), vec![], vec![3]);
    graph.add_node(Box::new(GainProcessor::new(1, 1.0)), vec![2], vec![0]);

    let mut engine = AudioEngine::new(cons, garbage_prod, tel_prod, Box::new(graph));

    let mut outputs = [[0.0f32; 128]; 2];
    {
        let (ch0, ch1) = outputs.split_at_mut(1);
        let mut out_refs = [&mut ch0[0][..], &mut ch1[0][..]];
        engine.process_block(&[], &mut out_refs, 10);
    }
    assert_eq!(outputs[0][0], 1.0);

    let _ = prod.push(TimestampedCommand {
        timestamp_samples: 15,
        command: Command::UpdateEdge { node_idx: 2, input_idx: 0, new_buffer_idx: 3 },
    });

    {
        let (ch0, ch1) = outputs.split_at_mut(1);
        let mut out_refs = [&mut ch0[0][..], &mut ch1[0][..]];
        engine.process_block(&[], &mut out_refs, 10);
    }
    assert_eq!(outputs[0][0], 1.0);
    assert_eq!(outputs[0][4], 1.0);
    assert_eq!(outputs[0][5], 2.0);
    assert_eq!(outputs[0][9], 2.0);
}

#[test]
fn test_backend_hot_swap() {
    let rb = RingBuffer::new(1024);
    let (_, cons) = rb.split();
    let garbage_rb = RingBuffer::new(32);
    let (garbage_prod, _) = garbage_rb.split();
    let tel_rb = RingBuffer::new(1024);
    let (tel_prod, _) = tel_rb.split();

    let mut graph = ProcessorGraph::new();
    graph.add_node(Box::new(ConstantProcessor { val: 1.0 }), vec![], vec![0]);
    let engine = AudioEngine::new(cons, garbage_prod, tel_prod, Box::new(graph));

    let mut backend1 = ThreadedBackend::new();
    backend1.start(engine).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    let engine_returned = backend1.stop().expect("Should return engine");
    assert!(engine_returned.sample_counter > 0);

    let mut backend2 = ThreadedBackend::new();
    let prev_samples = engine_returned.sample_counter;
    backend2.start(engine_returned).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    let engine_final = backend2.stop().expect("Should return engine");
    assert!(engine_final.sample_counter > prev_samples);
}

#[test]
fn test_burst_commands() {
    let rb = RingBuffer::new(1024);
    let (mut prod, cons) = rb.split();
    let garbage_rb = RingBuffer::new(32);
    let (garbage_prod, _) = garbage_rb.split();
    let tel_rb = RingBuffer::new(1024);
    let (tel_prod, _) = tel_rb.split();

    let mut graph = ProcessorGraph::new();
    graph.pool = None;
    graph.add_node(Box::new(GainProcessor::new(1, 0.0)), vec![0], vec![1]);
    let mut engine = AudioEngine::new(cons, garbage_prod, tel_prod, Box::new(graph));

    let _ = prod.push(TimestampedCommand {
        timestamp_samples: 5,
        command: Command::SetParam { target_id: 1, param_id: 0, value: 0.5, ramp_duration_samples: 0 },
    });
    let _ = prod.push(TimestampedCommand {
        timestamp_samples: 5,
        command: Command::SetParam { target_id: 1, param_id: 0, value: 1.0, ramp_duration_samples: 0 },
    });

    let mut outputs = [[0.0f32; 128]; 2];
    let inputs = [[1.0f32; 128]; 1];
    let (ch0, ch1) = outputs.split_at_mut(1);
    let mut out_refs = [&mut ch0[0][..], &mut ch1[0][..]];
    let in_refs = [&inputs[0][..]];

    engine.process_block(&in_refs, &mut out_refs, 10);

    assert_eq!(outputs[1][0], 0.0);
    assert_eq!(outputs[1][4], 0.0);
    assert_eq!(outputs[1][5], 1.0);
    assert_eq!(outputs[1][9], 1.0);
}

#[test]
fn test_node_telemetry() {
    let rb = RingBuffer::new(1024);
    let (_, cons) = rb.split();
    let garbage_rb = RingBuffer::new(32);
    let (garbage_prod, _) = garbage_rb.split();
    let tel_rb = RingBuffer::new(1024);
    let (tel_prod, mut tel_cons) = tel_rb.split();

    let mut graph = ProcessorGraph::new();
    graph.pool = None;
    graph.add_node(Box::new(ConstantProcessor { val: 0.5 }), vec![], vec![0]);
    let mut engine = AudioEngine::new(cons, garbage_prod, tel_prod, Box::new(graph));

    let mut outputs = [[0.0f32; 128]; 1];
    let mut out_refs = [&mut outputs[0][..]];
    engine.process_block(&[], &mut out_refs, 128);

    let tel = tel_cons.pop().expect("Should have telemetry");
    assert!(tel.node_load_ns[0] > 0);
    assert_eq!(tel.buffer_levels[0], 0.5);
}

#[test]
fn test_stage_grouping() {
    let mut graph = ProcessorGraph::new();
    struct Pass { }
    impl AudioProcessor for Pass { fn process(&mut self, _: &[&[f32]], _: &mut [&mut [f32]]) {} }
    graph.add_node(Box::new(Pass {}), vec![1], vec![2]);
    graph.add_node(Box::new(Pass {}), vec![1], vec![3]);
    graph.add_node(Box::new(Pass {}), vec![2, 3], vec![4]);
    graph.commit_graph();
    let topo = graph.current_topology();
    assert_eq!(topo.num_stages, 2);
}
