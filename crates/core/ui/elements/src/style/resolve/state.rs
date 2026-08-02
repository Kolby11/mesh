use crate::tree::ElementState;

pub(super) const STATE_HOVERED: u32 = 1 << 0;
pub(super) const STATE_FOCUSED: u32 = 1 << 1;
pub(super) const STATE_ACTIVE: u32 = 1 << 2;
pub(super) const STATE_DISABLED: u32 = 1 << 3;
pub(super) const STATE_READ_ONLY: u32 = 1 << 4;
pub(super) const STATE_REQUIRED: u32 = 1 << 5;
pub(super) const STATE_SELECTED: u32 = 1 << 6;
pub(super) const STATE_CHECKED: u32 = 1 << 7;
pub(super) const STATE_EXPANDED: u32 = 1 << 8;
pub(super) const STATE_PRESSED: u32 = 1 << 9;
pub(super) const STATE_INVALID: u32 = 1 << 10;
pub(super) const STATE_VALUE: u32 = 1 << 11;
pub(super) const STATE_FOCUS_VISIBLE: u32 = 1 << 12;
pub(super) const STATE_FULLSCREEN: u32 = 1 << 13;
pub(super) const STATE_MAXIMIZED: u32 = 1 << 14;
pub(super) const STATE_ACTIVATED: u32 = 1 << 15;
pub(super) const STATE_TILED: u32 = 1 << 16;
pub(super) const STATE_WINDOWED: u32 = 1 << 17;

pub(super) fn active_state_mask(state: ElementState) -> u32 {
    let mut mask = 0;
    if state.hovered {
        mask |= STATE_HOVERED;
    }
    if state.focused {
        mask |= STATE_FOCUSED;
    }
    if state.active {
        mask |= STATE_ACTIVE;
    }
    if state.disabled {
        mask |= STATE_DISABLED;
    }
    if state.read_only {
        mask |= STATE_READ_ONLY;
    }
    if state.required {
        mask |= STATE_REQUIRED;
    }
    if state.selected {
        mask |= STATE_SELECTED;
    }
    if state.checked {
        mask |= STATE_CHECKED;
    }
    if state.expanded {
        mask |= STATE_EXPANDED;
    }
    if state.pressed {
        mask |= STATE_PRESSED;
    }
    if state.invalid {
        mask |= STATE_INVALID;
    }
    if state.value {
        mask |= STATE_VALUE;
    }
    if state.focus_visible {
        mask |= STATE_FOCUS_VISIBLE;
    }
    if state.window.windowed {
        mask |= STATE_WINDOWED;
    }
    if state.window.fullscreen {
        mask |= STATE_FULLSCREEN;
    }
    if state.window.maximized {
        mask |= STATE_MAXIMIZED;
    }
    if state.window.activated {
        mask |= STATE_ACTIVATED;
    }
    if state.window.tiled {
        mask |= STATE_TILED;
    }
    mask
}

pub(super) fn state_name_bit(state: &str) -> Option<u32> {
    match state {
        "hover" | "hovered" => Some(STATE_HOVERED),
        "focus" | "focused" => Some(STATE_FOCUSED),
        "active" => Some(STATE_ACTIVE),
        "disabled" => Some(STATE_DISABLED),
        "readonly" => Some(STATE_READ_ONLY),
        "required" => Some(STATE_REQUIRED),
        "selected" => Some(STATE_SELECTED),
        "checked" => Some(STATE_CHECKED),
        "expanded" => Some(STATE_EXPANDED),
        "pressed" => Some(STATE_PRESSED),
        "invalid" => Some(STATE_INVALID),
        "value" => Some(STATE_VALUE),
        "focus-visible" => Some(STATE_FOCUS_VISIBLE),
        "windowed" => Some(STATE_WINDOWED),
        "fullscreen" => Some(STATE_FULLSCREEN),
        "maximized" => Some(STATE_MAXIMIZED),
        "activated" => Some(STATE_ACTIVATED),
        "tiled" => Some(STATE_TILED),
        _ => None,
    }
}
