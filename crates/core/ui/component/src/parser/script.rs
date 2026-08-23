use crate::{
    ComponentImport, ComponentImportTarget, ScriptAlias, ScriptAliasTarget, ScriptBlock,
    ScriptLang, ScriptMetadata, ScriptSymbol, ScriptSymbolKind, SourceSpan,
};
use full_moon::{
    LuaVersion,
    ast::{
        Assignment, Call, Expression, FunctionArgs, FunctionCall, FunctionDeclaration, Index,
        LocalAssignment, LocalFunction, Prefix, Suffix, Var,
    },
    tokenizer::{Lexer, LexerResult, TokenReference, TokenType},
    visitors::Visitor,
};
use std::collections::HashSet;

use super::ParseError;

const PARSER_STACK_BYTES: usize = 16 * 1024 * 1024;

/// Parse a script once for both component imports and AST-derived metadata.
pub(super) fn parse_script(
    source: &str,
) -> Result<(Vec<ComponentImport>, ScriptBlock), ParseError> {
    let (explicit_imports, masked_source) = scan_explicit_imports(source)?;
    let ast = parse_ast(&masked_source);
    let mut visitor = ScriptVisitor::default();
    visitor.visit_ast(&ast);
    if let Some(span) = visitor.invalid_require_span {
        return Err(ParseError::InvalidImport {
            message: "require source must be a quoted string".into(),
            span,
        });
    }
    let metadata = visitor.metadata;
    let require_imports = visitor.require_imports;

    let mut imports = explicit_imports;
    let mut aliases = imports
        .iter()
        .map(|import| import.alias.clone())
        .collect::<HashSet<_>>();
    for candidate in require_imports {
        if !aliases.insert(candidate.import.alias.clone()) {
            return Err(ParseError::InvalidImport {
                message: format!("duplicate import alias `{}`", candidate.import.alias),
                span: candidate.import.span,
            });
        }
        imports.push(candidate.import);
    }

    Ok((
        imports,
        ScriptBlock {
            lang: ScriptLang::Luau,
            source: masked_source,
            metadata,
            span: SourceSpan::new(0, source.len()),
        },
    ))
}

/// Parse a template expression with the same Luau parser used for scripts and
/// return its root identifiers. Member names are represented as AST indexes,
/// so `state.value` contributes only `state`; strings and comments contribute
/// nothing.
pub fn referenced_identifiers(expr: &str) -> Vec<String> {
    on_parser_stack(|| {
        let source = format!("return {expr}");
        let Ok(ast) = full_moon::parse(&source) else {
            return Vec::new();
        };

        let mut visitor = IdentifierVisitor::default();
        visitor.visit_ast(&ast);
        visitor.identifiers
    })
}

fn scan_explicit_imports(source: &str) -> Result<(Vec<ComponentImport>, String), ParseError> {
    let Some(tokens) = significant_tokens(source) else {
        return Ok((Vec::new(), source.to_string()));
    };

    let mut imports = Vec::new();
    let mut aliases = HashSet::new();
    let mut masked_ranges = Vec::new();

    for window in tokens.windows(4) {
        let [import, alias, from, source_token] = window else {
            continue;
        };
        if identifier(import) != Some("import")
            || identifier(alias).is_none()
            || identifier(from) != Some("from")
            || !is_string_literal(source_token)
        {
            continue;
        }

        let alias_name = identifier(alias).unwrap().to_string();
        let source_name = string_literal(source_token).unwrap_or_default();
        let target =
            classify_import_target(&source_name).ok_or_else(|| ParseError::InvalidImport {
                message: format!("unsupported import source `{source_name}`"),
                span: token_span(import),
            })?;

        if !aliases.insert(alias_name.clone()) {
            return Err(ParseError::InvalidImport {
                message: format!("duplicate import alias `{alias_name}`"),
                span: token_span(alias),
            });
        }

        imports.push(ComponentImport {
            alias: alias_name,
            target,
            span: SourceSpan::new(
                import.start_position().bytes(),
                source_token.end_position().bytes(),
            ),
            alias_span: token_span(alias),
            target_span: token_span(source_token),
        });
        masked_ranges.push((
            import.start_position().bytes(),
            source_token.end_position().bytes(),
        ));
    }

    let mut masked = source.as_bytes().to_vec();
    for (start, end) in masked_ranges {
        for byte in masked.get_mut(start..end).into_iter().flatten() {
            if !matches!(*byte, b'\n' | b'\r') {
                *byte = b' ';
            }
        }
    }

    Ok((
        imports,
        String::from_utf8(masked).expect("masking source preserves UTF-8 boundaries"),
    ))
}

