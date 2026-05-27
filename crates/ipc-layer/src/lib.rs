use std::sync::atomic::{AtomicUsize, Ordering, AtomicBool};
use std::cell::UnsafeCell;
use std::marker::PhantomData;
use std::alloc::Layout;
use std::ffi::CString;

/// Alignment for SIMD (AVX-512 requires 64 bytes).
pub const SIMD_ALIGNMENT: usize = 64;

/// A SIMD-aligned audio block.
#[repr(C, align(64))]
#[derive(Clone, Copy)]
pub struct AudioBlock {
    pub data: [f32; 128], // Fixed size for predictability
}

/// A status-flagged item for the ring buffer to ensure stable layout for IPC.
#[repr(C)]
pub struct ShmSlot<T> {
    occupied: AtomicBool,
    data: UnsafeCell<T>,
}

/// A lock-free, Single-Producer Single-Consumer (SPSC) ring buffer
/// that can reside in shared memory.
#[repr(C, align(64))]
pub struct ShmRingBuffer<T> {
    head: AtomicUsize,
    tail: AtomicUsize,
    capacity: usize,
    buffer_offset: usize,
    _marker: PhantomData<T>,
}

impl<T: Copy> ShmRingBuffer<T> {
    pub fn layout(capacity: usize) -> (Layout, usize) {
        let header_layout = Layout::new::<Self>();
        let buffer_element_layout = Layout::new::<ShmSlot<T>>();
        let (buffer_layout, offset) = header_layout.extend(
            Layout::from_size_align(
                buffer_element_layout.size() * capacity,
                buffer_element_layout.align()
            ).unwrap()
        ).unwrap();
        (buffer_layout.pad_to_align(), offset)
    }

    pub unsafe fn init(ptr: *mut u8, capacity: usize) -> *mut Self {
        let (_, offset) = Self::layout(capacity);
        let rb_ptr = ptr as *mut Self;
        std::ptr::write(&mut (*rb_ptr).head, AtomicUsize::new(0));
        std::ptr::write(&mut (*rb_ptr).tail, AtomicUsize::new(0));
        (*rb_ptr).capacity = capacity;
        (*rb_ptr).buffer_offset = offset;

        let buffer_ptr = ptr.add(offset) as *mut ShmSlot<T>;
        for i in 0..capacity {
            let slot = &*buffer_ptr.add(i);
            slot.occupied.store(false, Ordering::Relaxed);
        }
        rb_ptr
    }

    fn buffer_ptr(&self) -> *mut ShmSlot<T> {
        unsafe {
            let base_ptr = self as *const Self as *mut u8;
            base_ptr.add(self.buffer_offset) as *mut ShmSlot<T>
        }
    }

    pub fn push(&self, item: T) -> Result<(), T> {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Relaxed);
        if (tail + 1) % self.capacity == head {
            return Err(item);
        }
        unsafe {
            let slot = &*self.buffer_ptr().add(tail);
            std::ptr::write(slot.data.get(), item);
            slot.occupied.store(true, Ordering::Release);
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
            let slot = &*self.buffer_ptr().add(head);
            if !slot.occupied.load(Ordering::Acquire) {
                return None;
            }
            let val = std::ptr::read(slot.data.get());
            slot.occupied.store(false, Ordering::Release);
            val
        };
        self.head.store((head + 1) % self.capacity, Ordering::Release);
        Some(item)
    }
}

pub struct SharedMemory {
    ptr: *mut u8,
    size: usize,
    name: String,
    owner: bool,
}

impl SharedMemory {
    pub fn create(name: &str, size: usize) -> Result<Self, String> {
        let cname = CString::new(name).map_err(|e| e.to_string())?;
        unsafe {
            let fd = libc::shm_open(cname.as_ptr(), libc::O_CREAT | libc::O_RDWR | libc::O_TRUNC, 0o600);
            if fd < 0 { return Err("shm_open failed".to_string()); }
            if libc::ftruncate(fd, size as libc::off_t) < 0 {
                libc::close(fd);
                return Err("ftruncate failed".to_string());
            }
            let ptr = libc::mmap(std::ptr::null_mut(), size, libc::PROT_READ | libc::PROT_WRITE, libc::MAP_SHARED, fd, 0);
            libc::close(fd);
            if ptr == libc::MAP_FAILED { return Err("mmap failed".to_string()); }
            Ok(Self { ptr: ptr as *mut u8, size, name: name.to_string(), owner: true })
        }
    }

