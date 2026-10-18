//! Shared compilation for expressions embedded in `.mesh`
//! templates.
//!
//! A component is parsed once into [`CompiledExpression`] values.  The live
//! runtime registers those values as lexical Luau closures. Evaluation belongs
//! to the runtime resolver, which owns the realm and callback budgets.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::{Mutex, OnceLock};

const PARSER_STACK_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum ExpressionCompileError {
    #[error("expression is empty")]
    Empty,
    #[error("malformed Luau expression")]
    Malformed,
    #[error(
        "expression contains unsupported non-ASCII character {character:?} at byte offset {byte_offset}"
    )]
    NonAscii { character: char, byte_offset: usize },
    #[error("expression parser panicked")]
    ParserPanicked,
    #[error("could not start expression parser: {0}")]
    ParserThread(String),
}

/// The validated, cacheable semantic unit shared by every template consumer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledExpression {
    source: String,
    calls_function: bool,
    references: Vec<String>,
    is_reference: bool,
}

pub type SharedCompiledExpression = Arc<CompiledExpression>;

impl CompiledExpression {
    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn calls_function(&self) -> bool {
        self.calls_function
    }

    pub fn referenced_identifiers(&self) -> &[String] {
        &self.references
    }
    pub fn is_reference(&self) -> bool {
        self.is_reference
    }
}

#[derive(Default)]
struct ExpressionMetadata {
    calls_function: bool,
    references: HashSet<String>,
    scopes: Vec<HashSet<String>>,
    block_bindings: HashMap<usize, HashSet<String>>,
    repeat_blocks: HashSet<usize>,
}
impl ExpressionMetadata {
    fn reference(&mut self, name: String) {
        if !self.scopes.iter().rev().any(|scope| scope.contains(&name)) {
            self.references.insert(name);
        }
    }
}
impl full_moon::visitors::Visitor for ExpressionMetadata {
    fn visit_function_call(&mut self, _: &full_moon::ast::FunctionCall) {
        self.calls_function = true;
    }
    fn visit_var(&mut self, var: &full_moon::ast::Var) {
        if let full_moon::ast::Var::Name(name) = var {
            self.reference(name.token().to_string());
        }
    }
    fn visit_prefix(&mut self, prefix: &full_moon::ast::Prefix) {
        if let full_moon::ast::Prefix::Name(name) = prefix {
            self.reference(name.token().to_string());
        }
    }
    fn visit_block(&mut self, block: &full_moon::ast::Block) {
        self.scopes.push(
            self.block_bindings
                .remove(&(block as *const _ as usize))
                .unwrap_or_default(),
        );
    }
    fn visit_block_end(&mut self, block: &full_moon::ast::Block) {
        if !self.repeat_blocks.contains(&(block as *const _ as usize)) {
            self.scopes.pop();
        }
    }
    fn visit_function_body(&mut self, body: &full_moon::ast::FunctionBody) {
        let scope = self
            .block_bindings
            .entry(body.block() as *const _ as usize)
            .or_default();
        for parameter in body.parameters().iter() {
            if let full_moon::ast::Parameter::Name(name) = parameter {
                scope.insert(name.token().to_string());
            }
        }
    }
    fn visit_function_declaration(&mut self, function: &full_moon::ast::FunctionDeclaration) {
        if function.name().names().len() > 1 || function.name().method_name().is_some() {
            if let Some(name) = function.name().names().iter().next() {
                self.reference(name.token().to_string());
            }
        }
        if function.name().method_name().is_some() {
            self.block_bindings
                .entry(function.body().block() as *const _ as usize)
                .or_default()
                .insert("self".into());
        }
    }
    fn visit_local_assignment_end(&mut self, assignment: &full_moon::ast::LocalAssignment) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.extend(
                assignment
                    .names()
                    .iter()
                    .map(|name| name.token().to_string()),
            );
        }
    }
    fn visit_local_function(&mut self, function: &full_moon::ast::LocalFunction) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(function.name().token().to_string());
        }
    }
    fn visit_numeric_for(&mut self, statement: &full_moon::ast::NumericFor) {
        self.block_bindings
            .entry(statement.block() as *const _ as usize)
            .or_default()
            .insert(statement.index_variable().token().to_string());
    }
    fn visit_generic_for(&mut self, statement: &full_moon::ast::GenericFor) {
        self.block_bindings
            .entry(statement.block() as *const _ as usize)
            .or_default()
            .extend(
                statement
                    .names()
                    .iter()
                    .map(|name| name.token().to_string()),
            );
    }
    fn visit_repeat(&mut self, statement: &full_moon::ast::Repeat) {
        self.repeat_blocks
            .insert(statement.block() as *const _ as usize);
    }
    fn visit_repeat_end(&mut self, statement: &full_moon::ast::Repeat) {
        self.repeat_blocks
            .remove(&(statement.block() as *const _ as usize));
        self.scopes.pop();
    }
}
fn is_reference_expression(mut expression: &full_moon::ast::Expression) -> bool {
    while let full_moon::ast::Expression::Parentheses {
        expression: inner, ..
    } = expression
    {
        expression = inner;
    }
    matches!(expression, full_moon::ast::Expression::Var(_))
}

