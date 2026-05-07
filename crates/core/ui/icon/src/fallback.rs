/// Marker for the synthetic "missing icon" pack id used by `BuiltInIconFallback`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltInIconFallback;

impl BuiltInIconFallback {
    pub const NAME: &'static str = "__mesh_builtin_missing_icon";
}

/// Built-in "missing icon" SVG embedded in the binary. Rendered when every
/// resolution chain fails so the user never sees an invisible icon. Uses
/// `currentColor` for both stroke and fill so the painter's tint flows
/// through the same path as a regular monochrome SVG.
///
/// Visual: rounded square outline with a question mark inside — the
/// canonical "broken / unknown" affordance.
pub const MISSING_ICON_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="3"/><path d="M9.5 9a2.5 2.5 0 0 1 5 0c0 1.5-2.5 2-2.5 4"/><circle cx="12" cy="17" r="0.6" fill="currentColor"/></svg>"##;
