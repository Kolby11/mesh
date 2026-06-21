//! Regression guard: every `module.json` shipped in the workspace must validate
//! cleanly against the LSP manifest schema. If this fails, either a real
//! manifest drifted or the schema in `manifest::schema` fell out of sync with
//! the runtime `mesh_core_module` structs.

use mesh_tools_lsp::manifest::{ManifestDocument, diagnostics::diagnostics};
use tower_lsp::lsp_types::Url;

fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            // Skip build output, VCS, and agent worktrees (which may hold
            // older, legacy-format copies that are not the canonical sources).
            let skip = p
                .file_name()
                .map(|n| n == "target" || n == ".git" || n == ".claude")
                .unwrap_or(false);
            if !skip {
                walk(&p, out);
            }
        } else if p.file_name().map(|n| n == "module.json").unwrap_or(false) {
            out.push(p);
        }
    }
}

#[test]
fn shipped_manifests_validate_cleanly() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../..");
    let mut files = Vec::new();
    walk(std::path::Path::new(root), &mut files);
    assert!(
        !files.is_empty(),
        "expected to find shipped module.json files"
    );

    let mut failures = Vec::new();
    for f in &files {
        let src = std::fs::read_to_string(f).unwrap();
        let doc = ManifestDocument::new(Url::parse("file:///x/module.json").unwrap(), src);
        let diags = diagnostics(&doc);
        if !diags.is_empty() {
            failures.push(format!(
                "{} ({:?}):\n{}",
                f.display(),
                doc.flavor,
                diags
                    .iter()
                    .map(|d| format!("  [{:?}] {}", d.severity.unwrap(), d.message))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "shipped manifests produced diagnostics:\n{}",
        failures.join("\n")
    );
}
