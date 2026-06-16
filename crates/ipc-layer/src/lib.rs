use std::sync::atomic::{AtomicUsize, Ordering, AtomicBool};
use std::cell::UnsafeCell;
use std::marker::PhantomData;
use std::alloc::Layout;
use std::ffi::CString;
use std::sync::Arc;

#[derive(Debug)]
pub enum IpcError {
    ShmOpenFailed(String),
    FtruncateFailed(String),
    MmapFailed(String),
    EventFdFailed(String),
    PriorityFailed(String),
    CgroupFailed(String),
}

impl std::fmt::Display for IpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IpcError::ShmOpenFailed(s) => write!(f, "shm_open failed: {}", s),
            IpcError::FtruncateFailed(s) => write!(f, "ftruncate failed: {}", s),
            IpcError::MmapFailed(s) => write!(f, "mmap failed: {}", s),
            IpcError::EventFdFailed(s) => write!(f, "eventfd failed: {}", s),
            IpcError::PriorityFailed(s) => write!(f, "Failed to set RT priority: {}", s),
            IpcError::CgroupFailed(s) => write!(f, "Cgroup operation failed: {}", s),
        }
    }
}

impl std::error::Error for IpcError {}

pub use nullherz_traits::{AudioBlock, MidiEvent, SIMD_ALIGNMENT, MAX_BLOCK_SIZE};

impl nullherz_traits::GarbageProducer for crate::Producer<Box<dyn nullherz_traits::AudioProcessor>> {
    fn push_processor(&mut self, processor: Box<dyn nullherz_traits::AudioProcessor>) -> Result<(), Box<dyn nullherz_traits::AudioProcessor>> {
        self.push(processor)
    }
}

const _: () = assert!(std::mem::size_of::<AudioBlock>() == 1088); // 256*4 + 4 padded to 64
const _: () = assert!(std::mem::align_of::<AudioBlock>() == 64);

/// A status-flagged item for the ring buffer to ensure stable layout for IPC.
#[repr(C)]
pub struct ShmSlot<T> {
    data: UnsafeCell<T>,
}

/// A lock-free, Single-Producer Single-Consumer (SPSC) ring buffer
/// that can reside in shared memory.
#[repr(C, align(64))]
pub struct ShmRingBuffer<T> {
    head: AtomicUsize,
    _pad1: [u8; 64],
    tail: AtomicUsize,
    _pad2: [u8; 64],
    capacity: usize,
    buffer_offset: usize,
    _marker: PhantomData<T>,
}

#[derive(Clone)]
pub struct ShmProducer<T: Copy> {
    inner: *const ShmRingBuffer<T>,
}
unsafe impl<T: Copy + Send> Send for ShmProducer<T> {}
unsafe impl<T: Copy + Send> Sync for ShmProducer<T> {}
impl<T: Copy> ShmProducer<T> {
    pub fn new(inner: *const ShmRingBuffer<T>) -> Self { Self { inner } }
    pub fn push(&self, item: T) -> Result<(), T> {
        unsafe { (*self.inner).push(item) }
    }
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

