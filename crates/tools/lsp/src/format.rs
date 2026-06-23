//! Structural formatter for `.mesh` single-file components.
//!
//! `.mesh` files contain up to four top-level blocks — `<template>`,
//! `<script lang="luau">`, `<style>`, and `<i18n>` — each authored in a
//! different sub-language. The formatter is block-aware:
//!
//! - **template** (XHTML-like): re-indented from element/`{#block}` nesting.
//! - **style** (CSS-like): re-indented from brace nesting.
//! - **script** (Luau): formatted with stylua, the standard Luau formatter.
//!   If stylua cannot parse the block (e.g. a mid-edit syntax error), it falls
//!   back to rebasing the block flush-left while preserving the author's
//!   relative indentation.
//! - **i18n / other**: preserved verbatim (only trailing whitespace stripped).
//!
//! Top-level block tags and their closers sit at column 0; template and style
//! content starts one indent level in; script content is flush-left, matching
//! the shipped module corpus.

/// Re-format a whole `.mesh` document. `indent_unit` is one level of
/// indentation (e.g. four spaces or a tab), derived from the editor's
/// formatting options. The returned string always ends in a single newline.
pub fn format_document(source: &str, indent_unit: &str) -> String {
    let mut out = String::new();
    let mut state = State::TopLevel;

    for raw in source.split('\n') {
        let trimmed = raw.trim_end();
        let trimmed_start = trimmed.trim_start();

        match &mut state {
            State::TopLevel => {
                if let Some(kind) = block_open_kind(trimmed_start) {
                    out.push_str(trimmed_start);
                    out.push('\n');
                    state = State::InBlock(BlockState::new(kind));
                } else {
                    // Stray top-level content / blank lines: keep flush-left.
                    out.push_str(trimmed_start);
                    out.push('\n');
                }
            }
            State::InBlock(block) => {
                if is_block_close(trimmed_start, block.kind) {
                    block.flush(&mut out, indent_unit);
                    out.push_str(trimmed_start);
                    out.push('\n');
                    state = State::TopLevel;
                } else {
                    block.push_line(trimmed);
                }
            }
        }
    }

    if let State::InBlock(block) = &mut state {
        block.flush(&mut out, indent_unit);
    }

    // Normalize to exactly one trailing newline.
    while out.ends_with("\n\n") {
        out.pop();
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

enum State {
    TopLevel,
    InBlock(BlockState),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BlockKind {
    Template,
    Script,
    Style,
    Other,
}

/// Detect a top-level block opening tag (`<template>`, `<script ...>`, etc.).
fn block_open_kind(trimmed_start: &str) -> Option<BlockKind> {
    let after = trimmed_start.strip_prefix('<')?;
    if after.starts_with('/') {
        return None;
    }
    let name: String = after
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    match name.as_str() {
        "template" => Some(BlockKind::Template),
        "script" => Some(BlockKind::Script),
        "style" => Some(BlockKind::Style),
        "i18n" => Some(BlockKind::Other),
        _ => None,
    }
}

fn is_block_close(trimmed_start: &str, kind: BlockKind) -> bool {
    let name = match kind {
        BlockKind::Template => "template",
        BlockKind::Script => "script",
        BlockKind::Style => "style",
        BlockKind::Other => "i18n",
    };
    trimmed_start
        .strip_prefix("</")
        .map(|rest| rest.trim_start().starts_with(name))
        .unwrap_or(false)
}

struct BlockState {
    kind: BlockKind,
    /// Raw, trailing-trimmed lines collected for this block's content.
    lines: Vec<String>,
}

impl BlockState {
    fn new(kind: BlockKind) -> Self {
        Self {
            kind,
            lines: Vec::new(),
        }
    }

    fn push_line(&mut self, trimmed_end: &str) {
        self.lines.push(trimmed_end.to_string());
    }

    /// Render the accumulated content into `out`, then reset.
    fn flush(&mut self, out: &mut String, indent_unit: &str) {
        let lines = std::mem::take(&mut self.lines);
        let rendered = match self.kind {
            BlockKind::Template => format_template(&lines, indent_unit),
            BlockKind::Style => format_style(&lines, indent_unit),
            BlockKind::Script => format_script(&lines, indent_unit),
            BlockKind::Other => format_preserve(&lines),
        };
        for line in rendered {
            out.push_str(&line);
            out.push('\n');
        }
    }
}

/// Render `level` copies of `indent_unit`.
fn indent(level: usize, unit: &str) -> String {
    unit.repeat(level)
}

// ---------------------------------------------------------------------------
// Template (XHTML-like)
// ---------------------------------------------------------------------------

fn format_template(lines: &[String], unit: &str) -> Vec<String> {
    let mut out = Vec::with_capacity(lines.len());
    // Content sits one level inside the <template> tag.
    let mut depth: i32 = 1;
    // While inside a multi-line opening tag header, attributes indent one level
    // past the tag, and the closing `>` aligns with the tag itself.
    let mut tag_header: Option<i32> = None;

    for line in lines {
        let t = line.trim_start();
        if t.is_empty() {
            out.push(String::new());
            continue;
        }

        if let Some(tag_base) = tag_header {
            match find_tag_end(t) {
                Some(gt) => {
                    // The `>` / `/>` closing the multi-line tag aligns with the tag.
                    out.push(format!("{}{}", indent(tag_base.max(1) as usize, unit), t));
                    let self_closing = t[..gt].trim_end().ends_with('/');
                    depth = if self_closing { tag_base } else { tag_base + 1 };
                    tag_header = None;
                }
                None => {
                    // Attribute continuation line.
                    out.push(format!(
                        "{}{}",
                        indent((tag_base + 1).max(1) as usize, unit),
                        t
                    ));
                }
            }
            continue;
        }

        let scan = scan_template(t);
        let line_level = if scan.lead_close {
            (depth - 1).max(1)
        } else {
            depth.max(1)
        };
        out.push(format!("{}{}", indent(line_level as usize, unit), t));

        if scan.opens_multiline_tag {
            tag_header = Some(depth);
        } else {
            depth = (depth + scan.net).max(1);
        }
    }

    out
}

struct TagScan {
    /// The first token on the line closes a scope (`</tag>`, `{/block}`, `{:else}`).
    lead_close: bool,
    /// Net nesting change contributed by complete constructs on this line.
    net: i32,
    /// The line ends inside an unterminated `<tag ...` header.
    opens_multiline_tag: bool,
}

fn scan_template(t: &str) -> TagScan {
    let bytes = t.as_bytes();
    let mut net = 0i32;
    let mut lead_close = false;
    let mut opens_multiline = false;
    let mut seen = false;
    let mut i = 0;

    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let is_first = !seen;

        if c == b'<' && bytes.get(i + 1) == Some(&b'/') {
            // Closing element tag.
            if is_first {
                lead_close = true;
            }
            net -= 1;
            seen = true;
            match t[i..].find('>') {
                Some(rel) => i += rel + 1,
                None => break,
            }
            continue;
        }

        if c == b'<' && bytes.get(i + 1).is_some_and(|b| b.is_ascii_alphabetic()) {
            // Opening element tag.
            seen = true;
            match find_tag_end(&t[i..]) {
                Some(rel) => {
                    let self_closing = t[i..i + rel].trim_end().ends_with('/');
                    if !self_closing {
                        net += 1;
                    }
                    i += rel + 1;
                }
                None => {
                    opens_multiline = true;
                    break;
                }
            }
            continue;
        }

        if c == b'{' {
            match bytes.get(i + 1) {
                Some(b'#') => net += 1,
                Some(b'/') => {
                    if is_first {
                        lead_close = true;
                    }
                    net -= 1;
                }
                Some(b':') => {
                    if is_first {
                        lead_close = true;
                    }
                }
                _ => {}
            }
            seen = true;
            match t[i..].find('}') {
                Some(rel) => i += rel + 1,
                None => i += 1,
            }
            continue;
        }

        seen = true;
        i += 1;
    }

    TagScan {
        lead_close,
        net,
        opens_multiline_tag: opens_multiline,
    }
}

/// Index of the `>` that closes a tag starting at the slice's front, honoring
/// quoted attribute values. Returns `None` if the tag is unterminated.
fn find_tag_end(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut quote: u8 = 0;
    for (i, &c) in bytes.iter().enumerate() {
        if quote != 0 {
            if c == quote {
                quote = 0;
            }
        } else if c == b'"' || c == b'\'' {
            quote = c;
        } else if c == b'>' {
            return Some(i);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Style (CSS-like)
// ---------------------------------------------------------------------------

fn format_style(lines: &[String], unit: &str) -> Vec<String> {
    let mut out = Vec::with_capacity(lines.len());
    let mut depth: i32 = 1;

    for line in lines {
        let t = line.trim_start();
        if t.is_empty() {
            out.push(String::new());
            continue;
        }

        let lead_close = t.starts_with('}');
        let line_level = if lead_close {
            (depth - 1).max(1)
        } else {
            depth.max(1)
        };
        out.push(format!("{}{}", indent(line_level as usize, unit), t));

        let (opens, closes) = count_braces(t);
        depth = (depth + opens - closes).max(1);
    }

    out
}

/// Count `{` / `}` braces, ignoring those inside strings.
fn count_braces(t: &str) -> (i32, i32) {
    let bytes = t.as_bytes();
    let mut opens = 0;
    let mut closes = 0;
    let mut quote: u8 = 0;
    for &c in bytes {
        if quote != 0 {
            if c == quote {
                quote = 0;
            }
        } else if c == b'"' || c == b'\'' {
            quote = c;
        } else if c == b'{' {
            opens += 1;
        } else if c == b'}' {
            closes += 1;
        }
    }
    (opens, closes)
}

// ---------------------------------------------------------------------------
// Script (Luau)
// ---------------------------------------------------------------------------

fn format_script(lines: &[String], unit: &str) -> Vec<String> {
    let source = lines.join("\n");

    // Run the block through stylua, the standard Luau formatter, so the script
    // is properly reformatted (collapsing manual alignment padding, normalizing
    // indentation and spacing) rather than just rebased. stylua emits content
    // flush-left, which matches the `.mesh` script convention.
    if let Some(formatted) = stylua_format(&source, unit) {
        return formatted.lines().map(str::to_string).collect();
    }

    // Fallback for content stylua can't parse (e.g. mid-edit syntax errors):
    // strip the longest common leading-whitespace prefix to rebase the block
    // flush-left, preserving the author's relative indentation.
    let common = common_whitespace_prefix(lines);
    lines
        .iter()
        .map(|line| {
            if line.trim().is_empty() {
                String::new()
            } else {
                line.strip_prefix(&common).unwrap_or(line).to_string()
            }
        })
        .collect()
}

/// Format a Luau chunk with stylua, honoring the editor's indent preference.
/// Returns `None` if stylua cannot parse the source.
fn stylua_format(source: &str, unit: &str) -> Option<String> {
    use stylua_lib::{Config, IndentType, LuaVersion, OutputVerification};

    let (indent_type, indent_width) = if unit.contains('\t') {
        (IndentType::Tabs, unit.replace('\t', "    ").len().max(1))
    } else {
        (IndentType::Spaces, unit.len().max(1))
    };

    let config = Config {
        line_endings: stylua_lib::LineEndings::Unix,
        indent_type,
        indent_width,
        syntax: LuaVersion::Luau,
        ..Config::default()
    };

    let formatted = stylua_lib::format_code(source, config, None, OutputVerification::None).ok()?;
    Some(formatted.trim_end_matches('\n').to_string())
}

/// Longest leading-whitespace string shared by every non-blank line.
fn common_whitespace_prefix(lines: &[String]) -> String {
    let mut prefix: Option<String> = None;
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let ws: String = line
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();
        prefix = Some(match prefix {
            None => ws,
            Some(existing) => {
                let common_len = existing
                    .chars()
                    .zip(ws.chars())
                    .take_while(|(a, b)| a == b)
                    .count();
                existing.chars().take(common_len).collect()
            }
        });
        if prefix.as_deref() == Some("") {
            break;
        }
    }
    prefix.unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Other blocks — preserve verbatim
// ---------------------------------------------------------------------------

fn format_preserve(lines: &[String]) -> Vec<String> {
    lines.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNIT: &str = "    ";

    fn fmt(src: &str) -> String {
        format_document(src, UNIT)
    }

    #[test]
    fn reindents_template_nesting() {
        let src =
            "<template>\n<box>\n<button>\n<icon name=\"x\" />\n</button>\n</box>\n</template>\n";
        let expected = "\
<template>
    <box>
        <button>
            <icon name=\"x\" />
        </button>
    </box>
</template>
";
        assert_eq!(fmt(src), expected);
    }

    #[test]
    fn handles_multiline_opening_tag() {
        let src = "<template>\n<button\nref=\"r\"\nonclick={f}\n>\n<icon name=\"x\" />\n</button>\n</template>\n";
        let expected = "\
<template>
    <button
        ref=\"r\"
        onclick={f}
    >
        <icon name=\"x\" />
    </button>
</template>
";
        assert_eq!(fmt(src), expected);
    }

    #[test]
    fn reindents_control_blocks() {
        let src = "<template>\n<box>\n{#if cond}\n<text>{a}</text>\n{:else}\n<text>{b}</text>\n{/if}\n</box>\n</template>\n";
        let expected = "\
<template>
    <box>
        {#if cond}
            <text>{a}</text>
        {:else}
            <text>{b}</text>
        {/if}
    </box>
</template>
";
        assert_eq!(fmt(src), expected);
    }

    #[test]
    fn reindents_style_braces() {
        let src = "<style>\n.btn {\nwidth: 40px;\n}\n@container (max-width: 720px) {\n.btn {\nwidth: 36px;\n}\n}\n</style>\n";
        let expected = "\
<style>
    .btn {
        width: 40px;
    }
    @container (max-width: 720px) {
        .btn {
            width: 36px;
        }
    }
</style>
";
        assert_eq!(fmt(src), expected);
    }

    #[test]
    fn script_formatted_with_stylua() {
        // stylua reindents the block flush-left and normalizes spacing.
        let src = "<script lang=\"luau\">\n    local x = 1\n    if x then\n        x = 2\n    end\n</script>\n";
        let expected = "\
<script lang=\"luau\">
local x = 1
if x then
    x = 2
end
</script>
";
        assert_eq!(fmt(src), expected);
    }

    #[test]
    fn script_stylua_collapses_alignment_padding() {
        let src = "<script lang=\"luau\">\nlocal a       = 1\nlocal bb      = 2\n</script>\n";
        let expected = "\
<script lang=\"luau\">
local a = 1
local bb = 2
</script>
";
        assert_eq!(fmt(src), expected);
    }

    #[test]
    fn script_falls_back_when_unparseable() {
        // A syntax error (unterminated string) means stylua cannot parse it;
        // the fallback rebases flush-left while preserving relative indentation.
        let src = "<script lang=\"luau\">\n    local x = \"oops\n    broken(\n</script>\n";
        let out = fmt(src);
        // Fallback strips the common 4-space prefix; content is preserved.
        assert!(out.contains("\nlocal x = \"oops\n"), "got:\n{out}");
        assert!(out.contains("\nbroken(\n"), "got:\n{out}");
    }

    #[test]
    fn preserves_blank_lines_between_blocks() {
        let src = "<template>\n<box />\n</template>\n\n<style>\n.a {\ncolor: red;\n}\n</style>\n";
        let expected = "\
<template>
    <box />
</template>

<style>
    .a {
        color: red;
    }
</style>
";
        assert_eq!(fmt(src), expected);
    }

    #[test]
    fn idempotent_on_formatted_output() {
        let src = "<template>\n<box>\n<button onclick={f}>\n<icon name=\"x\" />\n</button>\n</box>\n</template>\n\n<style>\n.btn {\nwidth: 40px;\n}\n</style>\n";
        let once = fmt(src);
        let twice = format_document(&once, UNIT);
        assert_eq!(once, twice);
    }

    #[test]
    fn tabs_as_indent_unit() {
        let src = "<template>\n<box>\n<text>hi</text>\n</box>\n</template>\n";
        let expected = "<template>\n\t<box>\n\t\t<text>hi</text>\n\t</box>\n</template>\n";
        assert_eq!(format_document(src, "\t"), expected);
    }
}