fn significant_tokens(source: &str) -> Option<Vec<TokenReference>> {
    let mut lexer = Lexer::new(source, LuaVersion::new());
    let mut tokens = Vec::new();

    while let Some(result) = lexer.consume() {
        let token = match result {
            LexerResult::Ok(token) | LexerResult::Recovered(token, _) => token,
            LexerResult::Fatal(_) => return None,
        };
        if !matches!(token.token_kind(), full_moon::tokenizer::TokenKind::Eof) {
            tokens.push(token);
        }
    }

    Some(tokens)
}

fn parse_ast(source: &str) -> full_moon::ast::Ast {
    on_parser_stack(|| full_moon::parse_fallible(source, LuaVersion::new()).into_ast())
}

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

#[derive(Debug)]
struct RequireImport {
    import: ComponentImport,
}

#[derive(Default)]
struct ScriptVisitor {
    metadata: ScriptMetadata,
    require_imports: Vec<RequireImport>,
    invalid_require_span: Option<SourceSpan>,
}

impl Visitor for ScriptVisitor {
    fn visit_assignment(&mut self, assignment: &Assignment) {
        if assignment.variables().len() == 1 {
            if let Some(Var::Name(name)) = assignment.variables().iter().next() {
                if let Some(name_text) = identifier(name) {
                    push_unique(&mut self.metadata.state_vars, name_text.to_string());
                    self.push_symbol(name_text, ScriptSymbolKind::Variable, name);
                }
            }
        }
        self.visit_alias_assignment(assignment.variables(), assignment.expressions());
    }

    fn visit_local_assignment(&mut self, assignment: &LocalAssignment) {
        self.visit_alias_assignment(assignment.names(), assignment.expressions());

        let Some(name) = assignment.names().iter().next() else {
            return;
        };
        let Some(name_text) = identifier(name) else {
            return;
        };
        let statement_span = full_moon::node::Node::range(assignment)
            .map(position_span)
            .unwrap_or_else(|| token_span(name));
        let Some(expression) = assignment.expressions().iter().next() else {
            return;
        };

        if matches!(expression, Expression::Function(_)) {
            push_unique(&mut self.metadata.functions, name_text.to_string());
            self.push_symbol(name_text, ScriptSymbolKind::Function, name);
        }

        let Some((callee, arguments)) = direct_call(expression) else {
            return;
        };
        if !matches!(callee.as_str(), "require" | "import") {
            return;
        }

        let Some(source) = string_arguments(arguments).next() else {
            if callee == "require" {
                self.invalid_require_span = Some(statement_span);
            }
            return;
        };
        let has_extra_argument = argument_count(arguments) > 1;

        if callee == "require" {
            push_unique(&mut self.metadata.required_aliases, name_text.to_string());
            if has_extra_argument {
                self.invalid_require_span = Some(statement_span);
                return;
            }
            if let Some(target) = classify_import_target(&source) {
                let target_span = first_string_span(arguments).unwrap_or(statement_span);
                self.require_imports.push(RequireImport {
                    import: ComponentImport {
                        alias: name_text.to_string(),
                        target: target.clone(),
                        span: statement_span,
                        alias_span: token_span(name),
                        target_span,
                    },
                });
                if let ComponentImportTarget::InterfaceApi { interface, .. } = target {
                    self.metadata
                        .interface_proxies
                        .insert(name_text.to_string(), interface);
                }
            }
        } else if !has_extra_argument {
            if let Some(ComponentImportTarget::InterfaceApi { interface, .. }) =
                classify_import_target(&source)
            {
                self.metadata
                    .interface_proxies
                    .insert(name_text.to_string(), interface);
            }
        }
    }

    fn visit_function_declaration(&mut self, function: &FunctionDeclaration) {
        let mut names = function.name().names().iter();
        let Some(name) = names.next() else {
            return;
        };
        if names.next().is_some() || function.name().method_name().is_some() {
            return;
        }
        let Some(name_text) = identifier(name) else {
            return;
        };
        push_unique(&mut self.metadata.functions, name_text.to_string());
        push_unique(&mut self.metadata.public_functions, name_text.to_string());
        self.push_symbol(name_text, ScriptSymbolKind::Function, name);
    }