    /// # Safety
    /// ptr must be a valid pointer to a memory region of the size returned by `layout(capacity)`.
    pub unsafe fn init(ptr: *mut u8, capacity: usize) -> *mut Self {
        let (_, offset) = Self::layout(capacity);
        let rb_ptr = ptr as *mut Self;
        unsafe {
            std::ptr::write(&mut (*rb_ptr).head, AtomicUsize::new(0));
            std::ptr::write(&mut (*rb_ptr).tail, AtomicUsize::new(0));
            (*rb_ptr).capacity = capacity;
            (*rb_ptr).buffer_offset = offset;
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
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);
        if (tail + 1) % self.capacity == head {
            return Err(item);
        }
        unsafe {
            let slot = &*self.buffer_ptr().add(tail);
            std::ptr::write(slot.data.get(), item);
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
        let val = unsafe {
            let slot = &*self.buffer_ptr().add(head);
            std::ptr::read(slot.data.get())
        };
        self.head.store((head + 1) % self.capacity, Ordering::Release);
        Some(val)
    }
}

#[derive(Clone)]
pub struct IpcCommandProducer<T: Copy + Send + Into<nullherz_traits::TimestampedCommand> + From<nullherz_traits::TimestampedCommand>> {
    pub producer: ShmProducer<T>,
}

impl<T: Copy + Send + Into<nullherz_traits::TimestampedCommand> + From<nullherz_traits::TimestampedCommand>> nullherz_traits::CommandProducer for IpcCommandProducer<T> {
    fn push_command(&self, command: nullherz_traits::TimestampedCommand) -> Result<(), nullherz_traits::Command> {
        let item: T = command.into();
        self.producer.push(item).map_err(|_| command.command)
    }
}

pub struct IpcCommandConsumer<T: Copy + Send + Into<nullherz_traits::TimestampedCommand>> {
    pub buffer: Arc<SharedMemory>,
    pub rb: *const ShmRingBuffer<T>,
}

unsafe impl<T: Copy + Send + Into<nullherz_traits::TimestampedCommand>> Send for IpcCommandConsumer<T> {}

impl<T: Copy + Send + Into<nullherz_traits::TimestampedCommand>> nullherz_traits::CommandConsumer for IpcCommandConsumer<T> {
    fn pop_command(&mut self) -> Option<nullherz_traits::TimestampedCommand> {
        unsafe { (*self.rb).pop().map(|item| item.into()) }
    }
}

#[derive(Clone)]
pub struct LocalMpscCommandProducer(pub Arc<MpscRingBuffer<nullherz_traits::TimestampedCommand>>);
impl nullherz_traits::CommandProducer for LocalMpscCommandProducer {
    fn push_command(&self, command: nullherz_traits::TimestampedCommand) -> Result<(), nullherz_traits::Command> {
        let cmd = command.command;
        self.0.push(command).map_err(|_| cmd)
    }
}

pub struct LocalMpscCommandConsumer(pub Arc<MpscRingBuffer<nullherz_traits::TimestampedCommand>>);
impl nullherz_traits::CommandConsumer for LocalMpscCommandConsumer {
    fn pop_command(&mut self) -> Option<nullherz_traits::TimestampedCommand> {
        self.0.pop()
    }
}

impl nullherz_traits::CommandProducer for Producer<nullherz_traits::TimestampedCommand> {
    fn push_command(&self, command: nullherz_traits::TimestampedCommand) -> Result<(), nullherz_traits::Command> {
        let cmd = command.command;
        // Producer usually needs &mut for push, but ours is an Arc to a RingBuffer which might allow &self.
        // Looking at Producer::push, it takes &mut self.
        // We'll need to wrap it or change the trait.
        // Actually, Producer is just a wrapper around Arc<RingBuffer>.
        let mut cloned = self.clone();
        cloned.push(command).map_err(|_| cmd)
    }
}

impl nullherz_traits::CommandConsumer for Consumer<nullherz_traits::TimestampedCommand> {
    fn pop_command(&mut self) -> Option<nullherz_traits::TimestampedCommand> {
        self.pop()
    }
}

impl nullherz_traits::TelemetryProducer for Producer<nullherz_traits::telemetry::Telemetry> {
    fn push_telemetry(&mut self, telemetry: nullherz_traits::telemetry::Telemetry) -> Result<(), nullherz_traits::telemetry::Telemetry> {
        Producer::push(self, telemetry)
    }
}

pub struct SharedMemory {
    ptr: *mut u8,
    size: usize,
    name: String,
    owner: bool,
}

impl SharedMemory {
    pub fn create(name: &str, size: usize) -> Result<Self, IpcError> {
        let cname = CString::new(name).map_err(|e| IpcError::ShmOpenFailed(e.to_string()))?;
        unsafe {
            let fd = libc::shm_open(cname.as_ptr(), libc::O_CREAT | libc::O_RDWR | libc::O_TRUNC, 0o600);
            if fd < 0 { return Err(IpcError::ShmOpenFailed(std::io::Error::last_os_error().to_string())); }
            if libc::ftruncate(fd, size as libc::off_t) < 0 {
                libc::close(fd);
                return Err(IpcError::FtruncateFailed(std::io::Error::last_os_error().to_string()));
            }
            let ptr = libc::mmap(std::ptr::null_mut(), size, libc::PROT_READ | libc::PROT_WRITE, libc::MAP_SHARED, fd, 0);
            libc::close(fd);
            if ptr == libc::MAP_FAILED { return Err(IpcError::MmapFailed(std::io::Error::last_os_error().to_string())); }
            Ok(Self { ptr: ptr as *mut u8, size, name: name.to_string(), owner: true })
        }
    }

    pub fn open(name: &str, size: usize) -> Result<Self, IpcError> {
        let cname = CString::new(name).map_err(|e| IpcError::ShmOpenFailed(e.to_string()))?;
        unsafe {
            let fd = libc::shm_open(cname.as_ptr(), libc::O_RDWR, 0o600);
            if fd < 0 { return Err(IpcError::ShmOpenFailed(std::io::Error::last_os_error().to_string())); }
            let ptr = libc::mmap(std::ptr::null_mut(), size, libc::PROT_READ | libc::PROT_WRITE, libc::MAP_SHARED, fd, 0);
            libc::close(fd);
            if ptr == libc::MAP_FAILED { return Err(IpcError::MmapFailed(std::io::Error::last_os_error().to_string())); }
            Ok(Self { ptr: ptr as *mut u8, size, name: name.to_string(), owner: false })
        }
    }

