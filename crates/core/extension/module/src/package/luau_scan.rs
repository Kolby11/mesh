#![allow(dead_code)] // Scanner helpers are consumed by migration and test tooling.

//! Static analysis of the Luau a module actually runs.
//!
//! Graph scanning cross-checks module scripts against module declarations:
//! translation keys against the default catalog, published channels against the
//! shell-owned event namespace, backend provider handles against the interface
//! contract.
//! Everything goes through a real Luau parser, so a call is a call and a string
//! is a string — never a substring match over comments or `<style>` blocks.
//!
//! A source the parser rejects yields no findings rather than wrong ones: the
//! runtime already reports the parse failure when it loads the script.

use full_moon::ast::{Ast, Call, Expression, FunctionArgs, FunctionCall, Index, Prefix, Suffix};
use full_moon::tokenizer::{TokenReference, TokenType};
use full_moon::visitors::Visitor;

/// Stack for the parser thread. `full_moon` is recursive descent with large
/// unoptimized frames: a ~1000-line script overruns the 2 MiB a default debug
/// thread gets, and a stack overflow aborts the process rather than erroring.
/// Sized well past anything a module realistically contains.
const PARSER_STACK_BYTES: usize = 16 * 1024 * 1024;

/// The Luau in one source file, split by how it has to be parsed.
#[derive(Debug, Default)]
pub(crate) struct LuauSources<'a> {
    /// Complete chunks: a backend `main.luau`, or a `.mesh` `<script>` block.
    pub(crate) chunks: Vec<&'a str>,
    /// Bare template expressions (`{t('nav.volume') .. suffix}`), parsed as the
    /// operand of a `return`, which is how they are evaluated at runtime.
    pub(crate) expressions: Vec<&'a str>,
}

/// For each entry in `callees`, the first-argument string literal of every call
/// to it across `sources`. Results are sorted and deduplicated, positionally
/// matching `callees`.
///
/// A callee is the dotted path as written at the call site — `t` or
/// `mesh.events.publish`. Only a plain name followed
/// by `.` field accesses matches; a call reached through a local alias or a
/// bracket index is not statically resolvable and is skipped.
///
/// Calls whose first argument is not a string literal (`t(key_variable)`,
/// `publish(prefix .. name)`) contribute nothing, so a dynamic call site never
/// produces a diagnostic about a key nobody wrote.
pub(crate) fn static_call_string_arguments(
    sources: &LuauSources<'_>,
    callees: &[&str],
) -> Vec<Vec<String>> {
    on_parser_stack(|| {
        let mut found = vec![Vec::new(); callees.len()];

        for chunk in &sources.chunks {
            collect_from_chunk(chunk, callees, &mut found);
        }
        for expression in &sources.expressions {
            let expression = expression.trim();
            if expression.is_empty() {
                continue;
            }
            collect_from_chunk(&format!("return {expression}"), callees, &mut found);
        }

        for arguments in &mut found {
            arguments.retain(|argument| !argument.is_empty());
            arguments.sort();
            arguments.dedup();
        }
        found
    })
}

/// Convenience for the single-chunk, single-callee case.
pub(crate) fn static_call_string_arguments_in_chunk(chunk: &str, callee: &str) -> Vec<String> {
    let sources = LuauSources {
        chunks: vec![chunk],
        expressions: Vec::new(),
    };
    static_call_string_arguments(&sources, &[callee])
        .pop()
        .unwrap_or_default()
}

/// Find statically named provider events published through the provider-owned
/// `self.EventName:fire(...)` handle.
pub(crate) fn static_self_event_names_in_chunk(chunk: &str) -> Vec<String> {
    on_parser_stack(|| {
        let Ok(ast) = full_moon::parse(chunk) else {
            return Vec::new();
        };
        let mut collector = SelfEventNames::default();
        collector.visit_ast(&ast);
        collector.names.sort();
        collector.names.dedup();
        collector.names
    })
}

fn collect_from_chunk(chunk: &str, callees: &[&str], found: &mut [Vec<String>]) {
    let Ok(ast) = full_moon::parse(chunk) else {
        return;
    };
    collect_from_ast(&ast, callees, found);
}

fn collect_from_ast(ast: &Ast, callees: &[&str], found: &mut [Vec<String>]) {
    let mut collector = StaticCallArguments { callees, found };
    collector.visit_ast(ast);
}

/// Run `work` on a thread with a stack sized for the parser. See
/// [`PARSER_STACK_BYTES`].
fn on_parser_stack<T: Send>(work: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(PARSER_STACK_BYTES)
            .spawn_scoped(scope, work)
            .expect("spawn Luau parser thread")
            .join()
            .expect("Luau parser thread finished")
    })
}

struct StaticCallArguments<'a, 'f> {
    callees: &'a [&'a str],
    found: &'f mut [Vec<String>],
}

#[derive(Default)]
struct SelfEventNames {
    names: Vec<String>,
}

impl Visitor for SelfEventNames {
    fn visit_function_call(&mut self, call: &FunctionCall) {
        let Prefix::Name(name) = call.prefix() else {
            return;
        };
        if identifier_name(name).as_deref() != Some("self") {
            return;
        }
        let mut suffixes = call.suffixes();
        let Some(Suffix::Index(Index::Dot { name, .. })) = suffixes.next() else {
            return;
        };
        let Some(event_name) = identifier_name(name) else {
            return;
        };
        let Some(Suffix::Call(Call::MethodCall(method))) = suffixes.next() else {
            return;
        };
        if identifier_name(method.name()).as_deref() != Some("fire") || suffixes.next().is_some() {
            return;
        }
        self.names.push(event_name);
    }
}

