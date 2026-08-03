use super::common::*;
use super::*;

#[test]
fn scheduler_uses_component_tick_deadline() {
    let mut shell = Shell::new();
    park_reload_deadlines(&mut shell);
    let deadline = Instant::now() + Duration::from_millis(120);
    shell
        .components
        .push(super::types::ComponentRuntime::new(Box::new(
            DeadlineTickComponent::new("@test/deadline", Some(deadline)),
        )));

    let sleep = shell.next_runtime_sleep(false);

    assert!(sleep <= Duration::from_millis(120), "{sleep:?}");
    assert!(sleep >= Duration::from_millis(80), "{sleep:?}");
}

#[test]
fn scheduler_wakes_immediately_for_due_component_tick_deadline() {
    let mut shell = Shell::new();
    park_reload_deadlines(&mut shell);
    shell
        .components
        .push(super::types::ComponentRuntime::new(Box::new(
            DeadlineTickComponent::new(
                "@test/deadline",
                Some(Instant::now() - Duration::from_millis(1)),
            ),
        )));

    assert_eq!(shell.next_runtime_sleep(false), Duration::ZERO);
}

#[test]
fn scheduler_wakes_for_visible_dirty_component_even_without_previous_present() {
    let state = Arc::new(Mutex::new(DirtyHiddenState::default()));
    let mut shell = Shell::new();
    park_reload_deadlines(&mut shell);
    shell.register_component(Box::new(DirtyHiddenComponent::new(
        "@test/dirty",
        None,
        Arc::clone(&state),
    )));
    {
        shell
            .core
            .surfaces
            .get_mut("@test/dirty")
            .expect("registered core surface")
            .visible = true;
        let surface = shell
            .surfaces
            .get_mut("@test/dirty")
            .expect("registered surface target");
        surface.visible = true;
        surface.width = 120;
        surface.height = 36;
    }
    shell.presented_last_frame = false;

    assert_eq!(shell.next_runtime_sleep(false), Duration::ZERO);
}

#[test]
fn scheduler_ignores_hidden_component_deadlines_and_render_dirtiness() {
    let state = Arc::new(Mutex::new(DirtyHiddenState::default()));
    let mut shell = Shell::new();
    park_reload_deadlines(&mut shell);
    shell.register_component(Box::new(DirtyHiddenComponent::new(
        "@test/hidden",
        Some(Instant::now()),
        Arc::clone(&state),
    )));
    shell
        .core
        .surfaces
        .get_mut("@test/hidden")
        .expect("hidden surface state")
        .visible = false;
    shell.presented_last_frame = true;

    let sleep = shell.next_runtime_sleep(false);

    assert!(
        sleep >= Duration::from_secs(30),
        "hidden dirty component should not force an immediate wake: {sleep:?}"
    );
}

#[test]
fn render_skips_already_hidden_dirty_surface() {
    let state = Arc::new(Mutex::new(DirtyHiddenState::default()));
    let mut shell = Shell::new();
    shell.register_component(Box::new(DirtyHiddenComponent::new(
        "@test/hidden",
        None,
        Arc::clone(&state),
    )));
    shell
        .core
        .surfaces
        .get_mut("@test/hidden")
        .expect("hidden surface state")
        .visible = false;

    shell.render_components().unwrap();

    assert_eq!(state.lock().unwrap().render_calls, 0);
}
