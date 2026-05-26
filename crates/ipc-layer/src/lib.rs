use std::sync::atomic::{AtomicUsize, Ordering};
use std::cell::UnsafeCell;
use std::marker::PhantomData;

/// A lock-free, Single-Producer Single-Consumer (SPSC) ring buffer
/// that can reside in shared memory.
///
/// The layout is designed to be stable and transmutable from a raw byte buffer.
#[repr(C)]
pub struct ShmRingBuffer<T> {
    head: AtomicUsize,
    tail: AtomicUsize,
    capacity: usize,
    _marker: PhantomData<T>,
    // The buffer follows in memory
}

impl<T> ShmRingBuffer<T> {
    /// Calculate the size required for a ShmRingBuffer of given capacity.
    pub fn size_required(capacity: usize) -> usize {
        std::mem::size_of::<Self>() + capacity * std::mem::size_of::<UnsafeCell<Option<T>>>()
    }

    /// Initialize a ShmRingBuffer in a provided raw memory pointer.
    pub unsafe fn init(ptr: *mut u8, capacity: usize) -> *mut Self {
        let rb_ptr = ptr as *mut Self;
        std::ptr::write(&mut (*rb_ptr).head, AtomicUsize::new(0));
        std::ptr::write(&mut (*rb_ptr).tail, AtomicUsize::new(0));
        (*rb_ptr).capacity = capacity;

        // Initialize the buffer area to None
        let buffer_ptr = rb_ptr.add(1) as *mut UnsafeCell<Option<T>>;
        for i in 0..capacity {
            std::ptr::write(buffer_ptr.add(i), UnsafeCell::new(None));
        }

        rb_ptr
    }

    fn buffer_ptr(&self) -> *const UnsafeCell<Option<T>> {
        // self.add(1) increments by size_of::<Self>()
        unsafe { (self as *const Self).add(1) as *const UnsafeCell<Option<T>> }
    }

    pub fn push(&self, item: T) -> Result<(), T> {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Relaxed);

        if (tail + 1) % self.capacity == head {
            return Err(item);
        }

        unsafe {
            let cell_ptr = self.buffer_ptr().add(tail) as *mut UnsafeCell<Option<T>>;
            std::ptr::write((*cell_ptr).get(), Some(item));
        }

        self.tail.store((tail + 1) % self.capacity, Ordering::Release);
        Ok(())
    }

    pub fn pop(&self) -> Option<T> {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);

        if head == tail {
            return None;
        }

        let item = unsafe {
            let cell_ptr = self.buffer_ptr().add(head) as *mut UnsafeCell<Option<T>>;
            (*(*cell_ptr).get()).take()
        };

        self.head.store((head + 1) % self.capacity, Ordering::Release);
        item
    }
}

// Keep the Arc-based version for internal use as well
pub struct RingBuffer<T> {
    buffer: Box<[UnsafeCell<Option<T>>]>,
    head: AtomicUsize,
    tail: AtomicUsize,
    capacity: usize,
}

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
        let arc = std::sync::Arc::new(self);
        (
            Producer { inner: arc.clone() },
            Consumer { inner: arc },
        )
    }
}

pub struct Producer<T> {
    inner: std::sync::Arc<RingBuffer<T>>,
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
    inner: std::sync::Arc<RingBuffer<T>>,
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
    fn test_shm_ring_buffer() {
        let capacity = 4;
        let size = ShmRingBuffer::<i32>::size_required(capacity);
        let mut mem = vec![0u8; size];

        let rb_ptr = unsafe { ShmRingBuffer::<i32>::init(mem.as_mut_ptr(), capacity) };
        let rb = unsafe { &*rb_ptr };

        rb.push(10).unwrap();
        rb.push(20).unwrap();
        assert_eq!(rb.pop(), Some(10));
        assert_eq!(rb.pop(), Some(20));
        assert_eq!(rb.pop(), None);
    }
}
