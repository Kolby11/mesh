use mlua::Lua;

fn new_sandboxed_lua() -> Lua {
    let lua = Lua::new();
    lua.sandbox(true).expect("sandbox init failed");
    lua
}

thread_local! {
    /// One sandboxed Luau realm per execution thread. ScriptContext `_ENV`
    /// tables are the isolation boundary; cloning this handle does not create
    /// another VM or duplicate the standard-library heap.
    static THREAD_VM: Lua = new_sandboxed_lua();
}

/// Return a cheap handle to this thread's shared sandboxed Luau realm.
pub fn thread_vm() -> Lua {
    THREAD_VM.with(Clone::clone)
}

#[cfg(test)]
fn fresh_vm_for_benchmark() -> Lua {
    new_sandboxed_lua()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thread_vm_reuses_one_realm_per_thread() {
        let first = thread_vm();
        let second = thread_vm();
        assert_eq!(
            first.globals().to_pointer(),
            second.globals().to_pointer(),
            "same-thread handles must share one Lua realm"
        );

        let current_identity = first.globals().to_pointer() as usize;
        let other_identity = std::thread::spawn(|| thread_vm().globals().to_pointer() as usize)
            .join()
            .unwrap();
        assert_ne!(
            current_identity, other_identity,
            "different threads must own different Lua realms"
        );
    }

    #[test]
    fn thread_vm_keeps_standard_libraries_read_only() {
        let lua = thread_vm();
        let result = lua.load("string.format = function() end").exec();
        assert!(result.is_err(), "sandbox should prevent mutating stdlib");
    }

    // cargo test -p mesh-core-scripting --release -- thread_vm_handle_beats_fresh_vm_creation --ignored --nocapture
    #[test]
    #[ignore = "release-only per-thread VM creation benchmark"]
    fn thread_vm_handle_beats_fresh_vm_creation() {
        use std::time::Instant;

        let iterations = 256;
        let fresh_started = Instant::now();
        for _ in 0..iterations {
            std::hint::black_box(fresh_vm_for_benchmark());
        }
        let fresh = fresh_started.elapsed();

        // Measure on a new thread so the batch includes the one real VM that
        // must be constructed for that thread, not only steady-state clones.
        let shared = std::thread::spawn(move || {
            let shared_started = Instant::now();
            for _ in 0..iterations {
                std::hint::black_box(thread_vm());
            }
            shared_started.elapsed()
        })
        .join()
        .unwrap();
        let speedup = fresh.as_secs_f64() / shared.as_secs_f64().max(f64::EPSILON);

        let fresh_realms: Vec<_> = (0..32).map(|_| fresh_vm_for_benchmark()).collect();
        let fresh_memory: usize = fresh_realms.iter().map(Lua::used_memory).sum();
        let shared_memory = std::thread::spawn(|| thread_vm().used_memory())
            .join()
            .unwrap();

        eprintln!(
            "{iterations} script VM handles: fresh sandboxes {fresh:?}; one per-thread VM plus clones {shared:?}; ratio {speedup:.1}x"
        );
        eprintln!(
            "32 fresh VM heaps: {fresh_memory} bytes; one per-thread heap: {shared_memory} bytes"
        );
        eprintln!("MESH_PERF metric=thread_vm_checkout_speedup value={speedup:.6}");
        assert!(shared < fresh);
        assert!(
            fresh_memory > shared_memory.saturating_mul(16),
            "sharing should eliminate the duplicated standard-library heaps"
        );
    }
}
