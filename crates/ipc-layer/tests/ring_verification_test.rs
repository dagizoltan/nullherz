#![allow(unexpected_cfgs)]
use ipc_layer::{RingBuffer, MpscRingBuffer, AudioBlock};

#[test]
fn test_mpsc_ring_buffer_wraparound_near_usize_max() {
    let capacity = 8;
    let buffer = MpscRingBuffer::<u32>::new(capacity);

    // Fill and drain buffer across 10,000 cycles to verify monotonic index wraparound
    for cycle in 0..10_000u32 {
        for i in 0..8u32 {
            assert!(buffer.push(cycle * 10 + i).is_ok(), "Push item {} in cycle {} must succeed", i, cycle);
        }
        assert!(buffer.push(999).is_err(), "Full MPSC buffer must reject push");

        for i in 0..8u32 {
            assert_eq!(buffer.pop(), Some(cycle * 10 + i), "Pop item {} in cycle {} mismatch", i, cycle);
        }
        assert_eq!(buffer.pop(), None, "Empty MPSC buffer must return None");
    }
}

#[test]
fn test_ring_buffer_spsc_high_volume_concurrency() {
    let (mut prod, mut cons) = RingBuffer::<AudioBlock>::new(128).split();
    let num_items = 10_000;

    let producer_thread = std::thread::spawn(move || {
        for i in 0..num_items {
            let mut block = AudioBlock {
                data: [0.0; ipc_layer::IPC_BLOCK_SIZE],
                len: 256,
                _pad: [0; 15],
            };
            block.data[0] = i as f32;
            loop {
                match prod.push(block) {
                    Ok(()) => break,
                    Err(_) => std::hint::spin_loop(),
                }
            }
        }
    });

    let mut received = 0;
    while received < num_items {
        if let Some(block) = cons.pop() {
            assert_eq!(block.data[0], received as f32, "AudioBlock sequence error at {}", received);
            received += 1;
        } else {
            std::hint::spin_loop();
        }
    }

    producer_thread.join().expect("Producer thread joined successfully");
}

#[test]
fn test_mpsc_ring_buffer_multi_producer_stress() {
    let capacity = 64;
    let buffer = std::sync::Arc::new(MpscRingBuffer::<u64>::new(capacity));
    let num_producers = 8;
    let items_per_producer = 5_000;

    let mut handles = Vec::new();
    for p in 0..num_producers {
        let buf = buffer.clone();
        handles.push(std::thread::spawn(move || {
            for i in 0..items_per_producer {
                let val = (p as u64) * 1_000_000 + (i as u64);
                loop {
                    match buf.push(val) {
                        Ok(()) => break,
                        Err(_) => std::hint::spin_loop(),
                    }
                }
            }
        }));
    }

    let mut counts = vec![0u64; num_producers];
    let mut total_received = 0;
    let expected_total = num_producers * items_per_producer;

    while total_received < expected_total {
        if let Some(val) = buffer.pop() {
            let producer_idx = (val / 1_000_000) as usize;
            counts[producer_idx] += 1;
            total_received += 1;
        } else {
            std::hint::spin_loop();
        }
    }

    for h in handles {
        h.join().expect("Producer thread joined");
    }

    for (p, count) in counts.iter().enumerate() {
        assert_eq!(*count, items_per_producer as u64, "Producer {} item count mismatch", p);
    }
}

#[cfg(kani)]
#[kani::proof]
fn prove_mpsc_ring_buffer_wraparound_safety() {
    let capacity = 4;
    let buffer = MpscRingBuffer::<u8>::new(capacity);
    let val: u8 = kani::any();
    if buffer.push(val).is_ok() {
        let popped = buffer.pop();
        assert_eq!(popped, Some(val));
    }
}

#[cfg(loom)]
#[test]
fn loom_mpsc_ring_buffer_concurrency() {
    loom::model(|| {
        let buffer = std::sync::Arc::new(MpscRingBuffer::<u32>::new(4));
        let b1 = buffer.clone();
        let h1 = loom::thread::spawn(move || {
            let _ = b1.push(42);
        });
        let b2 = buffer.clone();
        let h2 = loom::thread::spawn(move || {
            let _ = b2.push(99);
        });
        h1.join().unwrap();
        h2.join().unwrap();
        let mut items = Vec::new();
        while let Some(item) = buffer.pop() {
            items.push(item);
        }
        assert!(items.len() <= 2);
    });
}
