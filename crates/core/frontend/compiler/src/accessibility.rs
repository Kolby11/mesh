use mesh_core_elements::AccessibilityRole;
use mesh_core_module::Manifest;

pub(crate) fn parse_accessibility_role(role: &str) -> AccessibilityRole {
    AccessibilityRole::from_name(role)
}

pub fn root_accessibility_role(manifest: &Manifest) -> Option<String> {
    manifest
        .accessibility
        .as_ref()
        .and_then(|accessibility| accessibility.role.clone())
}
