//! Report what this machine will actually give the audio thread.
//!
//! Run before blaming the DSP for a dropout — most "audio problems" on Linux are
//! this output. Everything here is read back from the kernel rather than
//! inferred from what was requested.
//!
//!   cargo run --release -p ipc-layer --example rt_status

use ipc_layer::SchedStatus;

fn main() {
    println!("=== scheduling ===");
    println!("  this thread, before        : {}", SchedStatus::current());
    match ipc_layer::set_rt_priority(85) {
        Ok(()) => println!("  set_rt_priority(85)        : Ok"),
        Err(e) => println!("  set_rt_priority(85)        : Err — {e}"),
    }
    let got = SchedStatus::current();
    println!("  this thread, after         : {got}");
    println!("  realtime obtained          : {}", got.is_realtime());
    println!("  RLIMIT_RTPRIO              : {}",
        ipc_layer::rtprio_limit().map(|l| l.to_string()).unwrap_or_else(|| "unreadable".into()));
    if got.is_realtime() && got.priority < 85 {
        println!("  NOTE: asked for 85, got {} — RTKit's ceiling, not the rlimit path.", got.priority);
    }

    println!("\n=== memory ===");
    match ipc_layer::lock_memory() {
        // Deliberately not reported as a success. mlockall(MCL_FUTURE) returns
        // immediately and lets LATER allocations fail to lock once the rlimit is
        // reached, so Ok here means "the call returned", not "the audio is
        // resident". The limit below is the number that decides.
        Ok(()) => println!("  mlockall()                 : returned Ok (says nothing on its own — see the limit)"),
        Err(e) => println!("  mlockall()                 : Err — {e}"),
    }
    println!("  RLIMIT_MEMLOCK             : {}", match ipc_layer::memlock_limit() {
        Some(l) if l == u64::MAX => "unlimited".to_string(),
        Some(l) => format!("{} KiB", l / 1024),
        None => "unreadable".to_string(),
    });

    println!("\n=== cpu ===");
    println!("  governor                   : {}",
        ipc_layer::cpu_governor().unwrap_or_else(|| "unreadable".into()));
    println!("  isolated cpus (isolcpus)   : {}", ipc_layer::has_isolated_cpus());
    println!("  cores                      : {}",
        std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0));

    println!("\n=== what the engine will warn about at startup ===");
    let w = ipc_layer::realtime_environment_warnings();
    if w.is_empty() {
        println!("  (nothing — this machine is configured for realtime audio)");
    }
    for x in &w {
        println!("  * {x}\n");
    }
}