    pub fn ptr(&self) -> *mut u8 { self.ptr }
    pub fn size(&self) -> usize { self.size }

    /// Scans /dev/shm for stale nullherz segments and unlinks them.
    pub fn cleanup_stale_segments() {
        if let Ok(entries) = std::fs::read_dir("/dev/shm") {
            for entry in entries.flatten() {
                #[allow(clippy::collapsible_if)]
                if let Some(name) = entry.file_name().to_str() {
                    if name.starts_with("nullherz_") {
                        let cname = CString::new(name).unwrap();
                        unsafe { libc::shm_unlink(cname.as_ptr()); }
                    }
                }
            }
        }
    }
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
    pub fn create() -> Result<Self, IpcError> {
        unsafe {
            let fd = libc::eventfd(0, libc::EFD_CLOEXEC);
            if fd < 0 { return Err(IpcError::EventFdFailed(std::io::Error::last_os_error().to_string())); }
            Ok(Self { fd, owner: true })
        }
    }
    pub fn from_raw(fd: i32) -> Self { Self { fd, owner: false } }
    pub fn notify(&self) {
        let val: u64 = 1;
        unsafe { libc::write(self.fd, &val as *const u64 as *const libc::c_void, 8); }
    }
    pub fn wait(&self) -> u64 {
        let mut val: u64 = 0;
        let _ = unsafe { libc::read(self.fd, &mut val as *mut u64 as *mut libc::c_void, 8) };
        val
    }
    pub fn fd(&self) -> i32 { self.fd }
}

impl Drop for EventFd {
    fn drop(&mut self) { if self.owner { unsafe { libc::close(self.fd); } } }
}

pub fn set_rt_priority(priority: i32) -> Result<(), IpcError> {
    set_rt_priority_for(0, priority)
}

pub fn set_rt_priority_for(pid: i32, priority: i32) -> Result<(), IpcError> {
    unsafe {
        let param = libc::sched_param { sched_priority: priority };
        let result = libc::sched_setscheduler(pid, libc::SCHED_FIFO, &param);
        if result == -1 {
            return Err(IpcError::PriorityFailed(format!("PID {}: {}", pid, std::io::Error::last_os_error())));
        }
    }
    Ok(())
}

pub fn move_to_cgroup(cgroup_name: &str, pid: i32) -> Result<(), IpcError> {
    let base_path = format!("/sys/fs/cgroup/{}", cgroup_name);
    let procs_path = format!("{}/cgroup.procs", base_path);

    if !std::path::Path::new(&base_path).exists() {
        std::fs::create_dir_all(&base_path).map_err(|e| IpcError::CgroupFailed(format!("Failed to create directory {}: {}", base_path, e)))?;
    }

    std::fs::write(&procs_path, pid.to_string())
        .map_err(|e| IpcError::CgroupFailed(format!("Failed to write PID to {}: {}", procs_path, e)))
}

#[repr(C, align(64))]
pub struct ShmSignal {
    pub(crate) flag: AtomicBool,
    pub heartbeat: std::sync::atomic::AtomicU64,
}

const _: () = assert!(std::mem::size_of::<ShmSignal>() == 64);
const _: () = assert!(std::mem::align_of::<ShmSignal>() == 64);

impl Default for ShmSignal {
    fn default() -> Self {
        Self::new()
    }
}

impl ShmSignal {
    pub fn new() -> Self { Self { flag: AtomicBool::new(false), heartbeat: std::sync::atomic::AtomicU64::new(0) } }
    pub fn notify(&self) { self.flag.store(true, Ordering::Release); }
    pub fn check_and_clear(&self) -> bool { self.flag.swap(false, Ordering::Acquire) }
    pub fn pulse_heartbeat(&self) { self.heartbeat.fetch_add(1, Ordering::Release); }
    pub fn get_heartbeat(&self) -> u64 { self.heartbeat.load(Ordering::Acquire) }
}

pub struct RingBuffer<T> {
    buffer: Box<[UnsafeCell<Option<T>>]>,
    head: AtomicUsize,
    _pad1: [u8; 64],
    tail: AtomicUsize,
    _pad2: [u8; 64],
    capacity: usize,
}

unsafe impl<T: Send> Sync for RingBuffer<T> {}

pub enum NonRtProducerInner<T> {
    Spsc(tokio::sync::Mutex<Producer<T>>),
    Mpsc(Arc<MpscRingBuffer<T>>),
    Boxed(tokio::sync::Mutex<Box<dyn nullherz_traits::CommandProducer>>),
}

pub struct NonRtProducer<T> {
    inner: Arc<NonRtProducerInner<T>>,
}

impl<T> NonRtProducer<T> {
    pub fn new(producer: Producer<T>) -> Self {
        Self { inner: Arc::new(NonRtProducerInner::Spsc(tokio::sync::Mutex::new(producer))) }
    }

