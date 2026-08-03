use super::common::*;
use super::*;

#[test]
fn selection_input_contract_key_pressed_preserves_modifiers() {
    let input = component_key_pressed_input("C".into(), true, false, true);
    match input {
        ComponentInput::KeyPressed { key, modifiers } => {
            assert_eq!(key, "C");
            assert!(modifiers.ctrl);
            assert!(!modifiers.shift);
            assert!(modifiers.alt);
        }
        other => panic!("expected key press input, got {other:?}"),
    }
}

#[test]
fn selection_input_contract_key_released_preserves_modifiers() {
    let input = component_key_released_input(
        "Enter".into(),
        KeyModifiers {
            ctrl: true,
            shift: true,
            alt: false,
        },
    );
    match input {
        ComponentInput::KeyReleased { key, modifiers } => {
            assert_eq!(key, "Enter");
            assert!(modifiers.ctrl);
            assert!(modifiers.shift);
            assert!(!modifiers.alt);
        }
        other => panic!("expected key release input, got {other:?}"),
    }
}

#[test]
fn selection_input_contract_debug_shortcuts_remain_global() {
    assert!(matches!(
        shell_global_shortcut_request("d", true, true, false),
        Some(CoreRequest::ToggleDebugOverlay)
    ));
    assert!(matches!(
        shell_global_shortcut_request("Tab", true, false, true),
        Some(CoreRequest::CycleDebugTab)
    ));
    assert!(matches!(
        shell_global_shortcut_request("c", true, true, false),
        Some(CoreRequest::ToggleDebugElementPicker)
    ));
}

#[test]
fn keyboard_shortcuts_shell_global_shortcuts_still_win() {
    assert!(matches!(
        shell_global_shortcut_request("d", true, true, false),
        Some(CoreRequest::ToggleDebugOverlay)
    ));
    assert!(matches!(
        shell_global_shortcut_request("Tab", true, false, true),
        Some(CoreRequest::CycleDebugTab)
    ));
}

#[test]
fn keyboard_regression_shell_global_shortcut_precedence_stays_global() {
    assert!(matches!(
        shell_global_shortcut_request("d", true, true, false),
        Some(CoreRequest::ToggleDebugOverlay)
    ));
    assert!(matches!(
        shell_global_shortcut_request("Tab", true, false, true),
        Some(CoreRequest::CycleDebugTab)
    ));
    assert!(shell_global_shortcut_request("m", false, false, false).is_none());
}

#[test]
fn selection_clipboard_shell_request_writes_text() {
    let mut shell = Shell::new();
    let writes = Arc::new(Mutex::new(Vec::new()));
    shell.clipboard = Box::new(RecordingClipboard {
        writes: writes.clone(),
    });

    shell
        .apply_request(CoreRequest::WriteClipboard {
            text: "proof copy".into(),
        })
        .unwrap();

    assert_eq!(writes.lock().unwrap().as_slice(), ["proof copy"]);
}
