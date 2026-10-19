use super::super::*;
use super::common::*;
use mesh_core_capability::CapabilitySet;
use mesh_core_elements::VariableStore;

#[test]
fn frontend_storage_flushes_on_unmount_and_loads_before_init() {
    let root = temp_storage_root("frontend-flush");
    let caps = CapabilitySet::default();
    let mut writer =
        ScriptContext::new_with_storage_root("@mesh/storage-lifecycle", caps.clone(), &root)
            .unwrap();
    writer
        .load_script(
            r#"
function init(self)
    self.storage.counter = 1
end

function render(self)
    self.storage.counter = 2
end

function unmount(self)
    self.storage.counter = 3
end
"#,
        )
        .unwrap();

    writer.call_init().unwrap();
    writer.call_render_lifecycle().unwrap();

    let mut before_flush =
        ScriptContext::new_with_storage_root("@mesh/storage-lifecycle", caps.clone(), &root)
            .unwrap();
    before_flush
        .load_script("function init(self)\nloaded = self.storage.counter\nend")
        .unwrap();
    before_flush.call_init().unwrap();
    assert_eq!(before_flush.state.get("loaded"), None);

    writer.call_handler("unmount", &[]).unwrap();

    let mut reader =
        ScriptContext::new_with_storage_root("@mesh/storage-lifecycle", caps, &root).unwrap();
    reader
        .load_script("function init(self)\nloaded = self.storage.counter\nend")
        .unwrap();
    reader.call_init().unwrap();
    assert_eq!(reader.state.get("loaded"), Some(serde_json::json!(3)));
}

#[test]
fn frontend_storage_flushes_when_unmount_hook_fails() {
    let root = temp_storage_root("frontend-flush-error");
    let caps = CapabilitySet::default();
    let mut writer =
        ScriptContext::new_with_storage_root("@mesh/storage-lifecycle-error", caps.clone(), &root)
            .unwrap();
    writer
        .load_script(
            r#"
function unmount(self)
    self.storage.counter = 7
    error("cleanup failed")
end
"#,
        )
        .unwrap();

    assert!(writer.call_handler("unmount", &[]).is_err());

    let mut reader =
        ScriptContext::new_with_storage_root("@mesh/storage-lifecycle-error", caps, &root).unwrap();
    reader
        .load_script("function init(self) loaded = self.storage.counter end")
        .unwrap();
    reader.call_init().unwrap();
    assert_eq!(reader.state.get("loaded"), Some(serde_json::json!(7)));
}

#[test]
fn frontend_storage_is_isolated_by_component_instance() {
    let root = temp_storage_root("frontend-instance-scope");
    let caps = CapabilitySet::default();
    let mut first = ScriptContext::new_with_storage_scope(
        "@mesh/module",
        "@mesh/component",
        "panel/first",
        caps.clone(),
        &root,
    )
    .unwrap();
    first
        .load_script("function unmount(self) self.storage.value = 'first' end")
        .unwrap();
    first.call_handler("unmount", &[]).unwrap();

    let mut second = ScriptContext::new_with_storage_scope(
        "@mesh/module",
        "@mesh/component",
        "panel/second",
        caps,
        &root,
    )
    .unwrap();
    second
        .load_script("function init(self) loaded = self.storage.value end")
        .unwrap();
    second.call_init().unwrap();

    assert_eq!(second.state.get("loaded"), None);
}

#[test]
fn frontend_storage_persistence_failure_is_diagnostic_and_keeps_memory_state() {
    let root = temp_storage_root("frontend-failure");
    std::fs::write(&root, "not a directory").unwrap();
    let caps = CapabilitySet::default();
    let mut ctx =
        ScriptContext::new_with_storage_root("@mesh/storage-failure", caps, &root).unwrap();
    ctx.load_script(
        r#"
function init(self)
    self.storage.value = "kept"
end

function render(self)
    latest = self.storage.value
end

function unmount(self)
end
"#,
    )
    .unwrap();

    ctx.call_init().unwrap();
    ctx.call_handler("unmount", &[]).unwrap();
    ctx.call_render_lifecycle().unwrap();

    assert_eq!(ctx.state.get("latest"), Some(serde_json::json!("kept")));
    let diagnostics = ctx.drain_diagnostics();
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.reason.contains("storage persistence failed"))
    );
}

#[test]
fn frontend_storage_render_reads_track_only_watched_key_writes() {
    let caps = CapabilitySet::default();
    let mut ctx = ScriptContext::new("@mesh/storage-watch", caps).unwrap();
    ctx.load_script(
        r#"
function render(self)
    rendered_theme = self.storage.theme
end

function set_watched()
    self.storage.theme = "dark"
end

function set_unwatched()
    self.storage.locale = "sk"
end
"#,
    )
    .unwrap();

    ctx.state_mut().clear_dirty();
    ctx.call_render_lifecycle().unwrap();
    assert!(ctx.tracked_storage_keys().contains("theme"));

    ctx.state_mut().clear_dirty();
    ctx.call_handler("set_unwatched", &[]).unwrap();
    assert!(!ctx.state().is_dirty());

    ctx.call_handler("set_watched", &[]).unwrap();
    assert!(ctx.state().is_dirty());
}