    fn visit_local_function(&mut self, function: &LocalFunction) {
        let Some(name_text) = identifier(function.name()) else {
            return;
        };
        push_unique(&mut self.metadata.functions, name_text.to_string());
        self.push_symbol(name_text, ScriptSymbolKind::Function, function.name());
    }

    fn visit_function_call(&mut self, call: &FunctionCall) {
        let Some((callee, arguments)) = call_info(call) else {
            return;
        };
        let mut values = string_arguments(arguments);

        if callee == "mesh.state.set" {
            if let Some(key) = values.next() {
                push_unique(&mut self.metadata.state_vars, key);
            }
        } else if callee == "mesh.service.bind" {
            if let (Some(service), Some(local)) = (values.next(), values.next()) {
                self.metadata.service_bindings.push((service, local));
            }
        } else if callee.ends_with(":bind") || callee.ends_with(".bind") {
            let service = callee
                .trim_end_matches(":bind")
                .trim_end_matches(".bind")
                .rsplit('.')
                .next()
                .unwrap_or_default()
                .to_string();
            if let Some(local) = values.nth(1) {
                self.metadata.service_bindings.push((service, local));
            }
        }
    }
}

impl ScriptVisitor {
    fn push_symbol(&mut self, name: &str, kind: ScriptSymbolKind, token: &TokenReference) {
        if self
            .metadata
            .symbols
            .iter()
            .any(|symbol| symbol.name == name && symbol.kind == kind)
        {
            return;
        }
        self.metadata.symbols.push(ScriptSymbol {
            name: name.to_string(),
            kind,
            span: SourceSpan::new(token.start_position().bytes(), token.end_position().bytes()),
        });
    }

    fn visit_alias_assignment<T>(
        &mut self,
        names: &full_moon::ast::punctuated::Punctuated<T>,
        expressions: &full_moon::ast::punctuated::Punctuated<Expression>,
    ) where
        T: AssignmentName,
    {
        if names.len() != 1 {
            return;
        }
        let Some(alias) = names.iter().next().and_then(AssignmentName::name) else {
            return;
        };
        let Some(expression) = expressions.iter().next() else {
            return;
        };
        let Some(target) = alias_target(expression) else {
            return;
        };
        if self
            .metadata
            .element_ref_aliases
            .iter()
            .any(|existing| existing.alias == alias)
        {
            return;
        }
        self.metadata.element_ref_aliases.push(ScriptAlias {
            alias: alias.to_string(),
            target,
        });
    }
}

trait AssignmentName {
    fn name(&self) -> Option<&str>;
}

impl AssignmentName for TokenReference {
    fn name(&self) -> Option<&str> {
        identifier(self)
    }
}

impl AssignmentName for Var {
    fn name(&self) -> Option<&str> {
        match self {
            Var::Name(token) => identifier(token),
            Var::Expression(_) => None,
            _ => None,
        }
    }
}

#[derive(Default)]
struct IdentifierVisitor {
    identifiers: Vec<String>,
}

impl Visitor for IdentifierVisitor {
    fn visit_var(&mut self, var: &Var) {
        let Var::Name(token) = var else {
            return;
        };
        self.push_identifier(token);
    }

    fn visit_prefix(&mut self, prefix: &Prefix) {
        let Prefix::Name(token) = prefix else {
            return;
        };
        self.push_identifier(token);
    }
}

impl IdentifierVisitor {
    fn push_identifier(&mut self, token: &TokenReference) {
        let Some(name) = identifier(token) else {
            return;
        };
        if name == "t" || is_lua_keyword(name) || self.identifiers.iter().any(|item| item == name) {
            return;
        }
        self.identifiers.push(name.to_string());
    }
}

fn direct_call(expression: &Expression) -> Option<(String, &FunctionArgs)> {
    match expression {
        Expression::FunctionCall(call) => call_info(call),
        Expression::Parentheses { expression, .. } => direct_call(expression),
        _ => None,
    }
}

