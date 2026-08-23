use crate::pseudo_state::{PseudoState, pseudo_state_mask};
use crate::tree::ElementState;

// Kept private for the existing resolver tests; the values come from the
// canonical table rather than maintaining a second bit assignment.
#[cfg(test)]
pub(super) const STATE_HOVERED: u32 = crate::pseudo_state::PSEUDO_STATE_TABLE[0].bit;
#[cfg(test)]
pub(super) const STATE_FOCUSED: u32 = crate::pseudo_state::PSEUDO_STATE_TABLE[1].bit;
#[cfg(test)]
pub(super) const STATE_ACTIVE: u32 = crate::pseudo_state::PSEUDO_STATE_TABLE[2].bit;
#[cfg(test)]
pub(super) const STATE_FULLSCREEN: u32 = crate::pseudo_state::PSEUDO_STATE_TABLE[13].bit;
#[cfg(test)]
pub(super) const STATE_MAXIMIZED: u32 = crate::pseudo_state::PSEUDO_STATE_TABLE[14].bit;
#[cfg(test)]
pub(super) const STATE_ACTIVATED: u32 = crate::pseudo_state::PSEUDO_STATE_TABLE[15].bit;
#[cfg(test)]
pub(super) const STATE_TILED: u32 = crate::pseudo_state::PSEUDO_STATE_TABLE[16].bit;
#[cfg(test)]
pub(super) const STATE_WINDOWED: u32 = crate::pseudo_state::PSEUDO_STATE_TABLE[17].bit;

pub(super) fn active_state_mask(state: ElementState) -> u32 {
    pseudo_state_mask(state)
}

pub(super) fn state_name_bit(state: &str) -> Option<u32> {
    PseudoState::from_name(state).map(|state| state.spec().bit)
}
