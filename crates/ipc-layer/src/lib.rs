// Non-RT plane (non-RT setup/cleanup helpers): thread spawn/sleep are sanctioned here.
// The disallowed-methods lint exists to protect the audio hot path only.
#![allow(clippy::disallowed_methods)]
use std::sync::atomic::{AtomicUsize, Ordering, AtomicBool};
use std::cell::UnsafeCell;
use std::marker::PhantomData;
use std::alloc::Layout;
use std::ffi::CString;
use std::sync::Arc;

pub mod tcp;

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
    pub head: AtomicUsize,
    _pad1: [u8; 64],
    pub tail: AtomicUsize,
    _pad2: [u8; 64],
    pub capacity: usize,
    pub buffer_offset: usize,
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

pub struct IpcMidiConsumer {
    pub buffer: Arc<SharedMemory>,
    pub rb: *const ShmRingBuffer<MidiEvent>,
}

unsafe impl Send for IpcMidiConsumer {}

impl IpcMidiConsumer {
    pub fn pop(&mut self) -> Option<MidiEvent> {
        unsafe { (*self.rb).pop() }
    }
}

pub struct IpcAudioConsumer {
    pub buffer: Arc<SharedMemory>,
    pub rb: *const ShmRingBuffer<AudioBlock>,
}

unsafe impl Send for IpcAudioConsumer {}

impl IpcAudioConsumer {
    pub fn pop(&mut self) -> Option<AudioBlock> {
        unsafe { (*self.rb).pop() }
    }
}

#[derive(Clone)]
pub struct IpcAudioProducer {
    pub buffer: Arc<SharedMemory>,
    pub rb: *const ShmRingBuffer<AudioBlock>,
}

unsafe impl Send for IpcAudioProducer {}
unsafe impl Sync for IpcAudioProducer {}