impl Visitor for StaticCallArguments<'_, '_> {
    fn visit_function_call(&mut self, call: &FunctionCall) {
        // The visitor walks the whole tree, so a nested call (`f(t("k"))`)
        // arrives as its own `visit_function_call`. Only the callee immediately
        // in front of the argument list is considered here.
        let Prefix::Name(name) = call.prefix() else {
            return;
        };
        let Some(mut path) = identifier_name(name) else {
            return;
        };

        for suffix in call.suffixes() {
            match suffix {
                Suffix::Index(Index::Dot { name, .. }) => {
                    let Some(field) = identifier_name(name) else {
                        return;
                    };
                    path.push('.');
                    path.push_str(&field);
                }
                Suffix::Call(Call::AnonymousCall(arguments)) => {
                    if let Some(index) = self.callees.iter().position(|callee| *callee == path)
                        && let Some(literal) = first_string_argument(arguments)
                    {
                        self.found[index].push(literal);
                    }
                    // Whatever the call returns is a new value; a further suffix
                    // chain no longer names this callee.
                    return;
                }
                // A method call (`x:y()`), a bracket index (`x["y"]`), or a Luau
                // type instantiation ends the statically resolvable path.
                _ => return,
            }
        }
    }
}

fn identifier_name(token: &TokenReference) -> Option<String> {
    match token.token_type() {
        TokenType::Identifier { identifier } => Some(identifier.to_string()),
        _ => None,
    }
}

fn first_string_argument(arguments: &FunctionArgs) -> Option<String> {
    match arguments {
        FunctionArgs::Parentheses { arguments, .. } => {
            expression_string_literal(arguments.iter().next()?)
        }
        // Lua's parenthesis-free string call: `t "nav.volume"`.
        FunctionArgs::String(token) => string_literal(token),
        _ => None,
    }
}

fn expression_string_literal(expression: &Expression) -> Option<String> {
    match expression {
        Expression::String(token) => string_literal(token),
        Expression::Parentheses { expression, .. } => expression_string_literal(expression),
        _ => None,
    }
}

fn string_literal(token: &TokenReference) -> Option<String> {
    match token.token_type() {
        TokenType::StringLiteral { literal, .. } => Some(literal.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(source: &str) -> Vec<String> {
        static_call_string_arguments_in_chunk(source, "t")
    }

    #[test]
    fn only_whole_identifier_callees_match() {
        // A substring scan would match the `t(` inside `format(` and `assert(`
        // and report their first argument as a translation key.
        let source = r#"
            local label = string.format("%d%%", value)
            assert("boom")
            table.insert(list, "entry")
            local title = t("nav.volume")
        "#;

        assert_eq!(keys(source), vec!["nav.volume".to_string()]);
    }

    #[test]
    fn comments_and_string_literals_are_not_code() {
        let source = r#"
            -- t("commented.out")
            --[[ t("block.commented") ]]
            local documentation = 't("inside.a.string")'
            local real = t("actually.called")
        "#;

        assert_eq!(keys(source), vec!["actually.called".to_string()]);
    }

    #[test]
    fn dotted_callee_paths_match_exactly() {
        let source = r#"
            mesh.events.publish("shell.reload", {})
            mesh.events.subscribe("shell.reload", handler)
            events.publish("not.the.same.path")
        "#;

        assert_eq!(
            static_call_string_arguments_in_chunk(source, "mesh.events.publish"),
            vec!["shell.reload".to_string()]
        );
    }

    #[test]
    fn dynamic_arguments_yield_nothing() {
        let source = r#"
            local a = t(key_variable)
            local b = t("prefix." .. suffix)
            local c = t()
            local d = t("static.key")
        "#;

        assert_eq!(keys(source), vec!["static.key".to_string()]);
    }

    #[test]
    fn nested_and_multiline_string_calls_are_found() {
        let source = r#"
            local label = string.format("%s", t('single.quoted'))
            local long = t([[bracket.quoted]])
            local bare = t "paren.free"
        "#;

        assert_eq!(
            keys(source),
            vec![
                "bracket.quoted".to_string(),
                "paren.free".to_string(),
                "single.quoted".to_string(),
            ]
        );
    }

    #[test]
    fn template_expressions_parse_as_expressions() {
        let sources = LuauSources {
            chunks: Vec::new(),
            expressions: vec![
                "value == '' and t('nav.volume') or t('nav.volume') .. ' ' .. value",
                "   ",
            ],
        };

        assert_eq!(
            static_call_string_arguments(&sources, &["t"]),
            vec![vec!["nav.volume".to_string()]]
        );
    }

    #[test]
    fn several_callees_share_one_parse() {
        let sources = LuauSources {
            chunks: vec![
                r#"
                    mesh.events.publish("shell.hide-surface", {})
                    local label = t("nav.volume")
                "#,
            ],
            expressions: vec!["t('nav.language_menu')"],
        };

        assert_eq!(
            static_call_string_arguments(&sources, &["t", "mesh.events.publish"]),
            vec![
                vec!["nav.language_menu".to_string(), "nav.volume".to_string()],
                vec!["shell.hide-surface".to_string()],
            ]
        );
    }

    #[test]
    fn unparseable_source_yields_nothing_rather_than_guesses() {
        assert!(keys("function broken( then t(\"nope\")").is_empty());
    }

    #[test]
    fn the_largest_shipped_script_shape_parses_within_the_parser_stack() {
        // Guards the stack sizing: an overflow aborts the process rather than
        // failing the scan.
        let statement = "local value = t(\"nav.volume\") .. string.format(\"%d%%\", percent)\n";
        let chunk = statement.repeat(8_000);

        assert_eq!(
            static_call_string_arguments_in_chunk(&chunk, "t"),
            vec!["nav.volume".to_string()]
        );
    }
}
