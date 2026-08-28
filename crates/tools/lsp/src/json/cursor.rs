//! A tolerant JSON token stream that determines the JSON "location" at a byte
//! offset: the object-key path to the innermost container, whether the cursor
//! is in a key or a value slot, and the partial token being typed.
//!
//! The JSONC scanner supplies standards-aware token boundaries and decoded
//! string values while retaining recovery for mid-edit, syntactically invalid
//! documents. This is exactly the state of a document while the user is typing
//! and asking for completion.

use super::schema::ARRAY_ELEMENT;
use jsonc_parser::{Scanner, ScannerOptions, tokens::Token};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Editing an object property name.
    Key,
    /// Editing a value (object property value or array element).
    Value,
}

#[derive(Debug, Clone)]
pub struct CursorContext {
    /// Object-key path to the innermost container, using [`ARRAY_ELEMENT`] for
    /// array-element steps. For a value, this is the path to the container; the
    /// value's own key (if any) is [`value_key`](Self::value_key).
    pub path: Vec<String>,
    pub innermost_is_array: bool,
    pub role: Role,
    /// The token text already typed at the cursor (without surrounding quotes).
    pub partial: String,
    /// The whole string literal the cursor sits in, including the part after the
    /// cursor. Completion filters on [`partial`](Self::partial); hover needs the
    /// whole word, because someone hovering the middle of `"anchor"` is asking
    /// about `anchor`, not about `anc`.
    pub token: String,
    /// Keys already present in the innermost object (to avoid suggesting dupes).
    pub existing_keys: Vec<String>,
    /// For a value: the key whose value is being edited (or, inside an array,
    /// the key the array belongs to).
    pub value_key: Option<String>,
    /// Whether the cursor sits inside a string literal.
    pub in_string: bool,
}

#[derive(Debug)]
enum FrameKind {
    Object,
    Array,
}

#[derive(Debug)]
struct Frame {
    kind: FrameKind,
    /// The key in the parent object whose value is this container
    /// ([`ARRAY_ELEMENT`] for an array element object).
    path_key: Option<String>,
    /// Keys seen so far in this object.
    keys: Vec<String>,
    /// The current property key awaiting / holding a value.
    current_key: Option<String>,
    /// True once a `:` has been seen for `current_key` and no `,`/close yet.
    after_colon: bool,
}

impl Frame {
    fn new(kind: FrameKind, path_key: Option<String>) -> Self {
        Self {
            kind,
            path_key,
            keys: Vec::new(),
            current_key: None,
            after_colon: false,
        }
    }
}

