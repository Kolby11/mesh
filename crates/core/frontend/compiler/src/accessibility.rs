use mesh_core_elements::AccessibilityRole;
use mesh_core_module::Manifest;

pub(crate) fn parse_accessibility_role(role: &str) -> AccessibilityRole {
    match role.trim().to_ascii_lowercase().as_str() {
        "button" => AccessibilityRole::Button,
        "slider" => AccessibilityRole::Slider,
        "label" => AccessibilityRole::Label,
        "text-input" | "textinput" | "text_input" => AccessibilityRole::TextInput,
        "checkbox" => AccessibilityRole::Checkbox,
        "switch" => AccessibilityRole::Switch,
        "region" => AccessibilityRole::Region,
        "list" => AccessibilityRole::List,
        "list-item" | "listitem" | "list_item" => AccessibilityRole::ListItem,
        "image" => AccessibilityRole::Image,
        "toolbar" => AccessibilityRole::Toolbar,
        "menu" => AccessibilityRole::Menu,
        "menu-item" | "menuitem" | "menu_item" => AccessibilityRole::MenuItem,
        "dialog" => AccessibilityRole::Dialog,
        "alert" => AccessibilityRole::Alert,
        "status" => AccessibilityRole::Status,
        "progress-bar" | "progressbar" | "progress_bar" => AccessibilityRole::ProgressBar,
        "tab" => AccessibilityRole::Tab,
        "tab-panel" | "tabpanel" | "tab_panel" => AccessibilityRole::TabPanel,
        "separator" => AccessibilityRole::Separator,
        custom => AccessibilityRole::Custom(custom.to_string()),
    }
}

pub fn root_accessibility_role(manifest: &Manifest) -> Option<String> {
    manifest
        .accessibility
        .as_ref()
        .and_then(|accessibility| accessibility.role.clone())
}