    pub fn from_mpsc(buffer: Arc<MpscRingBuffer<T>>) -> Self {
        Self { inner: Arc::new(NonRtProducerInner::Mpsc(buffer)) }
    }

    pub fn from_boxed(producer: Box<dyn nullherz_traits::CommandProducer>) -> Self {
        Self { inner: Arc::new(NonRtProducerInner::Boxed(tokio::sync::Mutex::new(producer))) }
    }

    pub async fn push(&self, item: T) -> Result<(), T> {
        match self.inner.as_ref() {
            NonRtProducerInner::Spsc(m) => {
                let mut producer = m.lock().await;
                producer.push(item)
            }
            NonRtProducerInner::Mpsc(b) => {
                b.push(item)
            }
            NonRtProducerInner::Boxed(_) => {
                Err(item)
            }
        }
    }
}

impl NonRtProducer<nullherz_traits::TimestampedCommand> {
    pub async fn push_command(&self, item: nullherz_traits::TimestampedCommand) -> Result<(), nullherz_traits::Command> {
        match self.inner.as_ref() {
            NonRtProducerInner::Spsc(m) => {
                let mut producer = m.lock().await;
                producer.push(item).map_err(|c| c.command)
            }
            NonRtProducerInner::Mpsc(b) => {
                b.push(item).map_err(|c| c.command)
            }
            NonRtProducerInner::Boxed(m) => {
                let mut producer = m.lock().await;
                producer.push_command(item)
            }
        }
    }
}

impl<T> Clone for NonRtProducer<T> {
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone() }
    }
}

impl<T> RingBuffer<T> {
    pub fn new(capacity: usize) -> Self {
        let mut buffer = Vec::with_capacity(capacity);
        for _ in 0..capacity { buffer.push(UnsafeCell::new(None)); }
        Self {
            buffer: buffer.into_boxed_slice(),
            head: AtomicUsize::new(0),
            _pad1: [0; 64],
            tail: AtomicUsize::new(0),
            _pad2: [0; 64],
            capacity,
        }
    }
    pub fn split(self) -> (Producer<T>, Consumer<T>) {
        let arc = std::sync::Arc::new(self);
        (Producer { inner: arc.clone() }, Consumer { inner: arc })
    }
}

pub struct Producer<T> { inner: std::sync::Arc<RingBuffer<T>> }
impl<T> Clone for Producer<T> {
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone() }
    }
}
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

/// A Multi-Producer Single-Consumer (MPSC) lock-free ring buffer.
/// Uses a sequence-per-slot strategy to allow multiple RT producers.
pub struct MpscRingBuffer<T> {
    buffer: Box<[UnsafeCell<Option<T>>]>,
    sequences: Box<[std::sync::atomic::AtomicUsize]>,
    head: std::sync::atomic::AtomicUsize,
    tail: std::sync::atomic::AtomicUsize,
    capacity: usize,
    mask: usize,
}

unsafe impl<T: Send> Sync for MpscRingBuffer<T> {}
unsafe impl<T: Send> Send for MpscRingBuffer<T> {}