static EXPRESSION_CACHE: OnceLock<Mutex<HashMap<String, SharedCompiledExpression>>> =
    OnceLock::new();

/// Parse and cache one expression. Repeated references from markup, preview,
/// and runtime setup return the same shared semantic value across threads.
pub fn compile_expression(
    source: &str,
) -> Result<SharedCompiledExpression, ExpressionCompileError> {
    let source = source.trim();
    if source.is_empty() {
        return Err(ExpressionCompileError::Empty);
    }
    let non_ascii = first_non_ascii(source);

    let cache = EXPRESSION_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache.lock().expect("expression cache poisoned");
    if let Some(compiled) = cache.get(source).cloned() {
        return Ok(compiled);
    }

    let candidate = format!("return ({source})");
    let parsed = std::thread::Builder::new()
        .stack_size(PARSER_STACK_BYTES)
        .spawn(move || {
            use full_moon::visitors::Visitor;
            full_moon::parse(&candidate)
                .map(|ast| {
                    let mut metadata = ExpressionMetadata::default();
                    metadata.visit_ast(&ast);
                    let reference = matches!(ast.nodes().last_stmt(), Some(full_moon::ast::LastStmt::Return(statement)) if statement.returns().iter().next().is_some_and(is_reference_expression)) && !metadata.calls_function;
                    let mut references = metadata.references.into_iter().collect::<Vec<_>>();
                    references.sort();
                    (metadata.calls_function, references, reference)
                })
                .ok()
        })
        .map_err(|error| ExpressionCompileError::ParserThread(error.to_string()))?
        .join()
        .map_err(|_| {
            non_ascii.map_or(
                ExpressionCompileError::ParserPanicked,
                |(byte_offset, character)| ExpressionCompileError::NonAscii {
                    character,
                    byte_offset,
                },
            )
        })?;
    let Some((calls_function, references, is_reference)) = parsed else {
        return Err(non_ascii.map_or(
            ExpressionCompileError::Malformed,
            |(byte_offset, character)| ExpressionCompileError::NonAscii {
                character,
                byte_offset,
            },
        ));
    };

    let compiled = Arc::new(CompiledExpression {
        source: source.to_owned(),
        calls_function,
        references,
        is_reference,
    });
    cache.insert(source.to_owned(), Arc::clone(&compiled));
    Ok(compiled)
}

fn first_non_ascii(source: &str) -> Option<(usize, char)> {
    source
        .char_indices()
        .find(|(_, character)| !character.is_ascii())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callback_references_respect_lexical_scopes_and_initializers() {
        let expression = compile_expression(
            r#"function(event)
            local item = item
            local function recurse(n) if n > 0 then return recurse(n - 1) end end
            for i = 1, limit do consume(i, event) end
            for key, value in pairs(items) do consume(key, value) end
            repeat local done = check(item) until done
            return outside(event, item)
        end"#,
        )
        .unwrap();
        assert_eq!(
            expression.referenced_identifiers(),
            [
                "check", "consume", "item", "items", "limit", "outside", "pairs"
            ]
        );
    }

    #[test]
    fn callback_method_declarations_bind_self_and_parameters() {
        let expression = compile_expression(
            "function() function actions:pick(value) return self.base + value + external end end",
        )
        .unwrap();
        assert_eq!(expression.referenced_identifiers(), ["actions", "external"]);
        assert!(compile_expression("(actions.pick)").unwrap().is_reference());
        assert!(!expression.is_reference());
    }

    #[test]
    fn compiles_once_and_preserves_luau_semantics() {
        let first = compile_expression("0 or 'fallback'").unwrap();
        let second = compile_expression("0 or 'fallback'").unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(first.source(), "0 or 'fallback'");
    }

    #[test]
    fn malformed_expression_is_rejected() {
        assert_eq!(
            compile_expression("value +").unwrap_err(),
            ExpressionCompileError::Malformed
        );
    }

    #[test]
    fn non_ascii_expression_syntax_is_diagnosed_without_panicking() {
        for source in [
            "é + 1",
            "value .. é",
            "é == value",
            "é ~= value",
            "é < value",
            "é <= value",
            "é > value",
            "é >= value",
            "é and value",
            "é or value",
            "not é",
            "#é",
        ] {
            let byte_offset = source
                .char_indices()
                .find(|(_, character)| !character.is_ascii())
                .map(|(byte_offset, _)| byte_offset)
                .expect("test expression contains non-ASCII input");
            let result = std::panic::catch_unwind(|| compile_expression(source));
            let result = result.expect("non-ASCII expression scanning must not panic");
            assert_eq!(
                result,
                Err(ExpressionCompileError::NonAscii {
                    character: 'é',
                    byte_offset,
                }),
                "unexpected result for {source:?}"
            );
        }
    }

    #[test]
    fn unicode_string_literals_remain_valid_luau_expressions() {
        assert_eq!(compile_expression("'café'").unwrap().source(), "'café'");
    }
}