impl IpcAudioProducer {
    pub fn push(&self, block: AudioBlock) -> Result<(), AudioBlock> {
        unsafe { (*self.rb).push(block) }
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

impl nullherz_traits::MidiConsumer for Consumer<nullherz_traits::MidiEvent> {
    fn pop(&mut self) -> Option<nullherz_traits::MidiEvent> {
        self.pop()
    }
}

impl nullherz_traits::TopologyMutationConsumer for Consumer<nullherz_traits::TopologyMutation> {
    fn pop(&mut self) -> Option<nullherz_traits::TopologyMutation> {
        self.pop()
    }
}

impl nullherz_traits::CommandBundleConsumer for Consumer<Vec<nullherz_traits::Command>> {
    fn pop(&mut self) -> Option<Vec<nullherz_traits::Command>> {
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

unsafe impl Send for SharedMemory {}
unsafe impl Sync for SharedMemory {}

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

impl AsRef<[u8]> for SharedMemory {
    fn as_ref(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.ptr, self.size) }
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

pub fn set_cgroup_memory_limit(cgroup_name: &str, limit_bytes: usize) -> Result<(), IpcError> {
    let base_path = format!("/sys/fs/cgroup/{}", cgroup_name);
    let limit_path = format!("{}/memory.max", base_path);

    if !std::path::Path::new(&base_path).exists() {
        std::fs::create_dir_all(&base_path).map_err(|e| IpcError::CgroupFailed(format!("Failed to create directory {}: {}", base_path, e)))?;
    }

    std::fs::write(&limit_path, limit_bytes.to_string())
        .map_err(|e| IpcError::CgroupFailed(format!("Failed to set memory limit for {}: {}", cgroup_name, e)))
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
    Spsc(parking_lot::Mutex<Producer<T>>),
    Mpsc(Arc<MpscRingBuffer<T>>),
    Boxed(parking_lot::Mutex<Box<dyn nullherz_traits::CommandProducer>>),
}

pub struct NonRtProducer<T> {
    inner: Arc<NonRtProducerInner<T>>,
}

impl<T> NonRtProducer<T> {
    pub fn new(producer: Producer<T>) -> Self {
        Self { inner: Arc::new(NonRtProducerInner::Spsc(parking_lot::Mutex::new(producer))) }
    }

    pub fn from_mpsc(buffer: Arc<MpscRingBuffer<T>>) -> Self {
        Self { inner: Arc::new(NonRtProducerInner::Mpsc(buffer)) }
    }

    pub fn from_boxed(producer: Box<dyn nullherz_traits::CommandProducer>) -> Self {
        Self { inner: Arc::new(NonRtProducerInner::Boxed(parking_lot::Mutex::new(producer))) }
    }

    pub fn push(&self, item: T) -> Result<(), T> {
        match self.inner.as_ref() {
            NonRtProducerInner::Spsc(m) => {
                let mut producer = m.lock();
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
    pub fn push_command(&self, item: nullherz_traits::TimestampedCommand) -> Result<(), nullherz_traits::Command> {
        match self.inner.as_ref() {
            NonRtProducerInner::Spsc(m) => {
                let mut producer = m.lock();
                producer.push(item).map_err(|c| c.command)
            }
            NonRtProducerInner::Mpsc(b) => {
                b.push(item).map_err(|c| c.command)
            }
            NonRtProducerInner::Boxed(m) => {
                let producer = m.lock();
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
impl<T> Clone for Consumer<T> {
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone() }
    }
}
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

pub fn pin_thread_to_core(core_id: usize) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        use nix::sched::{sched_setaffinity, CpuSet};
        use nix::unistd::Pid;
        let mut cpu_set = CpuSet::new();
        cpu_set.set(core_id).map_err(|e: nix::Error| e.to_string())?;
        sched_setaffinity(Pid::from_raw(0), &cpu_set).map_err(|e: nix::Error| e.to_string())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = core_id;
        Ok(())
    }
}

pub fn setup_rt_thread(priority: i32, cpu_id: Option<usize>) {
    thread_local! {
        static INITIALIZED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    if INITIALIZED.with(|i| i.get()) && cpu_id.is_none() {
        return;
    }

    nullherz_traits::mark_as_rt_thread();
    let _ = crate::set_rt_priority(priority);

    if let Some(id) = cpu_id {
        let _ = pin_thread_to_core(id);
    }

    // Set permanent FTZ/DAZ for the thread
    FpControlGuard::apply_ftz_daz();

    INITIALIZED.with(|i| i.set(true));
}

/// RAII guard for floating-point control state.
/// Ensures FTZ/DAZ are set during the lifetime of the guard and restored afterwards.
pub struct FpControlGuard {
    #[cfg(target_arch = "x86_64")]
    original_mxcsr: u32,
    #[cfg(target_arch = "aarch64")]
    original_fpcr: u64,
}

impl FpControlGuard {
    #[inline(always)]
    pub fn apply_ftz_daz() {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            let mut mxcsr: u32 = 0;
            std::arch::asm!("stmxcsr [{}]", in(reg) &mut mxcsr);
            // Enable Flush-to-Zero (bit 15) and Denormals-Are-Zero (bit 6)
            mxcsr |= 0x8000 | 0x0040;
            std::arch::asm!("ldmxcsr [{}]", in(reg) &mxcsr);
        }

        #[cfg(target_arch = "aarch64")]
        unsafe {
            let mut fpcr: u64;
            std::arch::asm!("mrs {}, fpcr", out(reg) fpcr);
            // Bit 24 is FZ (Flush-to-Zero)
            fpcr |= 1 << 24;
            std::arch::asm!("msr fpcr, {}", in(reg) fpcr);
        }
    }

    #[inline(always)]
    pub fn new() -> Self {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            let mut original_mxcsr: u32 = 0;
            std::arch::asm!("stmxcsr [{}]", in(reg) &mut original_mxcsr);
            Self::apply_ftz_daz();
            Self { original_mxcsr }
        }
        #[cfg(target_arch = "aarch64")]
        unsafe {
            let mut original_fpcr: u64 = 0;
            std::arch::asm!("mrs {}, fpcr", out(reg) original_fpcr);
            Self::apply_ftz_daz();
            Self { original_fpcr }
        }
        #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
        { Self {} }
    }
}

impl Default for FpControlGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for FpControlGuard {
    #[inline(always)]
    fn drop(&mut self) {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            std::arch::asm!("ldmxcsr [{}]", in(reg) &self.original_mxcsr);
        }
        #[cfg(target_arch = "aarch64")]
        unsafe {
            std::arch::asm!("msr fpcr, {}", in(reg) self.original_fpcr);
        }
    }
}

/// Prototype for zero-copy RDMA distribution.
/// This will facilitate high-density DSP offloading in Stage 7.
pub struct RdmaBridge {
    pub is_connected: bool,
}

impl RdmaBridge {
    pub fn new() -> Self {
        Self { is_connected: false }
    }

    pub fn push_block_zero_copy(&self, _node_idx: u32, _block: &AudioBlock) -> Result<(), String> {
        // Implementation pending: libibverbs / rdma-core integration
        Err("RDMA implementation pending Stage 7 finalization".to_string())
    }
}

impl Default for RdmaBridge {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(all(feature = "kani-verify", kani))]
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

    #[kani::proof]
    fn prove_shm_signal_atomic_ordering() {
        let signal = ShmSignal::new();
        kani::assert(!signal.check_and_clear(), "Initial flag must be false");
        signal.notify();
        kani::assert(signal.check_and_clear(), "Flag must be true after notify");
        kani::assert(!signal.check_and_clear(), "Flag must be cleared after check");

        let initial_heartbeat = signal.get_heartbeat();
        signal.pulse_heartbeat();
        kani::assert(signal.get_heartbeat() == initial_heartbeat + 1, "Heartbeat must increment");
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

#[cfg(test)]
mod ring_buffer_tests {
    use super::*;

    #[test]
    fn test_spsc_fifo_fill_reject_drain() {
        // Capacity N holds N-1 items (one slot kept open to distinguish full/empty).
        let (mut prod, mut cons) = RingBuffer::new(8).split();
        for i in 0..7u64 {
            assert!(prod.push(i).is_ok(), "slot {} must fit", i);
        }
        assert_eq!(prod.push(99), Err(99), "full buffer must reject and return the item");

        for i in 0..7u64 {
            assert_eq!(cons.pop(), Some(i), "strict FIFO order");
        }
        assert_eq!(cons.pop(), None, "drained buffer is empty");
    }

    #[test]
    fn test_spsc_wraparound_preserves_order() {
        let (mut prod, mut cons) = RingBuffer::new(4).split();
        // Cycle far more items than capacity so head/tail wrap many times.
        for i in 0..1000u64 {
            assert!(prod.push(i).is_ok());
            assert_eq!(cons.pop(), Some(i));
        }
        assert_eq!(cons.pop(), None);
    }

    #[test]
    fn test_spsc_cross_thread_transfers_every_item_in_order() {
        const N: u64 = 50_000;
        let (mut prod, mut cons) = RingBuffer::new(64).split();

        let producer = std::thread::spawn(move || {
            for i in 0..N {
                let mut item = i;
                loop {
                    match prod.push(item) {
                        Ok(()) => break,
                        Err(back) => {
                            item = back;
                            std::hint::spin_loop();
                        }
                    }
                }
            }
        });

        let mut expected = 0u64;
        while expected < N {
            if let Some(v) = cons.pop() {
                assert_eq!(v, expected, "items must arrive exactly once, in order");
                expected += 1;
            } else {
                std::hint::spin_loop();
            }
        }
        producer.join().unwrap();
        assert_eq!(cons.pop(), None, "nothing left after all items accounted for");
    }

    #[test]
    fn test_mpsc_capacity_reject_and_reuse() {
        let buf = MpscRingBuffer::new(8);
        for i in 0..8u64 {
            assert!(buf.push(i).is_ok());
        }
        assert_eq!(buf.push(99), Err(99), "full MPSC queue must reject");
        assert_eq!(buf.pop(), Some(0));
        assert!(buf.push(100).is_ok(), "freed slot must be reusable");
        // Remaining order: 1..=7 then 100
        for i in 1..8u64 {
            assert_eq!(buf.pop(), Some(i));
        }
        assert_eq!(buf.pop(), Some(100));
        assert_eq!(buf.pop(), None);
    }

    #[test]
    fn test_mpsc_concurrent_producers_lose_nothing() {
        const PRODUCERS: u64 = 4;
        const PER_PRODUCER: u64 = 10_000;
        let buf = std::sync::Arc::new(MpscRingBuffer::new(256));

        let handles: Vec<_> = (0..PRODUCERS)
            .map(|p| {
                let buf = buf.clone();
                std::thread::spawn(move || {
                    for i in 0..PER_PRODUCER {
                        // Tag items with the producer id in the high bits.
                        let mut item = (p << 32) | i;
                        loop {
                            match buf.push(item) {
                                Ok(()) => break,
                                Err(back) => {
                                    item = back;
                                    std::hint::spin_loop();
                                }
                            }
                        }
                    }
                })
            })
            .collect();

        let mut next_expected = [0u64; PRODUCERS as usize];
        let mut received = 0u64;
        while received < PRODUCERS * PER_PRODUCER {
            if let Some(v) = buf.pop() {
                let producer = (v >> 32) as usize;
                let seq = v & 0xFFFF_FFFF;
                assert_eq!(
                    seq, next_expected[producer],
                    "each producer's items must arrive in that producer's order"
                );
                next_expected[producer] += 1;
                received += 1;
            } else {
                std::hint::spin_loop();
            }
        }
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(buf.pop(), None, "exactly N*P items, no duplicates");
    }

    #[test]
    fn test_shm_signal_flag_and_heartbeat() {
        let sig = ShmSignal::new();
        assert!(!sig.check_and_clear(), "starts clear");
        sig.notify();
        assert!(sig.check_and_clear(), "notify sets the flag");
        assert!(!sig.check_and_clear(), "check clears it (edge-triggered)");

        let h0 = sig.get_heartbeat();
        sig.pulse_heartbeat();
        sig.pulse_heartbeat();
        assert_eq!(sig.get_heartbeat(), h0 + 2, "heartbeat is a monotonic counter");
    }
}

#[cfg(test)]
mod ring_buffer_properties {
    use super::*;
    use proptest::prelude::*;

    #[derive(Debug, Clone)]
    enum Op {
        Push(u64),
        Pop,
    }

    fn op_strategy() -> impl Strategy<Value = Op> {
        prop_oneof![
            (0u64..1000).prop_map(Op::Push),
            Just(Op::Pop),
        ]
    }

    proptest! {
        /// Model-based check: under ANY single-threaded interleaving of pushes
        /// and pops, the SPSC ring behaves exactly like a bounded FIFO queue —
        /// same accepts, same rejects, same pop order. (Cross-thread ordering
        /// is covered by the stress tests; this pins the sequential semantics
        /// they assume.)
        #[test]
        fn spsc_matches_bounded_fifo_model(
            capacity in 2usize..32,
            ops in proptest::collection::vec(op_strategy(), 0..200),
        ) {
            let (mut prod, mut cons) = RingBuffer::new(capacity).split();
            let mut model: std::collections::VecDeque<u64> = std::collections::VecDeque::new();
            let usable = capacity - 1; // one slot kept open to distinguish full/empty

            for op in ops {
                match op {
                    Op::Push(v) => {
                        let real = prod.push(v);
                        if model.len() < usable {
                            prop_assert!(real.is_ok(), "model accepts, ring must too");
                            model.push_back(v);
                        } else {
                            prop_assert_eq!(real, Err(v), "model full, ring must reject");
                        }
                    }
                    Op::Pop => {
                        prop_assert_eq!(cons.pop(), model.pop_front(), "pop order must match FIFO");
                    }
                }
            }
            // Drain: remaining contents must match exactly.
            while let Some(expected) = model.pop_front() {
                prop_assert_eq!(cons.pop(), Some(expected));
            }
            prop_assert_eq!(cons.pop(), None);
        }

        /// Same model check for the MPSC (Vyukov) queue in sequential use —
        /// full capacity usable, strict FIFO.
        #[test]
        fn mpsc_matches_bounded_fifo_model(
            cap_pow in 1u32..6,
            ops in proptest::collection::vec(op_strategy(), 0..200),
        ) {
            let capacity = 1usize << cap_pow;
            let buf = MpscRingBuffer::new(capacity);
            let mut model: std::collections::VecDeque<u64> = std::collections::VecDeque::new();

            for op in ops {
                match op {
                    Op::Push(v) => {
                        let real = buf.push(v);
                        if model.len() < capacity {
                            prop_assert!(real.is_ok());
                            model.push_back(v);
                        } else {
                            prop_assert_eq!(real, Err(v));
                        }
                    }
                    Op::Pop => {
                        prop_assert_eq!(buf.pop(), model.pop_front());
                    }
                }
            }
            while let Some(expected) = model.pop_front() {
                prop_assert_eq!(buf.pop(), Some(expected));
            }
            prop_assert_eq!(buf.pop(), None);
        }
    }
}
