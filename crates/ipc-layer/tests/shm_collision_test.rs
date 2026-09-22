//! A live shared-memory object must not be truncated out from under its owner.
//!
//! `SharedMemory::create` used to open with `O_CREAT | O_RDWR | O_TRUNC`. A
//! second creator of the same name therefore set the object's length to ZERO
//! while the first still had it mapped, and the first process's next touch of a
//! page past the new end raised SIGBUS — in the other process, with no stack
//! pointing at the cause.
//!
//! That stayed hidden for as long as pages were faulted in lazily and sparsely
//! enough that the truncated range went untouched. Adding page prefaulting at
//! creation made it fire: 7 of 30 release runs of `nullherz-conductor`'s
//! `raw_mode_test` died with SIGBUS, 0 of 20 with `--test-threads=1`, and the
//! name racing was the hardcoded `nullherz_midi_bridge` that every `Conductor`
//! in the binary created.
//!
//! Two properties, and they are different:
//!   * the owner's mapping stays valid and readable no matter who else tries to
//!     create the same name (this file);
//!   * names are unique in the first place (`midi_bridge_shm_name`).

use ipc_layer::SharedMemory;

fn unique(tag: &str) -> String {
    format!("nullherz_test_{}_{}_{:?}", tag, std::process::id(), std::thread::current().id())
}

/// The owner writes a pattern, a second `create` of the same name is attempted,
/// and the owner's pages must still hold the pattern afterwards.
///
/// Before the fix the second create truncated the object to zero and this read
/// was a SIGBUS, which a test cannot catch — so a crash here IS the failure
/// signal, and the assert only covers the subtler corruption case.
#[test]
fn a_second_create_cannot_truncate_a_live_mapping() {
    let name = unique("collide");
    let size = 64 * 1024;

    let owner = SharedMemory::create(&name, size).expect("first create must succeed");
    unsafe {
        std::ptr::write_bytes(owner.ptr(), 0xA5, size);
    }

    // Whatever this does, it must not damage `owner`.
    let second = SharedMemory::create(&name, size);

    // Touch EVERY page, which is what prefaulting does and what turned this
    // from latent to fatal.
    let mut ok = true;
    for off in (0..size).step_by(4096) {
        let v = unsafe { std::ptr::read_volatile(owner.ptr().add(off)) };
        if v != 0xA5 { ok = false; break; }
    }
    let last = unsafe { std::ptr::read_volatile(owner.ptr().add(size - 1)) };

    assert!(ok, "the owner's pages were corrupted by a second create of the same name");
    assert_eq!(last, 0xA5, "the owner's final byte was lost — the object was truncated");

    // The second creator gets a DIFFERENT object under the same name (the
    // unlink-and-retry path, which warns on stderr). That is a visible
    // malfunction — the two sides stop seeing each other — rather than a crash
    // in the owner's address space, and it is the best POSIX allows without a
    // way to distinguish a stale object from a live one. Uniqueness is what
    // actually prevents it; this only bounds the damage.
    let second = second.expect("reclaim path must still yield a usable region");
    unsafe { std::ptr::write_bytes(second.ptr(), 0x5A, size) };
    let owner_byte = unsafe { std::ptr::read_volatile(owner.ptr()) };
    assert_eq!(
        owner_byte, 0xA5,
        "writing through the second mapping reached the owner's pages — they must be \
         separate objects once the name has been reclaimed"
    );
}

/// A stale object left behind by a crashed process must not block startup
/// forever — the reason `create` unlinks before `O_EXCL` rather than just
/// failing on any pre-existing name.
#[test]
fn a_stale_object_is_recovered_not_fatal() {
    let name = unique("stale");
    let size = 8 * 1024;

    // Leak the name deliberately: forget the owner so Drop never unlinks,
    // which is exactly the state a SIGKILLed process leaves behind.
    let leaked = SharedMemory::create(&name, size).expect("create");
    std::mem::forget(leaked);

    let recovered = SharedMemory::create(&name, size);
    assert!(
        recovered.is_ok(),
        "a stale shm object must be recovered on the next start, not block it forever"
    );
}
