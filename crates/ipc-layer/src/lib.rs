use std::sync::atomic::{AtomicUsize, Ordering};
use std::cell::UnsafeCell;
use std::marker::PhantomData;
use std::alloc::Layout;

/// Alignment for SIMD (AVX-512 requires 64 bytes).
pub const SIMD_ALIGNMENT: usize = 64;

/// A SIMD-aligned audio block.
#[repr(C, align(64))]
#[derive(Clone, Copy)]
pub struct AudioBlock {
    pub data: [f32; 128], // Fixed size for predictability
}

/// A lock-free, Single-Producer Single-Consumer (SPSC) ring buffer
/// that can reside in shared memory.
#[repr(C, align(64))]
pub struct ShmRingBuffer<T> {
    head: AtomicUsize,
    tail: AtomicUsize,
    capacity: usize,
    buffer_offset: usize, // Offset in bytes from start of ShmRingBuffer to first element
    _marker: PhantomData<T>,
}

impl<T> ShmRingBuffer<T> {
    /// Calculate the size and layout required for a ShmRingBuffer of given capacity.
    pub fn layout(capacity: usize) -> (Layout, usize) {
        let header_layout = Layout::new::<Self>();
        let buffer_element_layout = Layout::new::<UnsafeCell<Option<T>>>();

        let (buffer_layout, offset) = header_layout.extend(
            Layout::from_size_align(
                buffer_element_layout.size() * capacity,
                buffer_element_layout.align()
            ).unwrap()
        ).unwrap();

        (buffer_layout.pad_to_align(), offset)
    }

    /// Initialize a ShmRingBuffer in a provided raw memory pointer.
    pub unsafe fn init(ptr: *mut u8, capacity: usize) -> *mut Self {
        let (_, offset) = Self::layout(capacity);
        let rb_ptr = ptr as *mut Self;

        std::ptr::write(&mut (*rb_ptr).head, AtomicUsize::new(0));
        std::ptr::write(&mut (*rb_ptr).tail, AtomicUsize::new(0));
        (*rb_ptr).capacity = capacity;
        (*rb_ptr).buffer_offset = offset;

        let buffer_ptr = ptr.add(offset) as *mut UnsafeCell<Option<T>>;
        for i in 0..capacity {
            std::ptr::write(buffer_ptr.add(i), UnsafeCell::new(None));
        }

        rb_ptr
    }

    fn buffer_ptr(&self) -> *mut UnsafeCell<Option<T>> {
        unsafe {
            let base_ptr = self as *const Self as *mut u8;
            base_ptr.add(self.buffer_offset) as *mut UnsafeCell<Option<T>>
        }
    }

    pub fn push(&self, item: T) -> Result<(), T> {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Relaxed);

        if (tail + 1) % self.capacity == head {
            return Err(item);
        }

        unsafe {
            let cell_ptr = self.buffer_ptr().add(tail);
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
            let cell_ptr = self.buffer_ptr().add(head);
            (*(*cell_ptr).get()).take()
        };

        self.head.store((head + 1) % self.capacity, Ordering::Release);
        item
    }
}

// Keep the Arc-based version for internal use
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
        let (layout, _) = ShmRingBuffer::<i32>::layout(capacity);
        let mut mem = vec![0u8; layout.size() + 64]; // Extra space for manual alignment
        let ptr = mem.as_mut_ptr();
        let aligned_ptr = unsafe { ptr.add(ptr.align_offset(64)) };

        let rb_ptr = unsafe { ShmRingBuffer::<i32>::init(aligned_ptr, capacity) };
        let rb = unsafe { &*rb_ptr };

        rb.push(10).unwrap();
        rb.push(20).unwrap();
        assert_eq!(rb.pop(), Some(10));
        assert_eq!(rb.pop(), Some(20));
        assert_eq!(rb.pop(), None);
    }

    #[test]
    fn test_alignment() {
        let capacity = 4;
        let (layout, offset) = ShmRingBuffer::<AudioBlock>::layout(capacity);
        assert!(offset % 64 == 0);
        assert!(layout.align() >= 64);
    }
}
