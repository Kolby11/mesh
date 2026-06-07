use mlua::Lua;
use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::Arc;
use std::thread::ThreadId;

pub struct LuaVmPool {
    slots: Vec<Option<Lua>>,
    #[allow(dead_code)]
    floor: usize,
    baseline_globals: Arc<HashSet<String>>,
}

pub struct PooledVm {
    lua: Option<Lua>,
    slot_index: usize,
    owner_thread: ThreadId,
}

fn new_sandboxed_lua() -> Lua {
    let lua = Lua::new();
    lua.sandbox(true).expect("sandbox init failed");
    lua
}

impl LuaVmPool {
    pub fn new(floor: usize) -> mlua::Result<Self> {
        let mut slots = Vec::with_capacity(floor);
        for _ in 0..floor {
            let lua = Lua::new();
            lua.sandbox(true)?;
            slots.push(Some(lua));
        }
        let baseline_vm = Lua::new();
        // Collect globals BEFORE sandbox(true) — Luau's luaL_sandbox replaces the
        // globals table with a read-only proxy, making raw iteration return nothing.
        let set: HashSet<String> = baseline_vm
            .globals()
            .pairs::<String, mlua::Value>()
            .filter_map(|r| r.ok().map(|(k, _)| k))
            .collect();
        baseline_vm.sandbox(true)?;
        let baseline_globals = Arc::new(set);
        Ok(Self { slots, floor, baseline_globals })
    }

    pub fn baseline_globals(&self) -> Arc<HashSet<String>> {
        Arc::clone(&self.baseline_globals)
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn available(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }

    pub fn checkout(&mut self) -> PooledVm {
        let owner_thread = std::thread::current().id();
        for (i, slot) in self.slots.iter_mut().enumerate() {
            if slot.is_some() {
                let lua = slot.take().unwrap();
                return PooledVm { lua: Some(lua), slot_index: i, owner_thread };
            }
        }
        // Grow on demand
        let slot_index = self.slots.len();
        self.slots.push(None);
        let lua = new_sandboxed_lua();
        PooledVm { lua: Some(lua), slot_index, owner_thread }
    }

    pub(crate) fn return_slot(&mut self, slot_index: usize, lua: Lua) {
        // Phase 94 (ISO-03): call Thread::reset() and drop env_table before reinsertion.
        self.slots[slot_index] = Some(lua);
    }
}

thread_local! {
    static POOL: RefCell<LuaVmPool> =
        RefCell::new(LuaVmPool::new(4).expect("pool init"));
}

pub fn checkout() -> PooledVm {
    POOL.with(|cell| cell.borrow_mut().checkout())
}

impl PooledVm {
    pub fn lua(&self) -> &Lua {
        self.lua.as_ref().expect("PooledVm lua already taken")
    }
}

impl Drop for PooledVm {
    fn drop(&mut self) {
        assert_eq!(
            std::thread::current().id(),
            self.owner_thread,
            "PooledVm dropped on a different thread than checkout"
        );
        if let Some(lua) = self.lua.take() {
            let idx = self.slot_index;
            POOL.with(|cell| cell.borrow_mut().return_slot(idx, lua));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_floor_vms() {
        let pool = LuaVmPool::new(4).unwrap();
        assert_eq!(pool.len(), 4);
        assert_eq!(pool.available(), 4);
    }

    #[test]
    fn checkout_returns_pooled_vm_and_drop_recycles() {
        let mut pool = LuaVmPool::new(4).unwrap();
        assert_eq!(pool.available(), 4);
        let mut vm = pool.checkout();
        assert_eq!(pool.available(), 3);
        let len_before = pool.len();
        // We need to return the vm to this pool, not the thread-local.
        // Since Drop uses the thread-local, we manually return it here.
        let slot_index = vm.slot_index;
        let lua = vm.lua.take().unwrap();
        std::mem::forget(vm); // avoid Drop (which uses thread-local)
        pool.return_slot(slot_index, lua);
        assert_eq!(pool.available(), 4);
        assert_eq!(pool.len(), len_before);
    }

    #[test]
    fn pool_grows_on_demand_beyond_floor() {
        let mut pool = LuaVmPool::new(4).unwrap();
        let mut vms: Vec<PooledVm> = (0..8).map(|_| pool.checkout()).collect();
        assert!(pool.len() >= 8);
        assert_eq!(pool.available(), 0);
        // Return all via manual return to avoid thread-local confusion
        for mut vm in vms.drain(..) {
            let slot_index = vm.slot_index;
            let lua = vm.lua.take().unwrap();
            std::mem::forget(vm);
            pool.return_slot(slot_index, lua);
        }
    }

    #[test]
    fn pooled_vm_dropped_on_wrong_thread_panics() {
        let vm = checkout();
        let handle = std::thread::spawn(move || {
            drop(vm);
        });
        let result = handle.join();
        assert!(result.is_err(), "expected panic on wrong-thread drop");
        let err = result.unwrap_err();
        let msg = err
            .downcast_ref::<String>()
            .map(|s| s.as_str())
            .or_else(|| err.downcast_ref::<&str>().copied())
            .unwrap_or("");
        assert!(
            msg.contains("PooledVm dropped on a different thread"),
            "unexpected panic message: {msg}"
        );
    }

    #[test]
    fn baseline_globals_is_non_empty() {
        let pool = LuaVmPool::new(4).unwrap();
        assert!(!pool.baseline_globals().is_empty());
    }

    #[test]
    fn baseline_globals_contains_stdlib() {
        let pool = LuaVmPool::new(4).unwrap();
        assert!(pool.baseline_globals().contains("string"));
    }

    #[test]
    fn baseline_globals_excludes_host_api_keys() {
        let pool = LuaVmPool::new(4).unwrap();
        let globals = pool.baseline_globals();
        for key in &["self", "module", "mesh"] {
            assert!(!globals.contains(*key), "baseline should not contain '{key}'");
        }
    }

    #[test]
    fn sandbox_is_enabled_on_pool_vm() {
        let mut pool = LuaVmPool::new(1).unwrap();
        let mut vm = pool.checkout();
        let result = vm.lua().load("string.format = function() end").exec();
        assert!(result.is_err(), "sandbox should prevent mutating stdlib");
        let slot_index = vm.slot_index;
        let lua = vm.lua.take().unwrap();
        std::mem::forget(vm);
        pool.return_slot(slot_index, lua);
    }
}
