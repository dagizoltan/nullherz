use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::cell::UnsafeCell;

/// A lock-free, Single-Producer Single-Consumer (SPSC) ring buffer.
/// Uses UnsafeCell to allow mutation through shared references (RT-safe).
pub struct RingBuffer<T> {
    buffer: Box<[UnsafeCell<Option<T>>]>,
    head: AtomicUsize, // Written by consumer
    tail: AtomicUsize, // Written by producer
    capacity: usize,
}

// Safety: RingBuffer is Sync if T is Send, because we ensure only one thread
// accesses each UnsafeCell at a time (SPSC).
unsafe impl<T: Send> Sync for RingBuffer<T> {}

impl<T> RingBuffer<T> {
    pub fn new(capacity: usize) -> Self {
        let mut buffer = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            buffer.push(UnsafeCell::new(None));
        }
        Self {
            buffer: buffer.into_boxed_slice(),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            capacity,
        }
    }

    pub fn split(self) -> (Producer<T>, Consumer<T>) {
        let arc = Arc::new(self);
        (
            Producer { inner: arc.clone() },
            Consumer { inner: arc },
        )
    }
}

pub struct Producer<T> {
    inner: Arc<RingBuffer<T>>,
}

impl<T> Producer<T> {
    pub fn push(&mut self, item: T) -> Result<(), T> {
        let head = self.inner.head.load(Ordering::Acquire);
        let tail = self.inner.tail.load(Ordering::Relaxed);

        if (tail + 1) % self.inner.capacity == head {
            return Err(item);
        }

        unsafe {
            let cell_ptr = self.inner.buffer[tail].get();
            std::ptr::write(cell_ptr, Some(item));
        }

        self.inner.tail.store((tail + 1) % self.inner.capacity, Ordering::Release);
        Ok(())
    }
}

pub struct Consumer<T> {
    inner: Arc<RingBuffer<T>>,
}

impl<T> Consumer<T> {
    pub fn pop(&mut self) -> Option<T> {
        let head = self.inner.head.load(Ordering::Relaxed);
        let tail = self.inner.tail.load(Ordering::Acquire);

        if head == tail {
            return None;
        }

        let item = unsafe {
            let cell_ptr = self.inner.buffer[head].get();
            (*cell_ptr).take()
        };

        self.inner.head.store((head + 1) % self.inner.capacity, Ordering::Release);
        item
    }

    pub fn peek(&self) -> Option<&T> {
        let head = self.inner.head.load(Ordering::Relaxed);
        let tail = self.inner.tail.load(Ordering::Acquire);

        if head == tail {
            return None;
        }

        unsafe {
            let cell_ptr = self.inner.buffer[head].get();
            (*cell_ptr).as_ref()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spsc_ring_buffer() {
        let rb = RingBuffer::new(4);
        let (mut prod, mut cons) = rb.split();

        prod.push(1).unwrap();
        prod.push(2).unwrap();
        prod.push(3).unwrap();
        assert!(prod.push(4).is_err());

        assert_eq!(cons.peek(), Some(&1));
        assert_eq!(cons.pop(), Some(1));
        assert_eq!(cons.pop(), Some(2));
        assert_eq!(cons.pop(), Some(3));
        assert_eq!(cons.pop(), None);
    }
}
