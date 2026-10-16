use super::{ParseError, script::on_parser_stack};
use crate::{SharedCompiledExpression, SourceSpan};
use full_moon::{
    ast::{Call, Expression, FunctionArgs, LastStmt, Suffix},
    node::Node,
};

pub(super) fn compile_handler(
    source: &str,
    span: SourceSpan,
) -> Result<(SharedCompiledExpression, bool), ParseError> {
    const PREFIX: &str = "return ";
    let wrapped = format!("{PREFIX}{source}");
    let invalid = |message: String| ParseError::InvalidTemplate {
        message: format!("invalid Luau handler expression: {message}"),
        span,
    };
    let ast = on_parser_stack(|| full_moon::parse(&wrapped))
        .map_err(|errors| invalid(errors[0].error_message().to_string()))?;
    let Some(LastStmt::Return(statement)) = ast.nodes().last_stmt() else {
        return Err(invalid("expected one function expression".into()));
    };
    if statement.returns().len() != 1 || ast.nodes().stmts().next().is_some() {
        return Err(invalid("expected one function expression".into()));
    }
    let expression = statement.returns().iter().next().unwrap();
    let call_binding = matches!(expression, Expression::FunctionCall(_));
    let lowered = if let Expression::FunctionCall(call) = expression {
        let arguments = match call.suffixes().last() {
            Some(Suffix::Call(Call::AnonymousCall(arguments))) => arguments,
            Some(Suffix::Call(Call::MethodCall(method))) => method.args(),
            _ => return Err(invalid("expected an event call".into())),
        };
        let call = match arguments {
            FunctionArgs::Parentheses {
                parentheses,
                arguments,
            } => {
                let close = parentheses
                    .tokens()
                    .1
                    .token()
                    .start_position()
                    .bytes()
                    .saturating_sub(PREFIX.len());
                let separator = if arguments.is_empty() { "" } else { ", " };
                format!("{}{}...{}", &source[..close], separator, &source[close..])
            }
            _ => {
                let (start, end) = arguments
                    .range()
                    .ok_or_else(|| invalid("missing call argument range".into()))?;
                let start = start.bytes().saturating_sub(PREFIX.len());
                let end = end.bytes().saturating_sub(PREFIX.len());
                format!(
                    "{}({}, ...){}",
                    &source[..start],
                    &source[start..end],
                    &source[end..]
                )
            }
        };
        format!("function(...)\nreturn {call}\nend")
    } else {
        format!("({source}\n)")
    };
    crate::compile_expression(&lowered)
        .map(|compiled| (compiled, call_binding))
        .map_err(|error| invalid(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calls_use_ast_argument_boundaries() {
        for source in [
            "pick({ a = 1, b = nested(2, 3) })",
            "actions:pick({ text = 'a,b)' })",
            "pick 'value'",
            "pick { a = 1, b = 2 }",
            "factory().pick()",
            "pick(1 --[[ trailing argument ]] ) -- trailing comment",
        ] {
            let (expression, call_binding) =
                compile_handler(source, SourceSpan::new(0, source.len())).unwrap();
            assert!(call_binding, "{source}");
            assert!(expression.source().contains("..."));
        }
    }

    #[test]
    fn function_values_and_invalid_expressions_use_one_expression_contract() {
        for source in [
            "private",
            "actions.pick",
            "function(event) local x = event.x; consume(x) end",
        ] {
            assert!(!compile_handler(source, SourceSpan::new(10, 20)).unwrap().1);
        }
        for source in [
            "pick(",
            "first, second",
            "first; return second",
            "local value = 1",
        ] {
            assert!(
                compile_handler(source, SourceSpan::new(10, 20)).is_err(),
                "{source}"
            );
        }
    }
}
