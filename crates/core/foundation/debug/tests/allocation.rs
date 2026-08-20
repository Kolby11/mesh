#[test]
fn allocation_profiling_is_unavailable_without_an_installed_allocator() {
    assert!(!mesh_core_debug::allocation::profiling_available());
}
