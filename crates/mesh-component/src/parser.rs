/// Parser for `.mesh` single-file components.
///
/// Splits the source into top-level blocks (`<template>`, `<script>`, `<style>`,
/// `<i18n>`) then parses each block with parser libraries.
mod markup;
mod script;
mod styles;

use crate::{ComponentFile, ComponentImportTarget};
use markup::parse_markup;
use script::{extract_imports, parse_script};
use std::collections::HashMap;
use styles::parse_style;

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("unclosed block <{tag}> opened at line {line}")]
    UnclosedBlock { tag: String, line: usize },

    #[error("unexpected closing tag </{tag}> at line {line}")]
    UnexpectedClose { tag: String, line: usize },

    #[error("invalid template syntax: {message}")]
    InvalidTemplate { message: String },

    #[error("invalid style syntax at line {line}: {message}")]
    InvalidStyle { message: String, line: usize },

    #[error("invalid i18n block: {0}")]
    InvalidI18n(String),

    #[error("invalid import at line {line}: {message}")]
    InvalidImport { line: usize, message: String },

    #[error("unknown block <{name}> at line {line}")]
    UnknownBlock { name: String, line: usize },
}

pub fn parse_component(source: &str) -> Result<ComponentFile, ParseError> {
    let blocks = extract_blocks(source)?;

    let (imports, script_source) = if let Some(s) = blocks.get("script") {
        let (imports, stripped) = extract_imports(s)?;
        (imports, Some(stripped))
    } else {
        (Vec::new(), None)
    };
    let imported_components: std::collections::HashSet<String> = imports
        .iter()
        .filter(|import| {
            matches!(
                import.target,
                ComponentImportTarget::ComponentLocal(_)
                    | ComponentImportTarget::ComponentPlugin(_)
            )
        })
        .map(|import| import.alias.clone())
        .collect();

    let template = blocks
        .get("template")
        .map(|s| parse_markup(s, &imported_components))
        .transpose()?;

    let script = script_source.map(|s| parse_script(&s));

    let style = blocks.get("style").map(|s| parse_style(s)).transpose()?;

    Ok(ComponentFile {
        imports,
        template,
        script,
        style,
    })
}

