use mesh_core_component::template::SourceTag;

/// Runtime primitive tag set.
///
/// Every source tag is lowered to one of these by `lower_source_tag` before
/// `WidgetNode` construction. This is the only tag vocabulary the layout
/// engine, style resolver, and painter need to understand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiTag {
    Row,
    Column,
    Box,
    Text,
    Button,
    Input,
    Slider,
    Scroll,
    Icon,
    Image,
    List,
    ListItem,
    Separator,
    Spacer,
    Toggle,
}

impl UiTag {
    /// The string used by the runtime (layout engine, painter, style resolver).
    pub fn as_str(&self) -> &'static str {
        match self {
            UiTag::Row => "row",
            UiTag::Column => "column",
            UiTag::Box => "box",
            UiTag::Text => "text",
            UiTag::Button => "button",
            UiTag::Input => "input",
            UiTag::Slider => "slider",
            UiTag::Scroll => "scroll",
            UiTag::Icon => "icon",
            UiTag::Image => "icon",
            UiTag::List => "column",
            UiTag::ListItem => "row",
            UiTag::Separator => "box",
            UiTag::Spacer => "box",
            UiTag::Toggle => "input",
        }
    }
}

/// Lower a `SourceTag` to the runtime `UiTag` primitive.
///
/// This is the explicit lowering step that replaces the old ad-hoc
/// `normalize_tag()` string function.
pub(crate) fn lower_source_tag(source_tag: &SourceTag) -> UiTag {
    match source_tag {
        SourceTag::Panel => UiTag::Box,
        SourceTag::Row => UiTag::Row,
        SourceTag::Column => UiTag::Column,
        SourceTag::Grid => UiTag::Box,
        SourceTag::Stack => UiTag::Box,
        SourceTag::ScrollView | SourceTag::ScrollArea => UiTag::Scroll,
        SourceTag::Spacer => UiTag::Spacer,
        SourceTag::Divider | SourceTag::Separator => UiTag::Separator,
        SourceTag::Section
        | SourceTag::Header
        | SourceTag::Footer
        | SourceTag::Group
        | SourceTag::FormRow => UiTag::Box,
        SourceTag::Text
        | SourceTag::Label
        | SourceTag::Badge
        | SourceTag::Shortcut
        | SourceTag::Progress
        | SourceTag::Meter
        | SourceTag::Tooltip => UiTag::Text,
        SourceTag::Icon | SourceTag::Avatar => UiTag::Icon,
        SourceTag::Image => UiTag::Image,
        SourceTag::Button
        | SourceTag::IconButton
        | SourceTag::ToggleButton
        | SourceTag::CommandButton
        | SourceTag::LinkButton => UiTag::Button,
        SourceTag::Input
        | SourceTag::TextArea
        | SourceTag::Search
        | SourceTag::Password
        | SourceTag::NumberInput
        | SourceTag::Stepper
        | SourceTag::TextInput
        | SourceTag::PasswordInput
        | SourceTag::SearchInput
        | SourceTag::EmailInput
        | SourceTag::UrlInput => UiTag::Input,
        SourceTag::Slider => UiTag::Slider,
        SourceTag::Select
        | SourceTag::Option
        | SourceTag::Switch
        | SourceTag::Checkbox
        | SourceTag::Radio
        | SourceTag::RadioGroup
        | SourceTag::SegmentedControl => UiTag::Toggle,
        SourceTag::Menu
        | SourceTag::MenuItem
        | SourceTag::CommandItem
        | SourceTag::PreferenceRow => UiTag::Row,
        SourceTag::Popover
        | SourceTag::Dialog
        | SourceTag::Sheet
        | SourceTag::Tabs
        | SourceTag::Tab
        | SourceTag::Accordion
        | SourceTag::Details => UiTag::Box,
        SourceTag::List | SourceTag::Table | SourceTag::Tree => UiTag::List,
        SourceTag::ListItem | SourceTag::Cell | SourceTag::EmptyState => UiTag::ListItem,
        SourceTag::Slot => UiTag::Box,
        SourceTag::Surface | SourceTag::Widget => UiTag::Box,
        SourceTag::Box => UiTag::Box,
        SourceTag::Scroll => UiTag::Scroll,
        SourceTag::Unknown => UiTag::Box,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planned_element_tags_lower_to_safe_runtime_primitives() {
        assert_eq!(lower_source_tag(&SourceTag::Grid).as_str(), "box");
        assert_eq!(
            lower_source_tag(&SourceTag::SegmentedControl).as_str(),
            "input"
        );
        assert_eq!(lower_source_tag(&SourceTag::Select).as_str(), "input");
        assert_eq!(lower_source_tag(&SourceTag::MenuItem).as_str(), "row");
        assert_eq!(lower_source_tag(&SourceTag::EmptyState).as_str(), "row");
    }

    #[test]
    fn existing_shipped_tags_keep_current_lowering() {
        assert_eq!(lower_source_tag(&SourceTag::Row).as_str(), "row");
        assert_eq!(lower_source_tag(&SourceTag::Box).as_str(), "box");
        assert_eq!(lower_source_tag(&SourceTag::Button).as_str(), "button");
        assert_eq!(lower_source_tag(&SourceTag::Text).as_str(), "text");
        assert_eq!(lower_source_tag(&SourceTag::Icon).as_str(), "icon");
        assert_eq!(lower_source_tag(&SourceTag::Input).as_str(), "input");
        assert_eq!(lower_source_tag(&SourceTag::Switch).as_str(), "input");
        assert_eq!(lower_source_tag(&SourceTag::Checkbox).as_str(), "input");
    }
}
