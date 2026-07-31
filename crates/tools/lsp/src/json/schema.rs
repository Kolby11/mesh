//! The schema description language shared by every JSON document the LSP
//! serves (`module.json`, `config/module.json`, `settings.json`).
//!
//! A schema is a tree of [`Node`]s. The tree drives key completion, value
//! completion, hover documentation, and unknown-key / enum diagnostics — one
//! description, three features. Documents that own a runtime schema elsewhere
//! (settings) build their tree from it rather than restating it here.

/// One node in the schema tree.
pub struct Node {
    pub doc: String,
    pub type_hint: String,
    pub kind: Kind,
}

pub enum Kind {
    /// An object with a fixed set of known properties. Anything else is an
    /// unknown key.
    Object(Vec<Field>),
    /// An object with a known set of properties *and* an open tail: keys that
    /// match no field are valid and described by `other`. This is the settings
    /// file's top level, where `shell` sits beside arbitrary module namespaces.
    OpenObject {
        fields: Vec<Field>,
        other: Box<Node>,
    },
    /// An object with arbitrary string keys, each mapping to `value`.
    Map(Box<Node>),
    /// An array whose elements match `element`.
    Array(Box<Node>),
    /// A string constrained to one of these values.
    Enum(&'static [&'static str]),
    /// A string with suggested values that are *not* enforced. Used for
    /// extensible vocabularies like capabilities (`<domain>.<action>`), so
    /// completion offers known entries but unknown values are never flagged.
    Suggest(&'static [&'static str]),
    /// As [`Suggest`](Kind::Suggest), for values discovered at runtime (themes
    /// on disk, installed module ids). Discovery can never be complete — a
    /// theme may live outside the workspace — so these are suggested, not
    /// enforced.
    SuggestDiscovered(Vec<String>),
    /// A leaf scalar (string / number / bool / freeform). Accepts any JSON.
    Scalar,
}

/// A named property of an object node.
pub struct Field {
    pub name: String,
    /// True if the field must be present.
    pub required: bool,
    pub node: Node,
}

pub fn obj(doc: impl Into<String>, fields: Vec<Field>) -> Node {
    Node {
        doc: doc.into(),
        type_hint: "object".to_string(),
        kind: Kind::Object(fields),
    }
}

pub fn open_obj(doc: impl Into<String>, fields: Vec<Field>, other: Node) -> Node {
    Node {
        doc: doc.into(),
        type_hint: "object".to_string(),
        kind: Kind::OpenObject {
            fields,
            other: Box::new(other),
        },
    }
}

pub fn map(doc: impl Into<String>, type_hint: impl Into<String>, value: Node) -> Node {
    Node {
        doc: doc.into(),
        type_hint: type_hint.into(),
        kind: Kind::Map(Box::new(value)),
    }
}

pub fn array(doc: impl Into<String>, type_hint: impl Into<String>, element: Node) -> Node {
    Node {
        doc: doc.into(),
        type_hint: type_hint.into(),
        kind: Kind::Array(Box::new(element)),
    }
}

pub fn scalar(doc: impl Into<String>, type_hint: impl Into<String>) -> Node {
    Node {
        doc: doc.into(),
        type_hint: type_hint.into(),
        kind: Kind::Scalar,
    }
}

pub fn enumeration(doc: impl Into<String>, values: &'static [&'static str]) -> Node {
    Node {
        doc: doc.into(),
        type_hint: "enum".to_string(),
        kind: Kind::Enum(values),
    }
}

pub fn discovered(doc: impl Into<String>, values: Vec<String>) -> Node {
    Node {
        doc: doc.into(),
        type_hint: "string".to_string(),
        kind: Kind::SuggestDiscovered(values),
    }
}

pub fn field(name: impl Into<String>, required: bool, node: Node) -> Field {
    Field {
        name: name.into(),
        required,
        node,
    }
}

/// The synthetic path segment used for "an element inside an array".
pub const ARRAY_ELEMENT: &str = "[]";

/// Navigate the schema tree following a container path of object keys. Array
/// element steps use the [`ARRAY_ELEMENT`] sentinel. Returns the node at the
/// path, or `None` if the path leaves the known schema (e.g. inside a free-form
/// map or `experimental`).
pub fn navigate<'a>(node: &'a Node, path: &[String]) -> Option<&'a Node> {
    let Some((head, rest)) = path.split_first() else {
        return Some(node);
    };
    let next = match &node.kind {
        Kind::Object(fields) => fields.iter().find(|f| f.name == *head).map(|f| &f.node),
        Kind::OpenObject { fields, other } => Some(
            fields
                .iter()
                .find(|f| f.name == *head)
                .map(|f| &f.node)
                .unwrap_or(other.as_ref()),
        ),
        Kind::Map(value) => Some(value.as_ref()),
        Kind::Array(element) if head == ARRAY_ELEMENT => Some(element.as_ref()),
        _ => None,
    }?;
    navigate(next, rest)
}

/// The fields an object-ish node declares, for key completion and lookup.
pub fn fields_of(node: &Node) -> Option<&[Field]> {
    match &node.kind {
        Kind::Object(fields) | Kind::OpenObject { fields, .. } => Some(fields),
        _ => None,
    }
}

/// The node describing the value of `key` inside `container`, following an
/// open object's tail when no declared field matches.
pub fn field_node<'a>(container: &'a Node, key: &str) -> Option<&'a Node> {
    match &container.kind {
        Kind::Object(fields) => fields.iter().find(|f| f.name == key).map(|f| &f.node),
        Kind::OpenObject { fields, other } => Some(
            fields
                .iter()
                .find(|f| f.name == key)
                .map(|f| &f.node)
                .unwrap_or(other.as_ref()),
        ),
        Kind::Map(value) => Some(value.as_ref()),
        _ => None,
    }
}

/// The values a node offers for completion, and whether they are exhaustive.
pub fn suggested_values(node: &Node) -> Option<Vec<String>> {
    match &node.kind {
        Kind::Enum(values) | Kind::Suggest(values) => {
            Some(values.iter().map(|v| (*v).to_string()).collect())
        }
        Kind::SuggestDiscovered(values) => Some(values.clone()),
        _ => None,
    }
}