impl<T> MpscRingBuffer<T> {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity.is_power_of_two());
        let mut buffer = Vec::with_capacity(capacity);
        let mut sequences = Vec::with_capacity(capacity);
        for i in 0..capacity {
            buffer.push(UnsafeCell::new(None));
            sequences.push(std::sync::atomic::AtomicUsize::new(i));
        }
        Self {
            buffer: buffer.into_boxed_slice(),
            sequences: sequences.into_boxed_slice(),
            head: std::sync::atomic::AtomicUsize::new(0),
            tail: std::sync::atomic::AtomicUsize::new(0),
            capacity,
            mask: capacity - 1,
        }
    }

    pub fn push(&self, item: T) -> Result<(), T> {
        let mut pos = self.tail.load(Ordering::Relaxed);
        loop {
            let seq_ptr = &self.sequences[pos & self.mask];
            let seq = seq_ptr.load(Ordering::Acquire);
            let diff = seq as isize - pos as isize;

            if diff == 0 {
                if self.tail.compare_exchange_weak(pos, pos + 1, Ordering::Relaxed, Ordering::Relaxed).is_ok() {
                    unsafe { *self.buffer[pos & self.mask].get() = Some(item); }
                    seq_ptr.store(pos + 1, Ordering::Release);
                    return Ok(());
                }
            } else if diff < 0 {
                return Err(item);
            } else {
                pos = self.tail.load(Ordering::Relaxed);
            }
        }
    }

    pub fn pop(&self) -> Option<T> {
        let mut pos = self.head.load(Ordering::Relaxed);
        loop {
            let seq_ptr = &self.sequences[pos & self.mask];
            let seq = seq_ptr.load(Ordering::Acquire);
            let diff = seq as isize - (pos + 1) as isize;

            if diff == 0 {
                if self.head.compare_exchange_weak(pos, pos + 1, Ordering::Relaxed, Ordering::Relaxed).is_ok() {
                    let item = unsafe { (*self.buffer[pos & self.mask].get()).take() };
                    seq_ptr.store(pos + self.capacity, Ordering::Release);
                    return item;
                }
            } else if diff < 0 {
                return None;
            } else {
                pos = self.head.load(Ordering::Relaxed);
            }
        }
    }
}

#[cfg(feature = "kani-verify")]
#[allow(unexpected_cfgs)]
mod proofs {
    use super::*;

    #[kani::proof]
    #[kani::unwind(5)]
    fn prove_shm_ring_buffer_safety() {
        let capacity = kani::any_where(|&c: &usize| c > 1 && c < 5);
        let (layout, _) = ShmRingBuffer::<u32>::layout(capacity);

        // Use a small fixed buffer for verification to stay within bounds
        let mut mem = [0u8; 1024];
        if layout.size() + 64 > mem.len() { return; }

        let ptr = mem.as_mut_ptr();
        let aligned_ptr = unsafe { ptr.add(ptr.align_offset(64)) };

        let rb_ptr = unsafe { ShmRingBuffer::<u32>::init(aligned_ptr, capacity) };
        let rb = unsafe { &*rb_ptr };

        let val: u32 = kani::any();
        if rb.push(val).is_ok() {
            let popped = rb.pop();
            kani::assert(popped == Some(val), "Popped value must match pushed value");
        }
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn prove_mpsc_ring_buffer_safety() {
        // MpscRingBuffer requires power-of-two capacity
        let capacity = 4;
        let buffer = MpscRingBuffer::<u32>::new(capacity);

        let val: u32 = kani::any();
        if buffer.push(val).is_ok() {
            let popped = buffer.pop();
            kani::assert(popped == Some(val), "Popped value must match pushed value");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn test_mpsc_ring_buffer_consistency(
            values in prop::collection::vec(0..1000i32, 1..100)
        ) {
            let buffer = MpscRingBuffer::new(128);
            for &val in &values {
                let _ = buffer.push(val);
            }
            let mut popped = Vec::new();
            while let Some(val) = buffer.pop() {
                popped.push(val);
            }
            // MPSC guarantees order from a single thread
            prop_assert_eq!(values, popped);
        }
    }

    #[test]
    fn test_mpsc_stress() {
        let buffer = Arc::new(MpscRingBuffer::new(1024));
        let num_producers = 4;
        let items_per_producer = 1000;
        let mut handles = Vec::new();

        for p in 0..num_producers {
            let buf = buffer.clone();
            handles.push(std::thread::spawn(move || {
                for i in 0..items_per_producer {
                    while buf.push(p * 10000 + i).is_err() {
                        std::hint::spin_loop();
                    }
                }
            }));
        }

        let mut counts = vec![0; num_producers];
        let mut received = 0;
        while received < num_producers * items_per_producer {
            if let Some(val) = buffer.pop() {
                let producer = val / 10000;
                counts[producer] += 1;
                received += 1;
            }
        }

        for h in handles { h.join().unwrap(); }
        for count in counts {
            assert_eq!(count, items_per_producer);
        }
    }

    #[test]
    fn test_shm_ring_buffer_alignment_and_offset() {
        // Verify that different capacities result in correct layout sizes and alignment
        for &capacity in &[2, 4, 8, 16, 32] {
            let (layout, _) = ShmRingBuffer::<AudioBlock>::layout(capacity);
            assert!(layout.size() > capacity * std::mem::size_of::<AudioBlock>());
            assert_eq!(layout.align(), 64);
        }
    }

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
