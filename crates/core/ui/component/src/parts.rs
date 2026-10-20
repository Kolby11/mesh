//! Static public style targets exposed by a component's authored elements.
use crate::{AttributeValue, ComponentFile, TemplateNode};
use std::collections::BTreeSet;

pub fn style_parts(component: &ComponentFile) -> BTreeSet<String> {
    fn visit(nodes: &[TemplateNode], parts: &mut BTreeSet<String>) {
        for node in nodes {
            match node {
                TemplateNode::Element(element) => {
                    for attr in &element.attributes {
                        if attr.name == "part"
                            && let AttributeValue::Static(value) = &attr.value
                        {
                            parts.extend(value.split_whitespace().map(str::to_owned));
                        }
                    }
                    visit(&element.children, parts);
                }
                TemplateNode::Component(component) => visit(&component.children, parts),
                TemplateNode::If(node) => {
                    visit(&node.then_children, parts);
                    visit(&node.else_children, parts);
                }
                TemplateNode::For(node) => visit(&node.children, parts),
                _ => {}
            }
        }
    }
    let mut parts = BTreeSet::new();
    if let Some(template) = &component.template {
        visit(&template.root, &mut parts);
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_are_static_public_element_names() {
        let component = crate::parse_component("<template><box part=\"frame\">{#if true}<icon part=\"glyph decoration\"/>{/if}</box></template>").unwrap();
        assert_eq!(
            style_parts(&component).into_iter().collect::<Vec<_>>(),
            ["decoration", "frame", "glyph"]
        );
        for attribute in ["part={name}", "part=\"bad.name\"", "part=\"\""] {
            assert!(
                crate::parse_component(&format!("<template><box {attribute}/></template>"))
                    .is_err()
            );
        }
        assert!(crate::parse_component("<template><Child part=\"frame\"/></template>").is_err());
    }
}
