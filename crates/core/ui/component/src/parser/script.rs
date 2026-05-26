use crate::{ComponentImport, ComponentImportTarget, ScriptBlock, ScriptLang};

use super::ParseError;

pub(super) fn extract_imports(source: &str) -> Result<(Vec<ComponentImport>, String), ParseError> {
    let mut imports = Vec::new();
    let mut aliases = std::collections::HashSet::new();
    let mut stripped = String::new();

    for (index, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("import ") {
            let import = parse_import_line(trimmed, index + 1)?;
            if !aliases.insert(import.alias.clone()) {
                return Err(ParseError::InvalidImport {
                    line: index + 1,
                    message: format!("duplicate import alias `{}`", import.alias),
                });
            }
            imports.push(import);
            stripped.push('\n');
            continue;
        }
        if let Some(import) = parse_require_import_line(trimmed, index + 1)? {
            if !aliases.insert(import.alias.clone()) {
                return Err(ParseError::InvalidImport {
                    line: index + 1,
                    message: format!("duplicate import alias `{}`", import.alias),
                });
            }
            imports.push(import);
        }
        stripped.push_str(line);
        stripped.push('\n');
    }

    Ok((imports, stripped))
}

fn parse_require_import_line(
    line: &str,
    line_number: usize,
) -> Result<Option<ComponentImport>, ParseError> {
    let Some(rest) = line.strip_prefix("local ") else {
        return Ok(None);
    };
    let Some((alias, expr)) = rest.split_once('=') else {
        return Ok(None);
    };
    let alias = alias.trim();
    if alias.contains(',') || !is_valid_import_alias(alias) {
        return Ok(None);
    }
    let expr = expr.trim();
    let Some(source_part) = expr
        .strip_prefix("require(")
        .and_then(|value| value.strip_suffix(')'))
        .map(str::trim)
    else {
        return Ok(None);
    };
    let source =
        parse_quoted_import_source(source_part).ok_or_else(|| ParseError::InvalidImport {
            line: line_number,
            message: "require source must be a quoted string".into(),
        })?;
    let Some(target) = classify_import_target(&source) else {
        return Ok(None);
    };

    Ok(Some(ComponentImport {
        alias: alias.to_string(),
        target,
    }))
}

fn parse_import_line(line: &str, line_number: usize) -> Result<ComponentImport, ParseError> {
    let rest = line.strip_prefix("import ").unwrap_or(line).trim();
    let Some((alias, source_part)) = rest.split_once(" from ") else {
        return Err(ParseError::InvalidImport {
            line: line_number,
            message: "expected `import Alias from \"source\"`".into(),
        });
    };
    let alias = alias.trim();
    if !is_valid_import_alias(alias) {
        return Err(ParseError::InvalidImport {
            line: line_number,
            message: format!("invalid import alias `{alias}`"),
        });
    }

    let source = parse_quoted_import_source(source_part.trim()).ok_or_else(|| {
        ParseError::InvalidImport {
            line: line_number,
            message: "import source must be a quoted string".into(),
        }
    })?;
    let target = classify_import_target(&source).ok_or_else(|| ParseError::InvalidImport {
        line: line_number,
        message: format!("unsupported import source `{source}`"),
    })?;

    Ok(ComponentImport {
        alias: alias.to_string(),
        target,
    })
}

fn is_valid_import_alias(alias: &str) -> bool {
    let mut chars = alias.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn parse_quoted_import_source(source: &str) -> Option<String> {
    let quote = source.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let end = source[1..].find(quote)?;
    let value = source[1..end + 1].to_string();
    if !source[end + 2..].trim().is_empty() {
        return None;
    }
    Some(value)
}

fn classify_import_target(source: &str) -> Option<ComponentImportTarget> {
    if source.starts_with("./")
        || source.starts_with("../")
        || source.starts_with("/")
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

pub(super) fn parse_script(source: &str) -> ScriptBlock {
    ScriptBlock {
        lang: ScriptLang::Luau,
        source: source.to_string(),
    }
}