    pub fn open(name: &str, size: usize) -> Result<Self, String> {
        let cname = CString::new(name).map_err(|e| e.to_string())?;
        unsafe {
            let fd = libc::shm_open(cname.as_ptr(), libc::O_RDWR, 0o600);
            if fd < 0 { return Err("shm_open failed".to_string()); }
            let ptr = libc::mmap(std::ptr::null_mut(), size, libc::PROT_READ | libc::PROT_WRITE, libc::MAP_SHARED, fd, 0);
            libc::close(fd);
            if ptr == libc::MAP_FAILED { return Err("mmap failed".to_string()); }
            Ok(Self { ptr: ptr as *mut u8, size, name: name.to_string(), owner: false })
        }
    }

    pub fn ptr(&self) -> *mut u8 { self.ptr }
    pub fn size(&self) -> usize { self.size }
}

impl Drop for SharedMemory {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.ptr as *mut libc::c_void, self.size);
            if self.owner {
                let cname = CString::new(self.name.as_str()).unwrap();
                libc::shm_unlink(cname.as_ptr());
            }
        }
    }
}

pub struct EventFd {
    fd: i32,
    owner: bool,
}

impl EventFd {
    pub fn create() -> Result<Self, String> {
        unsafe {
            let fd = libc::eventfd(0, libc::EFD_NONBLOCK);
            if fd < 0 { return Err("eventfd failed".to_string()); }
            Ok(Self { fd, owner: true })
        }
    }
    pub fn from_raw(fd: i32) -> Self { Self { fd, owner: false } }
    pub fn notify(&self) {
        let val: u64 = 1;
        unsafe { libc::write(self.fd, &val as *const u64 as *const libc::c_void, 8); }
    }
    pub fn wait(&self) {
        let mut val: u64 = 0;
        let _ = unsafe { libc::read(self.fd, &mut val as *mut u64 as *mut libc::c_void, 8) };
    }
    pub fn fd(&self) -> i32 { self.fd }
}

impl Drop for EventFd {
    fn drop(&mut self) { if self.owner { unsafe { libc::close(self.fd); } } }
}

#[repr(C, align(64))]
pub struct ShmSignal {
    pub(crate) flag: AtomicBool,
}

impl ShmSignal {
    pub fn new() -> Self { Self { flag: AtomicBool::new(false) } }
    pub fn notify(&self) { self.flag.store(true, Ordering::Release); }
    pub fn check_and_clear(&self) -> bool { self.flag.swap(false, Ordering::Acquire) }
}

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
        for _ in 0..capacity { buffer.push(UnsafeCell::new(None)); }
        Self { buffer: buffer.into_boxed_slice(), head: AtomicUsize::new(0), tail: AtomicUsize::new(0), capacity }
    }
    pub fn split(self) -> (Producer<T>, Consumer<T>) {
        let arc = std::sync::Arc::new(self);
        (Producer { inner: arc.clone() }, Consumer { inner: arc })
    }
}

pub struct Producer<T> { inner: std::sync::Arc<RingBuffer<T>> }
impl<T> Producer<T> {
    pub fn push(&mut self, item: T) -> Result<(), T> {
        let head = self.inner.head.load(Ordering::Acquire);
        let tail = self.inner.tail.load(Ordering::Relaxed);
        if (tail + 1) % self.inner.capacity == head { return Err(item); }
        unsafe { let cell_ptr = self.inner.buffer[tail].get(); std::ptr::write(cell_ptr, Some(item)); }
        self.inner.tail.store((tail + 1) % self.inner.capacity, Ordering::Release);
        Ok(())
    }
}

pub struct Consumer<T> { inner: std::sync::Arc<RingBuffer<T>> }
impl<T> Consumer<T> {
    pub fn pop(&mut self) -> Option<T> {
        let head = self.inner.head.load(Ordering::Relaxed);
        let tail = self.inner.tail.load(Ordering::Acquire);
        if head == tail { return None; }
        let item = unsafe { let cell_ptr = self.inner.buffer[head].get(); (*cell_ptr).take() };
        self.inner.head.store((head + 1) % self.inner.capacity, Ordering::Release);
        item
    }
    pub fn peek(&self) -> Option<&T> {
        let head = self.inner.head.load(Ordering::Relaxed);
        let tail = self.inner.tail.load(Ordering::Acquire);
        if head == tail { return None; }
        unsafe { let cell_ptr = self.inner.buffer[head].get(); (*cell_ptr).as_ref() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shm_ring_buffer() {
        let capacity = 4;
        let (layout, _) = ShmRingBuffer::<i32>::layout(capacity);
        let mut mem = vec![0u8; layout.size() + 64];
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
}
