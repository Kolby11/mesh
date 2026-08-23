use mesh_core_component::template::SourceTag;
use mesh_core_elements::element_runtime_tag_for_tag;

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
    Unknown,
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
            UiTag::Image => "image",
            UiTag::List => "column",
            UiTag::ListItem => "row",
            UiTag::Separator => "box",
            UiTag::Spacer => "box",
            UiTag::Toggle => "input",
            UiTag::Unknown => "unknown",
        }
    }

    fn from_runtime_tag(tag: &str) -> Self {
        match tag {
            "row" => Self::Row,
            "column" => Self::Column,
            "box" => Self::Box,
            "text" => Self::Text,
            "button" => Self::Button,
            "input" => Self::Input,
            "slider" => Self::Slider,
            "scroll" => Self::Scroll,
            "icon" => Self::Icon,
            "image" => Self::Image,
            _ => Self::Unknown,
        }
    }
}

/// Lower a `SourceTag` to the runtime `UiTag` primitive.
///
/// This is the explicit lowering step that replaces the old ad-hoc
/// `normalize_tag()` string function.
pub(crate) fn lower_source_tag(source_tag: &SourceTag) -> UiTag {
    element_runtime_tag_for_tag(source_tag.as_str())
        .map(UiTag::from_runtime_tag)
        .unwrap_or(UiTag::Unknown)
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
        assert_eq!(lower_source_tag(&SourceTag::Image).as_str(), "image");
    }

    #[test]
    fn unknown_source_tags_are_not_silently_lowered_to_box() {
        assert_eq!(lower_source_tag(&SourceTag::Unknown), UiTag::Unknown);
        assert_eq!(lower_source_tag(&SourceTag::Unknown).as_str(), "unknown");
    }

    #[test]
    fn phase88_action_and_input_variants_share_native_runtime_primitives() {
        for source_tag in [
            SourceTag::Button,
            SourceTag::IconButton,
            SourceTag::ToggleButton,
            SourceTag::CommandButton,
            SourceTag::LinkButton,
        ] {
            assert_eq!(lower_source_tag(&source_tag).as_str(), "button");
        }

        for source_tag in [
            SourceTag::Input,
            SourceTag::TextArea,
            SourceTag::Search,
            SourceTag::Password,
            SourceTag::NumberInput,
            SourceTag::Stepper,
        ] {
            assert_eq!(lower_source_tag(&source_tag).as_str(), "input");
        }
    }
}
