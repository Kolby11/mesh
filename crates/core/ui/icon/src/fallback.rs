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

/// Canonical semantic fallbacks retained from the freedesktop vocabulary and
/// the original MESH shell names. These are candidates, not a second pack:
/// the active ordered chain still decides which asset wins.
pub fn semantic_fallback_names(name: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut parts = name.split('-').collect::<Vec<_>>();
    names.push(name.to_string());
    if let Some(fallback) = canonical_fallback(name) {
        names.push(fallback.to_string());
    }
    while parts.len() > 1 {
        parts.pop();
        let parent = parts.join("-");
        if !names.iter().any(|existing| existing == &parent) {
            names.push(parent.clone());
        }
        if let Some(fallback) = canonical_fallback(&parent)
            && !names.iter().any(|existing| existing == fallback)
        {
            names.push(fallback.to_string());
        }
    }
    names
}

/// Describe which documented fallback stage produced `candidate`.
pub fn fallback_stage(original: &str, candidate: &str) -> &'static str {
    if original == candidate {
        return "exact";
    }
    let mut parts = original.split('-').collect::<Vec<_>>();
    while parts.len() > 1 {
        parts.pop();
        if parts.join("-") == candidate {
            return "dash-generalization";
        }
    }
    if semantic_fallback_names(original)
        .iter()
        .any(|name| name == candidate)
    {
        "canonical-semantic"
    } else {
        "fallback"
    }
}

fn canonical_fallback(name: &str) -> Option<&'static str> {
    Some(match name {
        "audio-volume-high" | "audio-volume-medium" | "audio-volume-low" => "volume",
        "audio-volume-muted" => "volume-off",
        "network-wireless" => "wifi",
        "settings" => "preferences-system",
        "weather-clear-night" => "weather-clear",
        "battery-empty" | "battery-caution" => "battery-low",
        "battery-good" | "battery-full" => "battery",
        "close" => "window-close",
        "star" => "starred",
        "warning" => "dialog-warning",
        "wifi" => "network-wireless",
        "volume" => "audio-volume-high",
        "volume-off" => "audio-volume-muted",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::semantic_fallback_names;

    #[test]
    fn semantic_fallbacks_keep_canonical_names_before_dash_parents() {
        assert_eq!(
            semantic_fallback_names("audio-volume-muted"),
            vec!["audio-volume-muted", "volume-off", "audio-volume", "audio"]
        );
        assert_eq!(
            semantic_fallback_names("network-wireless-signal-weak"),
            vec![
                "network-wireless-signal-weak",
                "network-wireless-signal",
                "network-wireless",
                "wifi",
                "network",
            ]
        );
    }
}
