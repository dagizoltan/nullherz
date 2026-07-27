//! Device enumeration must describe the machine it runs on, not a guess.
//!
//! The previous implementation returned a fabricated `["default", "hw:0,0"]`
//! regardless of hardware. The UI presents that list as selectable, so on a
//! machine without a `hw:0,0` the menu offered a device that could only fail to
//! open. These tests pin the two properties that made that a bug: every entry
//! must come from the driver, and every entry must be openable as written.

use nullherz_backends::alsa::{device_id, AlsaBackend};
use nullherz_backends::AudioBackend;

#[test]
fn test_enumerated_devices_are_not_the_old_fabricated_list() {
    let backend = AlsaBackend::new();
    let devices = backend.enumerate_devices();

    if devices.is_empty() {
        // No libasound on this machine — nothing to assert, and returning an
        // empty list is the honest answer.
        return;
    }

    // The old stub's signature: exactly these two, in this order, always.
    let fabricated: Vec<String> = vec!["default".to_string(), "hw:0,0".to_string()];
    assert_ne!(
        devices, fabricated,
        "enumerate_devices() returned the hardcoded placeholder list rather than \
         querying ALSA — a user picking 'hw:0,0' from this would hit an open failure \
         on a device we invented"
    );

    // A real hint query on a machine with a sound server yields more than the
    // two placeholders, or at minimum yields entries that are not `hw:0,0`
    // unless that device genuinely exists.
    assert!(
        devices.iter().any(|d| device_id(d) == "default"),
        "'default' must always be offerable — it is what we open when unconfigured; got {devices:?}"
    );
    // Entries must be distinct after the description is stripped, or the list
    // shows the same device twice (this is why `default` is deduped rather than
    // unconditionally prepended: ALSA already emits it as a hint here).
    let mut ids: Vec<&str> = devices.iter().map(|d| device_id(d)).collect();
    let before = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(before, ids.len(), "duplicate device ids in {devices:?}");
}

#[test]
fn test_every_enumerated_device_is_openable_as_written() {
    // The contract that makes the list useful: a string taken from this list
    // and handed back via set_device must be a valid ALSA device name. If the
    // human-readable suffix leaked into the id, this fails.
    let backend = AlsaBackend::new();
    for entry in backend.enumerate_devices() {
        let id = device_id(&entry);
        assert!(!id.is_empty(), "device entry {entry:?} has an empty id");
        assert!(
            !id.contains(" — "),
            "device id {id:?} still carries its description suffix — snd_pcm_open would \
             fail on it verbatim"
        );
        assert!(
            !id.contains('\0'),
            "device id {id:?} contains a NUL byte and cannot become a CString"
        );
    }
}

#[test]
fn test_set_device_strips_the_description_suffix() {
    let mut backend = AlsaBackend::new();
    backend.set_device("hw:CARD=PCH,DEV=0 — HDA Intel PCH, ALC257 Analog");
    assert_eq!(
        backend.device(),
        "hw:CARD=PCH,DEV=0",
        "a list entry pasted back verbatim must reduce to the openable id"
    );

    backend.set_device("plughw:0,0");
    assert_eq!(backend.device(), "plughw:0,0", "a bare id must pass through untouched");
}

#[test]
fn test_empty_device_falls_back_rather_than_opening_nothing() {
    let mut backend = AlsaBackend::new();
    backend.set_device("   ");
    assert_eq!(
        backend.device(),
        "default",
        "a blank device must fall back to 'default'; opening \"\" fails with an \
         error that reads like a library bug rather than a config mistake"
    );
}

#[test]
fn test_default_device_is_default_when_env_is_unset() {
    // Guard against the env override changing behaviour for users who never set
    // it. Only assert when the variable is genuinely absent, so this does not
    // fail for someone running the suite with a deliberate override.
    if std::env::var("NULLHERZ_ALSA_DEVICE").is_err() {
        assert_eq!(AlsaBackend::default_device(), "default");
    }
}

#[test]
fn test_device_id_is_idempotent() {
    // enumerate -> set_device -> enumerate round trips must not erode the id.
    for s in ["default", "hw:0,0", "pipewire", "a — b — c"] {
        let once = device_id(s).to_string();
        assert_eq!(device_id(&once), once, "device_id is not idempotent for {s:?}");
    }
}
