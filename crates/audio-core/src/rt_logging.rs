use std::sync::atomic::{AtomicUsize, Ordering, AtomicBool};
use std::cell::UnsafeCell;

#[derive(Debug, Clone, Copy)]
pub enum RtLogLevel {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Copy)]
pub struct RtLogEntry {
    pub level: RtLogLevel,
    pub message: [u8; 64],
    pub length: usize,
    pub timestamp: u64,
}

pub struct RtLogSlot {
    pub entry: UnsafeCell<RtLogEntry>,
    pub written: AtomicBool,
}

pub struct RtLogger {
    buffer: Box<[RtLogSlot]>,
    head: AtomicUsize,
    tail: AtomicUsize,
    capacity: usize,
}

unsafe impl Sync for RtLogger {}

impl RtLogger {
    pub fn new(capacity: usize) -> Self {
        let mut buffer = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            buffer.push(RtLogSlot {
                entry: UnsafeCell::new(RtLogEntry {
                    level: RtLogLevel::Info,
                    message: [0; 64],
                    length: 0,
                    timestamp: 0,
                }),
                written: AtomicBool::new(false),
            });
        }
        Self {
            buffer: buffer.into_boxed_slice(),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            capacity,
        }
    }

    pub fn log(&self, level: RtLogLevel, msg: &str, timestamp: u64) {
        let mut tail = self.tail.load(Ordering::Relaxed);
        let idx = loop {
            let head = self.head.load(Ordering::Acquire);

            if (tail + 1) % self.capacity == head {
                return; // Buffer full, discard log to preserve RT safety
            }

            match self.tail.compare_exchange_weak(
                tail,
                (tail + 1) % self.capacity,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break tail,
                Err(actual) => tail = actual,
            }
        };

        let slot = &self.buffer[idx];
        let entry_ptr = slot.entry.get();
        unsafe {
            (*entry_ptr).level = level;
            let bytes = msg.as_bytes();
            let len = bytes.len().min(64);
            (&mut (*entry_ptr).message)[..len].copy_from_slice(&bytes[..len]);
            (*entry_ptr).length = len;
            (*entry_ptr).timestamp = timestamp;
        }

        slot.written.store(true, Ordering::Release);
    }

    pub fn pop(&self) -> Option<RtLogEntry> {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);

        if head == tail {
            return None;
        }

        let slot = &self.buffer[head];
        if !slot.written.load(Ordering::Acquire) {
            return None; // Slot is reserved but not yet fully written by producer
        }

        let entry = unsafe { *slot.entry.get() };
        slot.written.store(false, Ordering::Release);
        self.head.store((head + 1) % self.capacity, Ordering::Release);
        Some(entry)
    }
}