/// Compute the cursor context at `offset` (a byte offset into `source`).
pub fn context_at(source: &str, offset: usize) -> CursorContext {
    let offset = offset.min(source.len());

    let mut stack: Vec<Frame> = Vec::new();
    // The most recent string literal's decoded text and whether it was followed
    // by `:` (making it a key). Tracked so a string at offset can be classified.
    let mut last_string: Option<String> = None;

    let options = ScannerOptions {
        allow_single_quoted_strings: false,
        allow_hexadecimal_numbers: false,
        allow_unary_plus_numbers: false,
    };
    let mut scanner = Scanner::new(source, &options);

    loop {
        let token = match scanner.scan() {
            Ok(Some(token)) => token,
            Ok(None) => break,
            Err(error) => {
                // An unfinished string is the common recovery case while
                // completing a key or value. Decode its prefix with the same
                // parser after supplying a synthetic closing quote.
                let start = scanner.token_start();
                if start < offset && source.as_bytes().get(start) == Some(&b'"') {
                    if let Some(partial) = decode_string_prefix(source, start, offset) {
                        let token = partial.clone();
                        return classify_in_string(&stack, &partial, &token);
                    }
                }
                let _ = error;
                break;
            }
        };

        let start = scanner.token_start();
        let end = scanner.token_end();

        if let Token::String(text) = &token {
            if start < offset && offset < end {
                let partial =
                    decode_string_prefix(source, start, offset).unwrap_or_else(|| text.to_string());
                return classify_in_string(&stack, &partial, text);
            }
        }

        if start >= offset {
            break;
        }

        match token {
            Token::OpenBrace => {
                let path_key = pending_path_key(&mut stack);
                stack.push(Frame::new(FrameKind::Object, path_key));
                last_string = None;
            }
            Token::OpenBracket => {
                let path_key = pending_path_key(&mut stack);
                stack.push(Frame::new(FrameKind::Array, path_key));
                last_string = None;
            }
            Token::CloseBrace | Token::CloseBracket => {
                stack.pop();
                last_string = None;
            }
            Token::Colon => {
                if let Some(frame) = stack.last_mut() {
                    if matches!(frame.kind, FrameKind::Object) {
                        if let Some(key) = last_string.take() {
                            if !frame.keys.contains(&key) {
                                frame.keys.push(key.clone());
                            }
                            frame.current_key = Some(key);
                            frame.after_colon = true;
                        }
                    }
                }
            }
            Token::Comma => {
                if let Some(frame) = stack.last_mut() {
                    frame.current_key = None;
                    frame.after_colon = false;
                }
                last_string = None;
            }
            Token::String(text) => last_string = Some(text.into_owned()),
            Token::Word(_)
            | Token::Boolean(_)
            | Token::Number(_)
            | Token::Null
            | Token::CommentLine(_)
            | Token::CommentBlock(_) => {}
        }
    }

    // Cursor is in whitespace / structural position (not inside a string).
    classify_outside_string(&stack)
}

/// When entering a container, the linking key is the enclosing object's current
/// key (consumed) or [`ARRAY_ELEMENT`] when the parent is an array.
fn pending_path_key(stack: &mut [Frame]) -> Option<String> {
    match stack.last_mut() {
        Some(frame) => match frame.kind {
            FrameKind::Object => frame.current_key.clone(),
            FrameKind::Array => Some(ARRAY_ELEMENT.to_string()),
        },
        None => None,
    }
}

fn decode_string_prefix(source: &str, start: usize, offset: usize) -> Option<String> {
    let prefix = source.get(start..offset)?;
    let mut repaired = String::with_capacity(prefix.len() + 1);
    repaired.push_str(prefix);
    repaired.push('"');

    let options = ScannerOptions {
        allow_single_quoted_strings: false,
        allow_hexadecimal_numbers: false,
        allow_unary_plus_numbers: false,
    };
    let mut scanner = Scanner::new(&repaired, &options);
    match scanner.scan().ok()? {
        Some(Token::String(text)) => Some(text.into_owned()),
        _ => None,
    }
}

fn path_of(stack: &[Frame]) -> Vec<String> {
    stack.iter().filter_map(|f| f.path_key.clone()).collect()
}

fn classify_in_string(stack: &[Frame], text: &str, token: &str) -> CursorContext {
    let path = path_of(stack);
    match stack.last() {
        Some(frame) => match frame.kind {
            FrameKind::Object => {
                if frame.after_colon {
                    CursorContext {
                        path,
                        innermost_is_array: false,
                        role: Role::Value,
                        partial: text.to_string(),
                        token: token.to_string(),
                        existing_keys: frame.keys.clone(),
                        value_key: frame.current_key.clone(),
                        in_string: true,
                    }
                } else {
                    // A string in key position (the partial is the key itself,
                    // not yet committed to `keys`).
                    CursorContext {
                        path,
                        innermost_is_array: false,
                        role: Role::Key,
                        partial: text.to_string(),
                        token: token.to_string(),
                        existing_keys: frame.keys.clone(),
                        value_key: None,
                        in_string: true,
                    }
                }
            }
            FrameKind::Array => CursorContext {
                path,
                innermost_is_array: true,
                role: Role::Value,
                partial: text.to_string(),
                token: token.to_string(),
                existing_keys: Vec::new(),
                value_key: frame.path_key.clone(),
                in_string: true,
            },
        },
        None => CursorContext {
            path,
            innermost_is_array: false,
            role: Role::Value,
            partial: text.to_string(),
            token: token.to_string(),
            existing_keys: Vec::new(),
            value_key: None,
            in_string: true,
        },
    }
}

