use mesh_component::template::SourceTag;

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
        SourceTag::Stack => UiTag::Box,
        SourceTag::ScrollView => UiTag::Scroll,
        SourceTag::Spacer => UiTag::Spacer,
        SourceTag::Separator => UiTag::Separator,
        SourceTag::Text | SourceTag::Label => UiTag::Text,
        SourceTag::Icon => UiTag::Icon,
        SourceTag::Image => UiTag::Image,
        SourceTag::Button | SourceTag::IconButton => UiTag::Button,
        SourceTag::Input
        | SourceTag::TextInput
        | SourceTag::PasswordInput
        | SourceTag::SearchInput
        | SourceTag::NumberInput
        | SourceTag::EmailInput
        | SourceTag::UrlInput => UiTag::Input,
        SourceTag::Slider => UiTag::Slider,
        SourceTag::Switch | SourceTag::Checkbox => UiTag::Toggle,
        SourceTag::List => UiTag::List,
        SourceTag::ListItem => UiTag::ListItem,
        SourceTag::Slot => UiTag::Box,
        SourceTag::Surface | SourceTag::Widget => UiTag::Box,
        SourceTag::LegacyBox => UiTag::Box,
        SourceTag::LegacyScroll => UiTag::Scroll,
        SourceTag::Unknown => UiTag::Box,
    }
}
