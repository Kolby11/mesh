/// Meta block — component metadata and accessibility defaults.
use serde::{Deserialize, Serialize};

/// Metadata for a component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaBlock {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub role: Option<AccessibilityRole>,
    #[serde(default)]
    pub label: Option<String>,
}

/// Accessibility roles for semantic tree construction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AccessibilityRole {
    Button,
    Slider,
    Label,
    TextInput,
    Checkbox,
    Switch,
    Region,
    List,
    ListItem,
    Image,
    Toolbar,
    Menu,
    MenuItem,
    Dialog,
    Alert,
    Status,
    ProgressBar,
    Tab,
    TabPanel,
    Separator,
    #[serde(untagged)]
    Custom(String),
}

impl std::fmt::Display for AccessibilityRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Custom(s) => write!(f, "{s}"),
            other => write!(f, "{other:?}"),
        }
    }
}