fn classify_outside_string(stack: &[Frame]) -> CursorContext {
    let path = path_of(stack);
    match stack.last() {
        Some(frame) => match frame.kind {
            FrameKind::Object => {
                if frame.after_colon {
                    CursorContext {
                        path,
                        innermost_is_array: false,
                        role: Role::Value,
                        partial: String::new(),
                        token: String::new(),
                        existing_keys: frame.keys.clone(),
                        value_key: frame.current_key.clone(),
                        in_string: false,
                    }
                } else {
                    CursorContext {
                        path,
                        innermost_is_array: false,
                        role: Role::Key,
                        partial: String::new(),
                        token: String::new(),
                        existing_keys: frame.keys.clone(),
                        value_key: None,
                        in_string: false,
                    }
                }
            }
            FrameKind::Array => CursorContext {
                path,
                innermost_is_array: true,
                role: Role::Value,
                partial: String::new(),
                token: String::new(),
                existing_keys: Vec::new(),
                value_key: frame.path_key.clone(),
                in_string: false,
            },
        },
        None => CursorContext {
            // At the very top level, before/around the root object.
            path,
            innermost_is_array: false,
            role: Role::Value,
            partial: String::new(),
            token: String::new(),
            existing_keys: Vec::new(),
            value_key: None,
            in_string: false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(src: &str) -> CursorContext {
        // Cursor marked by `|`.
        let offset = src.find('|').expect("need a | cursor marker");
        let clean = src.replacen('|', "", 1);
        context_at(&clean, offset)
    }

    #[test]
    fn top_level_key() {
        let ctx = at(r#"{ "|" }"#);
        assert_eq!(ctx.role, Role::Key);
        assert!(ctx.path.is_empty());
    }

    #[test]
    fn nested_object_key() {
        let ctx = at(r#"{ "mesh": { "ki|" } }"#);
        assert_eq!(ctx.role, Role::Key);
        assert_eq!(ctx.path, vec!["mesh".to_string()]);
        assert_eq!(ctx.partial, "ki");
    }

    #[test]
    fn unicode_escapes_are_decoded_for_string_completion() {
        let ctx = at(r#"{ "\u006d|" }"#);
        assert_eq!(ctx.role, Role::Key);
        assert_eq!(ctx.partial, "m");
        assert_eq!(ctx.token, "m");
    }

    #[test]
    fn enum_value() {
        let ctx = at(r#"{ "mesh": { "kind": "fr|" } }"#);
        assert_eq!(ctx.role, Role::Value);
        assert_eq!(ctx.value_key.as_deref(), Some("kind"));
        assert_eq!(ctx.path, vec!["mesh".to_string()]);
    }

    #[test]
    fn array_element_value() {
        let ctx = at(r#"{ "mesh": { "uses": { "capabilities": [ "sh|" ] } } }"#);
        assert_eq!(ctx.role, Role::Value);
        assert!(ctx.innermost_is_array);
        assert_eq!(ctx.value_key.as_deref(), Some("capabilities"));
        assert_eq!(
            ctx.path,
            vec![
                "mesh".to_string(),
                "uses".to_string(),
                "capabilities".to_string()
            ]
        );
    }

    #[test]
    fn existing_keys_tracked() {
        let ctx = at(r#"{ "name": "x", "version": "1", "|" }"#);
        assert_eq!(ctx.role, Role::Key);
        assert!(ctx.existing_keys.contains(&"name".to_string()));
        assert!(ctx.existing_keys.contains(&"version".to_string()));
    }

    #[test]
    fn array_element_object_key() {
        let ctx = at(r#"{ "mesh": { "implements": [ { "inter|" } ] } }"#);
        assert_eq!(ctx.role, Role::Key);
        assert_eq!(
            ctx.path,
            vec![
                "mesh".to_string(),
                "implements".to_string(),
                ARRAY_ELEMENT.to_string()
            ]
        );
    }
}