fn extract_blocks(source: &str) -> Result<HashMap<String, String>, ParseError> {
    let mut blocks = HashMap::new();
    let known_tags = ["template", "script", "style", "i18n"];

    let mut remaining = source;
    let mut line_offset = 1;

    while !remaining.is_empty() {
        let Some(open_start) = remaining.find('<') else {
            break;
        };

        let before = &remaining[..open_start];
        line_offset += before.chars().filter(|&c| c == '\n').count();

        let after_open = &remaining[open_start + 1..];
        let Some(open_end) = after_open.find('>') else {
            break;
        };

        let tag_content = &after_open[..open_end];
        let tag_name = tag_content.split_whitespace().next().unwrap_or("");

        if tag_name.starts_with('/') || tag_name.is_empty() {
            remaining = &after_open[open_end + 1..];
            continue;
        }

        if !known_tags.contains(&tag_name) {
            remaining = &after_open[open_end + 1..];
            continue;
        }

        let close_tag = format!("</{tag_name}>");
        let body_start = open_start + 1 + open_end + 1;
        let body_source = &remaining[body_start..];

        let Some(close_pos) = body_source.find(close_tag.as_str()) else {
            return Err(ParseError::UnclosedBlock {
                tag: tag_name.to_string(),
                line: line_offset,
            });
        };

        blocks.insert(tag_name.to_string(), body_source[..close_pos].to_string());

        let skip = body_start + close_pos + close_tag.len();
        line_offset += remaining[..skip].chars().filter(|&c| c == '\n').count();
        remaining = &remaining[skip..];
    }

    Ok(blocks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ComponentImportTarget, ScriptLang,
        style::{ContainerQuery, Selector, StyleValue},
        template::{AttributeValue, TemplateNode},
    };

    #[test]
    fn parse_minimal_component() {
        let source = r#"
<template>
  <text>Hello</text>
</template>
"#;
        let file = parse_component(source).unwrap();
        assert!(file.template.is_some());
        assert!(file.script.is_none());
        assert!(file.style.is_none());
    }

    #[test]
    fn rejects_removed_html_compat_tags() {
        let source = r#"
<template>
  <div>Hello</div>
</template>
"#;
        let err = parse_component(source).unwrap_err();
        assert!(
            err.to_string().contains("unknown UI tag <div>"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn parse_full_component() {
        let source = r#"
<template>
  <box>
    <text class="title">{ title }</text>
    <button onclick="onTap">Click me</button>
  </box>
</template>

<script lang="luau">
local title = "Hello"
function onTap()
    title = "Clicked!"
end
</script>

<style>
.title {
    color: token(color.on-surface);
    font-size: token(typography.size.lg);
}

button {
    background: token(color.primary);
    padding: 8px;
}
</style>

<i18n>
[en]
greeting = "Hello"

[fr]
greeting = "Bonjour"
</i18n>
"#;
        let file = parse_component(source).unwrap();

        let tmpl = file.template.unwrap();
        assert_eq!(tmpl.root.len(), 1);

        let script = file.script.unwrap();
        assert_eq!(script.lang, ScriptLang::Luau);
        assert!(script.source.contains("function onTap"));

        let style = file.style.unwrap();
        assert_eq!(style.rules.len(), 2);
        match &style.rules[0].declarations[0].value {
            StyleValue::Token(name) => assert_eq!(name, "color.on-surface"),
            other => panic!("expected token, got {other:?}"),
        }
    }

    #[test]
    fn parse_expression_interpolation() {
        let source = r#"
<template>
  <text>Time: { formatTime(time) }</text>
</template>
"#;
        let file = parse_component(source).unwrap();
        let tmpl = file.template.unwrap();
        match &tmpl.root[0] {
            TemplateNode::Element(el) => {
                assert_eq!(el.tag, "text");
                assert!(!el.children.is_empty());
            }
            _ => panic!("expected element"),
        }
    }

    #[test]
    fn parse_style_tokens_and_literals() {
        let source = r#"
<style>
box {
    gap: 8px;
    padding: token(spacing.md);
    background: var(--bg);
}
</style>
"#;
        let file = parse_component(source).unwrap();
        let style = file.style.unwrap();
        let decls = &style.rules[0].declarations;
        assert!(matches!(&decls[0].value, StyleValue::Literal(v) if v == "8px"));
        assert!(matches!(&decls[1].value, StyleValue::Token(v) if v == "spacing.md"));
        assert!(matches!(&decls[2].value, StyleValue::Var(v) if v == "--bg"));
        assert!(style.rules[0].container_query.is_none());
    }

    #[test]
    fn parse_grouped_selectors_into_multiple_rules() {
        let source = r#"
<style>
.panel, #main {
    color: #fff;
}
</style>
"#;
        let file = parse_component(source).unwrap();
        let style = file.style.unwrap();
        assert_eq!(style.rules.len(), 2);
        assert!(matches!(&style.rules[0].selector, Selector::Class(name) if name == "panel"));
        assert!(matches!(&style.rules[1].selector, Selector::Id(name) if name == "main"));
    }

    #[test]
    fn parse_container_query_rules() {
        let source = r#"
<style>
@container (max-width: 640px) {
    .sidebar {
        width: 100%;
        overflow-y: auto;
    }
}
</style>
"#;
        let file = parse_component(source).unwrap();
        let style = file.style.unwrap();
        assert_eq!(style.rules.len(), 1);
        let rule = &style.rules[0];
        assert_eq!(
            rule.container_query,
            Some(ContainerQuery {
                max_width: Some(640.0),
                ..Default::default()
            })
        );
        assert!(matches!(&rule.selector, Selector::Class(name) if name == "sidebar"));
    }

    #[test]
    fn parse_self_closing_element() {
        let source = r#"
<template>
  <icon name="battery" size="24"/>
</template>
"#;
        let file = parse_component(source).unwrap();
        let tmpl = file.template.unwrap();
        match &tmpl.root[0] {
            TemplateNode::Element(el) => {
                assert_eq!(el.tag, "icon");
                assert_eq!(el.children.len(), 0);
                assert_eq!(el.attributes.len(), 2);
            }
            _ => panic!("expected self-closing element"),
        }
    }

    #[test]
    fn parse_binding_and_event_attributes() {
        let source = r#"
<template>
  <input value="{volume}" onchange="onVolumeChange"/>
</template>
"#;
        let file = parse_component(source).unwrap();
        let tmpl = file.template.unwrap();
        match &tmpl.root[0] {
            TemplateNode::Element(el) => {
                assert!(
                    matches!(&el.attributes[0].value, AttributeValue::Binding(v) if v == "volume")
                );
                assert!(
                    matches!(&el.attributes[1].value, AttributeValue::EventHandler(v) if v == "onVolumeChange")
                );
            }
            _ => panic!("expected element"),
        }
    }

    #[test]
    fn parse_unquoted_brace_event_attribute() {
        let source = r#"
<template>
  <button onclick={onTap}>Click</button>
</template>
"#;
        let file = parse_component(source).unwrap();
        let tmpl = file.template.unwrap();
        match &tmpl.root[0] {
            TemplateNode::Element(el) => {
                assert!(
                    matches!(&el.attributes[0].value, AttributeValue::EventHandler(v) if v == "onTap")
                );
            }
            _ => panic!("expected element"),
        }
    }

    #[test]
    fn script_source_passed_through_unchanged() {
        let source = r#"
<template>
  <text>{title}</text>
</template>

<script lang="luau">
mesh.state.set("title", "Hello")
mesh.state.set("count", 0)

function onTap()
    local tmp = count + 1
    count = tmp
end
</script>
"#;
        let file = parse_component(source).unwrap();
        let script = file.script.unwrap();
        assert!(
            script
                .source
                .contains("mesh.state.set(\"title\", \"Hello\")")
        );
        assert!(script.source.contains("local tmp = count + 1"));
    }

    #[test]
    fn local_declarations_preserved_verbatim() {
        let source = r#"
<script lang="luau">
local handler = function() end
local audio = mesh.interfaces.get("mesh.audio")
mesh.service.bind("audio.muted", "audio_muted")
mesh.service.on("audio", "sync_audio_state")
</script>
"#;
        let file = parse_component(source).unwrap();
        let script = file.script.unwrap();
        assert!(script.source.contains("local handler = function()"));
        assert!(
            script
                .source
                .contains("local audio = mesh.interfaces.get(\"mesh.audio\")")
        );
        assert!(
            script
                .source
                .contains("mesh.service.bind(\"audio.muted\", \"audio_muted\")")
        );
        assert!(
            script
                .source
                .contains("mesh.service.on(\"audio\", \"sync_audio_state\")")
        );
    }

    #[test]
    fn parse_two_way_binding() {
        let source = r#"
<template>
  <input type="text" bind:value="searchQuery"/>
</template>
"#;
        let file = parse_component(source).unwrap();
        let tmpl = file.template.unwrap();
        match &tmpl.root[0] {
            TemplateNode::Element(el) => {
                assert!(matches!(&el.attributes[0].value, AttributeValue::Static(_)));
                assert!(
                    matches!(&el.attributes[1].value, AttributeValue::TwoWayBinding(v) if v == "searchQuery")
                );
            }
            _ => panic!("expected element"),
        }
    }

    #[test]
    fn parse_semantic_input_tags() {
        let source = r#"
<template>
  <panel>
    <text-input value="{name}"/>
    <password-input value="{secret}"/>
    <search-input value="{query}"/>
    <number-input value="{count}"/>
    <email-input value="{email}"/>
    <url-input value="{website}"/>
  </panel>
</template>
"#;
        let file = parse_component(source).unwrap();
        let tmpl = file.template.unwrap();
        let TemplateNode::Element(root) = &tmpl.root[0] else {
            panic!("expected root element");
        };
        let tags: Vec<_> = root
            .children
            .iter()
            .map(|child| match child {
                TemplateNode::Element(el) => el.tag.as_str(),
                _ => panic!("expected input element"),
            })
            .collect();
        assert_eq!(
            tags,
            [
                "text-input",
                "password-input",
                "search-input",
                "number-input",
                "email-input",
                "url-input",
            ]
        );
    }

    #[test]
    fn rejects_uppercase_builtin_tags() {
        let source = r#"
<template>
  <Text>Not a builtin primitive</Text>
</template>
"#;
        let err = parse_component(source).unwrap_err();
        assert!(
            err.to_string()
                .contains("built-in UI tag <Text> must be lowercase; use <text> instead"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn parses_pascal_case_custom_components() {
        let source = r#"
<template>
  <BatteryWidget percent="{percent}"/>
</template>
<script lang="luau">
import BatteryWidget from "./components/battery-widget.mesh"
</script>
"#;
        let file = parse_component(source).unwrap();
        assert_eq!(file.imports.len(), 1);
        let tmpl = file.template.unwrap();
        match &tmpl.root[0] {
            TemplateNode::Component(component) => assert_eq!(component.name, "BatteryWidget"),
            other => panic!("expected component ref, got {other:?}"),
        }
    }

    #[test]
    fn rejects_unimported_pascal_case_custom_components() {
        let source = r#"
<template>
  <BatteryWidget percent="{percent}"/>
</template>
"#;
        let err = parse_component(source).unwrap_err();
        assert!(
            err.to_string()
                .contains("component <BatteryWidget> is not imported"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn parses_and_strips_explicit_imports() {
        let source = r#"
<template>
  <BatteryWidget />
  <VolumeBar />
</template>
<script lang="luau">
import BatteryWidget from "./components/battery-widget.mesh"
import VolumeBar from "@mesh/volume-bar"
import audio from "mesh.audio@>=1.0"
mesh.state.set("ready", true)
</script>
"#;
        let file = parse_component(source).unwrap();
        assert_eq!(file.imports.len(), 3);
        assert!(matches!(
            file.imports[0].target,
            ComponentImportTarget::ComponentLocal(_)
        ));
        assert!(matches!(
            file.imports[1].target,
            ComponentImportTarget::ComponentPlugin(_)
        ));
        assert!(matches!(
            file.imports[2].target,
            ComponentImportTarget::InterfaceApi { .. }
        ));
        let script = file.script.unwrap();
        assert!(!script.source.contains("import BatteryWidget"));
        assert_eq!(script.source.lines().count(), 5);
    }

    #[test]
    fn rejects_api_import_used_as_component_tag() {
        let source = r#"
<template>
  <Audio />
</template>
<script lang="luau">
import Audio from "mesh.audio"
</script>
"#;
        let err = parse_component(source).unwrap_err();
        assert!(
            err.to_string()
                .contains("component <Audio> is not imported"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn rejects_duplicate_import_aliases() {
        let source = r#"
<script lang="luau">
import Thing from "./components/one.mesh"
import Thing from "./components/two.mesh"
</script>
"#;
        let err = parse_component(source).unwrap_err();
        assert!(
            err.to_string().contains("duplicate import alias `Thing`"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn parse_named_slot() {
        let source = r#"
<template>
  <box>
    <slot name="sidebar"/>
  </box>
</template>
"#;
        let file = parse_component(source).unwrap();
        let tmpl = file.template.unwrap();
        match &tmpl.root[0] {
            TemplateNode::Element(el) => match &el.children[0] {
                TemplateNode::Slot(slot) => assert_eq!(slot.name.as_deref(), Some("sidebar")),
                other => panic!("expected slot node, got {other:?}"),
            },
            other => panic!("expected element node, got {other:?}"),
        }
    }

    #[test]
    fn padding_property_names_from_lightningcss() {
        // Verify which property names lightningcss emits for the padding variants
        // we use in .mesh components so apply_declaration handles them all.
        let source = r#"
<style>
.box {
    padding: 8px;
    padding-inline: 16px;
    padding-block: 12px;
    padding-top: 4px;
    padding-right: 5px;
    padding-bottom: 6px;
    padding-left: 7px;
}
</style>
"#;
        let file = parse_component(source).unwrap();
        let style = file.style.unwrap();
        let decls = &style.rules[0].declarations;
        let props: Vec<&str> = decls.iter().map(|d| d.property.as_str()).collect();
        // Confirm every declaration landed under a name our style resolver knows.
        let known = [
            "padding",
            "padding-top",
            "padding-right",
            "padding-bottom",
            "padding-left",
            "padding-inline",
            "padding-block",
            "padding-inline-start",
            "padding-inline-end",
            "padding-block-start",
            "padding-block-end",
        ];
        for p in &props {
            assert!(known.contains(p), "unrecognised padding property: {p}");
        }
        // Spot-check that the shorthand emitted individual sides (lightningcss expands it).
        eprintln!("padding properties emitted by lightningcss: {props:?}");
    }

    #[test]
    fn parse_for_loop() {
        let source = r#"
<template>
  <box>
    {#for item in items}
      <text>{item.name}</text>
    {/for}
  </box>
</template>
"#;
        let file = parse_component(source).unwrap();
        let tmpl = file.template.unwrap();
        match &tmpl.root[0] {
            TemplateNode::Element(el) => {
                assert_eq!(el.tag, "box");
                assert_eq!(el.children.len(), 1);
                match &el.children[0] {
                    TemplateNode::For(f) => {
                        assert_eq!(f.item_name, "item");
                        assert_eq!(f.iterable, "items");
                        assert_eq!(f.children.len(), 1);
                    }
                    other => panic!("expected ForNode, got {other:?}"),
                }
            }
            other => panic!("expected element, got {other:?}"),
        }
    }

    #[test]
    fn parse_if_else() {
        let source = r#"
<template>
  <box>
    {#if show}
      <text>visible</text>
    {:else}
      <text>hidden</text>
    {/if}
  </box>
</template>
"#;
        let file = parse_component(source).unwrap();
        let tmpl = file.template.unwrap();
        match &tmpl.root[0] {
            TemplateNode::Element(el) => {
                assert_eq!(el.children.len(), 1);
                match &el.children[0] {
                    TemplateNode::If(n) => {
                        assert_eq!(n.condition, "show");
                        assert_eq!(n.then_children.len(), 1);
                        assert_eq!(n.else_children.len(), 1);
                    }
                    other => panic!("expected IfNode, got {other:?}"),
                }
            }
            other => panic!("expected element, got {other:?}"),
        }
    }

    #[test]
    fn parse_if_elif_else() {
        let source = r#"
<template>
  <box>
    {#if a}
      <text>a</text>
    {:else if b}
      <text>b</text>
    {:else}
      <text>c</text>
    {/if}
  </box>
</template>
"#;
        let file = parse_component(source).unwrap();
        let tmpl = file.template.unwrap();
        let root = match &tmpl.root[0] {
            TemplateNode::Element(el) => el,
            other => panic!("expected element, got {other:?}"),
        };
        // Outer if: condition "a"
        let outer = match &root.children[0] {
            TemplateNode::If(n) => n,
            other => panic!("expected IfNode, got {other:?}"),
        };
        assert_eq!(outer.condition, "a");
        // Else branch is another IfNode (the elif chain)
        assert_eq!(outer.else_children.len(), 1);
        let inner = match &outer.else_children[0] {
            TemplateNode::If(n) => n,
            other => panic!("expected nested IfNode, got {other:?}"),
        };
        assert_eq!(inner.condition, "b");
        assert_eq!(inner.else_children.len(), 1); // the final else
    }

    #[test]
    fn parse_for_inside_if() {
        let source = r#"
<template>
  <box>
    {#if items and #items > 0}
      {#for item in items}
        <text>{item.name}</text>
      {/for}
    {:else}
      <text>empty</text>
    {/if}
  </box>
</template>
"#;
        let file = parse_component(source).unwrap();
        let tmpl = file.template.unwrap();
        let root = match &tmpl.root[0] {
            TemplateNode::Element(el) => el,
            other => panic!("expected element, got {other:?}"),
        };
        let if_node = match &root.children[0] {
            TemplateNode::If(n) => n,
            other => panic!("expected IfNode, got {other:?}"),
        };
        assert_eq!(if_node.condition, "items and #items > 0");
        assert_eq!(if_node.then_children.len(), 1);
        match &if_node.then_children[0] {
            TemplateNode::For(f) => {
                assert_eq!(f.item_name, "item");
                assert_eq!(f.iterable, "items");
            }
            other => panic!("expected ForNode, got {other:?}"),
        }
        assert_eq!(if_node.else_children.len(), 1);
    }
}