fn call_info(call: &FunctionCall) -> Option<(String, &FunctionArgs)> {
    let Prefix::Name(name) = call.prefix() else {
        return None;
    };
    let mut path = identifier(name)?.to_string();

    for suffix in call.suffixes() {
        match suffix {
            Suffix::Index(Index::Dot { name, .. }) => {
                path.push('.');
                path.push_str(identifier(name)?);
            }
            Suffix::Call(Call::AnonymousCall(arguments)) => return Some((path, arguments)),
            Suffix::Call(Call::MethodCall(method)) => {
                path.push(':');
                path.push_str(identifier(method.name())?);
                return Some((path, method.args()));
            }
            _ => return None,
        }
    }

    None
}

fn string_arguments(arguments: &FunctionArgs) -> impl Iterator<Item = String> + '_ {
    let values = match arguments {
        FunctionArgs::Parentheses { arguments, .. } => arguments
            .iter()
            .filter_map(expression_string_literal)
            .collect::<Vec<_>>(),
        FunctionArgs::String(token) => string_literal(token).into_iter().collect(),
        FunctionArgs::TableConstructor(_) => Vec::new(),
        _ => Vec::new(),
    };
    values.into_iter()
}

fn argument_count(arguments: &FunctionArgs) -> usize {
    match arguments {
        FunctionArgs::Parentheses { arguments, .. } => arguments.len(),
        FunctionArgs::String(_) | FunctionArgs::TableConstructor(_) => 1,
        _ => 0,
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

fn is_string_literal(token: &TokenReference) -> bool {
    matches!(token.token_type(), TokenType::StringLiteral { .. })
}

fn identifier(token: &TokenReference) -> Option<&str> {
    match token.token_type() {
        TokenType::Identifier { identifier } => Some(identifier),
        _ => None,
    }
}

fn alias_target(expression: &Expression) -> Option<ScriptAliasTarget> {
    let path = expression_path(expression)?;
    if let Some(name) = path.strip_prefix("refs.") {
        if !name.is_empty()
            && name
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        {
            return Some(ScriptAliasTarget::Ref(name.to_string()));
        }
    }
    (path == "event.current_target").then_some(ScriptAliasTarget::CurrentTarget)
}

fn expression_path(expression: &Expression) -> Option<String> {
    let Expression::Var(Var::Expression(expression)) = expression else {
        return None;
    };
    let Prefix::Name(name) = expression.prefix() else {
        return None;
    };
    let mut path = identifier(name)?.to_string();
    for suffix in expression.suffixes() {
        let Suffix::Index(Index::Dot { name, .. }) = suffix else {
            return None;
        };
        path.push('.');
        path.push_str(identifier(name)?);
    }
    Some(path)
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn token_span(token: &TokenReference) -> SourceSpan {
    SourceSpan::new(token.start_position().bytes(), token.end_position().bytes())
}

fn position_span(
    range: (
        full_moon::tokenizer::Position,
        full_moon::tokenizer::Position,
    ),
) -> SourceSpan {
    SourceSpan::new(range.0.bytes(), range.1.bytes())
}

fn first_string_span(arguments: &FunctionArgs) -> Option<SourceSpan> {
    match arguments {
        FunctionArgs::Parentheses { arguments, .. } => arguments.iter().find_map(|expression| {
            expression_string_literal(expression)
                .and_then(|_| full_moon::node::Node::range(expression).map(position_span))
        }),
        FunctionArgs::String(token) => Some(token_span(token)),
        _ => None,
    }
}

fn is_lua_keyword(value: &str) -> bool {
    matches!(
        value,
        "and"
            | "break"
            | "do"
            | "else"
            | "elseif"
            | "end"
            | "false"
            | "for"
            | "function"
            | "if"
            | "in"
            | "local"
            | "nil"
            | "not"
            | "or"
            | "repeat"
            | "return"
            | "then"
            | "true"
            | "until"
            | "while"
    )
}

fn classify_import_target(source: &str) -> Option<ComponentImportTarget> {
    if source.starts_with("./")
        || source.starts_with("../")
        || source.starts_with('/')
        || source.starts_with("@src/")
    {
        return Some(ComponentImportTarget::ComponentLocal(source.to_string()));
    }
    if source.starts_with("@mesh/") {
        return Some(ComponentImportTarget::ComponentModule(source.to_string()));
    }
    if source.starts_with("mesh.") {
        let (interface, version) = source
            .split_once('@')
            .map(|(interface, version)| (interface, Some(version.to_string())))
            .unwrap_or((source, None));
        if interface.len() > "mesh.".len() {
            return Some(ComponentImportTarget::InterfaceApi {
                interface: interface.to_string(),
                version,
            });
        }
    }
    None
}
