use crate::policy::RuntimePolicy;
use mlua::Lua;

#[derive(Clone)]
struct ThreadRealm {
    lua: Lua,
    policy: RuntimePolicy,
}

fn new_sandboxed_realm() -> ThreadRealm {
    let lua = Lua::new();
    let policy = RuntimePolicy::default();
    policy.install(&lua).expect("sandbox policy init failed");
    ThreadRealm { lua, policy }
}

thread_local! {
    /// One sandboxed Luau realm per execution thread. ScriptContext `_ENV`
    /// tables are the isolation boundary; cloning this handle does not create
    /// another VM or duplicate the standard-library heap.
    static THREAD_REALM: ThreadRealm = new_sandboxed_realm();
}

/// Return a cheap handle to this thread's shared sandboxed Luau realm.
pub fn thread_vm() -> Lua {
    THREAD_REALM.with(|realm| realm.lua.clone())
}

/// Return the policy and host-resource broker for this thread's shared realm.
pub(crate) fn thread_policy() -> RuntimePolicy {
    THREAD_REALM.with(|realm| realm.policy.clone())
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
}
