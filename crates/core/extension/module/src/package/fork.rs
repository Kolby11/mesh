//! Tracked forks: a user-owned copy of a module that remembers its upstream,
//! so upstream updates can be reviewed and merged instead of overwritten.
//! See [02 §3](../../../../../../docs/spec/02-installation.md).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{ModuleManifestError, NodeSlotOverride, ShellProfile};

/// Directory inside a fork holding its record and the upstream snapshot it
/// was last synchronized with. Module discovery stops at the fork's own
/// `module.json`, so the snapshot is never loaded as a module.
pub const FORK_STATE_DIR: &str = ".mesh-fork";
const RECORD_FILE: &str = "fork.json";
const BASE_DIR: &str = "base";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForkRecord {
    pub upstream: String,
    /// Upstream version of the snapshot the fork last synchronized with.
    pub upstream_version: String,
}

fn io_error(path: &Path, source: std::io::Error) -> ModuleManifestError {
    ModuleManifestError::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn invalid(message: impl Into<String>) -> ModuleManifestError {
    ModuleManifestError::Validation(message.into())
}

/// The fork record of `fork_dir`, or `None` when it is not a tracked fork.
pub fn fork_record(fork_dir: &Path) -> Result<Option<ForkRecord>, ModuleManifestError> {
    let path = fork_dir.join(FORK_STATE_DIR).join(RECORD_FILE);
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text)
            .map(Some)
            .map_err(|source| ModuleManifestError::Json { path, source }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io_error(&path, error)),
    }
}

/// Copy `upstream_dir` to `destination` as module `fork_id`, keeping a
/// snapshot of the upstream tree for later diffs and rebases.
///
/// Interface modules are refused: a copy would declare a second contract
/// under the same interface name.
pub fn create_fork(
    upstream_dir: &Path,
    upstream_id: &str,
    upstream_version: &str,
    fork_id: &str,
    destination: &Path,
) -> Result<ForkRecord, ModuleManifestError> {
    if destination.exists() {
        return Err(invalid(format!("{} already exists", destination.display())));
    }
    let upstream_manifest = upstream_dir.join("module.json");
    let text = std::fs::read_to_string(&upstream_manifest)
        .map_err(|error| io_error(&upstream_manifest, error))?;
    let manifest: serde_json::Value =
        serde_json::from_str(&text).map_err(|source| ModuleManifestError::Json {
            path: upstream_manifest.clone(),
            source,
        })?;
    if manifest["mesh"]["kind"] == "interface" {
        return Err(invalid(format!(
            "{upstream_id} is an interface module and cannot be forked: the copy would \
             declare a second {} contract. Fork a provider or consumer instead, or \
             publish a new interface under its own name",
            manifest["mesh"]["interface"]["name"].as_str().unwrap_or("interface")
        )));
    }
    copy_tree(upstream_dir, destination)?;
    copy_tree(upstream_dir, &destination.join(FORK_STATE_DIR).join(BASE_DIR))?;
    let manifest = destination.join("module.json");
    let text = std::fs::read_to_string(&manifest).map_err(|error| io_error(&manifest, error))?;
    let renamed = rename_manifest(&text, upstream_id, fork_id).ok_or_else(|| {
        invalid(format!("{} does not name {upstream_id}", manifest.display()))
    })?;
    std::fs::write(&manifest, renamed).map_err(|error| io_error(&manifest, error))?;
    let record = ForkRecord {
        upstream: upstream_id.to_string(),
        upstream_version: upstream_version.to_string(),
    };
    write_record(destination, &record)?;
    Ok(record)
}

/// Replace the manifest's `"name"` value textually, keeping its layout.
fn rename_manifest(text: &str, from: &str, to: &str) -> Option<String> {
    let needle = format!("\"name\": {}", serde_json::to_string(from).ok()?);
    let replacement = format!("\"name\": {}", serde_json::to_string(to).ok()?);
    text.contains(&needle)
        .then(|| text.replacen(&needle, &replacement, 1))
}

fn write_record(fork_dir: &Path, record: &ForkRecord) -> Result<(), ModuleManifestError> {
    let path = fork_dir.join(FORK_STATE_DIR).join(RECORD_FILE);
    let mut text = serde_json::to_string_pretty(record).map_err(|source| ModuleManifestError::Json {
        path: path.clone(),
        source,
    })?;
    text.push('\n');
    std::fs::write(&path, text).map_err(|error| io_error(&path, error))
}

fn copy_tree(from: &Path, to: &Path) -> Result<(), ModuleManifestError> {
    std::fs::create_dir_all(to).map_err(|error| io_error(to, error))?;
    for path in tree_files(from)? {
        let target = to.join(&path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|error| io_error(parent, error))?;
        }
        std::fs::copy(from.join(&path), &target).map_err(|error| io_error(&target, error))?;
    }
    Ok(())
}

/// Regular files under `root`, relative to it, excluding the fork state.
fn tree_files(root: &Path) -> Result<BTreeSet<PathBuf>, ModuleManifestError> {
    let mut files = BTreeSet::new();
    let mut pending = vec![PathBuf::new()];
    while let Some(relative) = pending.pop() {
        let dir = root.join(&relative);
        for entry in std::fs::read_dir(&dir).map_err(|error| io_error(&dir, error))? {
            let entry = entry.map_err(|error| io_error(&dir, error))?;
            let name = entry.file_name();
            if relative.as_os_str().is_empty() && name == FORK_STATE_DIR {
                continue;
            }
            let path = relative.join(&name);
            let kind = entry.file_type().map_err(|error| io_error(&dir, error))?;
            if kind.is_symlink() {
                return Err(invalid(format!(
                    "module tree contains unsupported symlink {}",
                    root.join(&path).display()
                )));
            }
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file() {
                files.insert(path);
            }
        }
    }
    Ok(files)
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, ModuleManifestError> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io_error(path, error)),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileChange {
    Added,
    Removed,
    Modified,
}

/// One changed file with a unified diff when both sides are text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForkFileDiff {
    pub path: PathBuf,
    pub change: FileChange,
    pub patch: Option<String>,
}

/// Changes since the fork last synchronized: upstream's (snapshot → current
/// upstream) and the fork's own (snapshot → fork).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForkDiff {
    pub upstream: Vec<ForkFileDiff>,
    pub local: Vec<ForkFileDiff>,
}

pub fn diff_fork(fork_dir: &Path, upstream_dir: &Path) -> Result<ForkDiff, ModuleManifestError> {
    let base = fork_dir.join(FORK_STATE_DIR).join(BASE_DIR);
    Ok(ForkDiff {
        upstream: diff_trees(&base, upstream_dir)?,
        local: diff_trees(&base, fork_dir)?,
    })
}

fn diff_trees(from: &Path, to: &Path) -> Result<Vec<ForkFileDiff>, ModuleManifestError> {
    let paths = tree_files(from)?
        .into_iter()
        .chain(tree_files(to)?)
        .collect::<BTreeSet<_>>();
    let mut changes = Vec::new();
    for path in paths {
        let (old, new) = (read_optional(&from.join(&path))?, read_optional(&to.join(&path))?);
        let change = match (&old, &new) {
            (None, Some(_)) => FileChange::Added,
            (Some(_), None) => FileChange::Removed,
            (Some(old), Some(new)) if old != new => FileChange::Modified,
            _ => continue,
        };
        let text = |bytes: &Option<Vec<u8>>| match bytes {
            Some(bytes) => std::str::from_utf8(bytes).ok().map(str::to_string),
            None => Some(String::new()),
        };
        let patch = match (text(&old), text(&new)) {
            (Some(old), Some(new)) => Some(
                similar::TextDiff::from_lines(&old, &new)
                    .unified_diff()
                    .header(&format!("a/{}", path.display()), &format!("b/{}", path.display()))
                    .to_string(),
            ),
            _ => None,
        };
        changes.push(ForkFileDiff { path, change, patch });
    }
    Ok(changes)
}

/// What a rebase did to each file that changed upstream.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RebaseOutcome {
    /// Upstream changes taken where the fork had not edited the file.
    pub updated: Vec<PathBuf>,
    /// Files both sides changed whose edits merged cleanly.
    pub merged: Vec<PathBuf>,
    /// Files left with conflict markers (or, for binary files, the fork's
    /// version beside `<file>.upstream`).
    pub conflicts: Vec<PathBuf>,
}

/// Merge upstream changes since the snapshot into the fork, then make the
/// current upstream the new snapshot. Conflicts are written in place with
/// markers; the fork's record advances to `upstream_version`.
pub fn rebase_fork(
    fork_dir: &Path,
    upstream_dir: &Path,
    upstream_version: &str,
) -> Result<RebaseOutcome, ModuleManifestError> {
    let mut record = fork_record(fork_dir)?
        .ok_or_else(|| invalid(format!("{} is not a tracked fork", fork_dir.display())))?;
    let base_dir = fork_dir.join(FORK_STATE_DIR).join(BASE_DIR);
    let paths = tree_files(&base_dir)?
        .into_iter()
        .chain(tree_files(upstream_dir)?)
        .collect::<BTreeSet<_>>();
    let mut outcome = RebaseOutcome::default();
    for path in paths {
        let base = read_optional(&base_dir.join(&path))?;
        let upstream = read_optional(&upstream_dir.join(&path))?;
        if upstream == base {
            continue;
        }
        let target = fork_dir.join(&path);
        let fork = read_optional(&target)?;
        if fork == upstream {
            continue;
        }
        if fork == base {
            write_optional(&target, upstream.as_deref())?;
            outcome.updated.push(path);
            continue;
        }
        let texts = [&base, &fork, &upstream].map(|bytes| {
            bytes
                .as_deref()
                .map(|bytes| std::str::from_utf8(bytes).ok())
                .unwrap_or(Some(""))
        });
        match texts {
            [Some(base), Some(fork), Some(upstream)] if fork_exists_or_upstream(&fork, &upstream) => {
                let (merged, clean) = merge3(base, fork, upstream);
                write_optional(&target, Some(merged.as_bytes()))?;
                if clean {
                    outcome.merged.push(path);
                } else {
                    outcome.conflicts.push(path);
                }
            }
            _ => {
                let mut side = target.clone().into_os_string();
                side.push(".upstream");
                write_optional(Path::new(&side), upstream.as_deref())?;
                outcome.conflicts.push(path);
            }
        }
    }
    std::fs::remove_dir_all(&base_dir).map_err(|error| io_error(&base_dir, error))?;
    copy_tree(upstream_dir, &base_dir)?;
    record.upstream_version = upstream_version.to_string();
    write_record(fork_dir, &record)?;
    Ok(outcome)
}

/// A deletion on one side against an edit on the other is a conflict that
/// the text merge cannot express.
fn fork_exists_or_upstream(fork: &&str, upstream: &&str) -> bool {
    !fork.is_empty() || !upstream.is_empty()
}

fn write_optional(path: &Path, bytes: Option<&[u8]>) -> Result<(), ModuleManifestError> {
    match bytes {
        Some(bytes) => {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|error| io_error(parent, error))?;
            }
            std::fs::write(path, bytes).map_err(|error| io_error(path, error))
        }
        None => match std::fs::remove_file(path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(io_error(path, error)),
            _ => Ok(()),
        },
    }
}

/// One side's replacement of base lines `start..end`.
struct Edit<'a> {
    start: usize,
    end: usize,
    lines: Vec<&'a str>,
    fork: bool,
}

fn edits<'a>(base: &[&'a str], side: &'a str, fork: bool) -> Vec<Edit<'a>> {
    let side_lines = split_lines(side);
    let diff = similar::TextDiff::configure().diff_slices(base, &side_lines);
    diff.ops()
        .iter()
        .filter(|op| op.tag() != similar::DiffTag::Equal)
        .map(|op| {
            let (old, new) = (op.old_range(), op.new_range());
            Edit {
                start: old.start,
                end: old.end,
                lines: side_lines[new].to_vec(),
                fork,
            }
        })
        .collect()
}

fn split_lines(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

/// Line-based three-way merge. Returns the merged text and whether it merged
/// without conflicts; conflicting regions carry git-style markers.
pub fn merge3(base: &str, fork: &str, upstream: &str) -> (String, bool) {
    let base_lines = split_lines(base);
    let mut all = edits(&base_lines, fork, true);
    all.extend(edits(&base_lines, upstream, false));
    all.sort_by_key(|edit| (edit.start, edit.end));

    let mut output = String::new();
    let mut clean = true;
    let mut position = 0;
    let mut index = 0;
    while index < all.len() {
        let (start, mut end) = (all[index].start, all[index].end);
        let mut cluster = vec![&all[index]];
        index += 1;
        // Edits overlap when they share base lines, or when both sides insert
        // at the same point; merely adjacent edits apply in order.
        while index < all.len()
            && (all[index].start < end
                || (all[index].start == all[index].end
                    && all[index].start == end
                    && cluster.iter().any(|edit| {
                        edit.fork != all[index].fork && edit.start == edit.end && edit.end == end
                    })))
        {
            end = end.max(all[index].end);
            cluster.push(&all[index]);
            index += 1;
        }
        output.extend(base_lines[position..start].iter().copied());
        let side = |fork: bool| {
            let mut text = String::new();
            let mut cursor = start;
            for edit in cluster.iter().filter(|edit| edit.fork == fork) {
                text.extend(base_lines[cursor..edit.start].iter().copied());
                text.extend(edit.lines.iter().copied());
                cursor = edit.end;
            }
            text.extend(base_lines[cursor..end].iter().copied());
            text
        };
        let touched = |fork: bool| cluster.iter().any(|edit| edit.fork == fork);
        let (ours, theirs) = (side(true), side(false));
        if !touched(false) || ours == theirs {
            output.push_str(&ours);
        } else if !touched(true) {
            output.push_str(&theirs);
        } else {
            clean = false;
            let terminated = |text: &str| {
                if text.is_empty() || text.ends_with('\n') {
                    text.to_string()
                } else {
                    format!("{text}\n")
                }
            };
            output.push_str("<<<<<<< fork\n");
            output.push_str(&terminated(&ours));
            output.push_str("=======\n");
            output.push_str(&terminated(&theirs));
            output.push_str(">>>>>>> upstream\n");
        }
        position = end;
    }
    output.extend(base_lines[position..].iter().copied());
    (output, clean)
}

/// The active profile's sparse delta after replacing every use of `upstream`
/// with `fork`: roots, providers, and placements in customizable slots
/// (including host defaults the profile did not override). `effective` is
/// the composition-resolved profile; `slot_defaults` yields each active
/// root's `(instance, slot, default references)`.
pub fn rewire_profile(
    profile: &ShellProfile,
    effective: &ShellProfile,
    slot_defaults: &[(String, String, Vec<String>)],
    upstream: &str,
    fork: &str,
) -> (ShellProfile, bool) {
    let mut next = profile.clone();
    let mut changed = false;
    for (instance, root) in &effective.roots {
        if root.module == upstream {
            let entry = next.roots.entry(instance.clone()).or_insert_with(|| {
                super::ProfileRootInstance {
                    module: String::new(),
                    entrypoint: root.entrypoint.clone(),
                    active: None,
                    surface: None,
                }
            });
            entry.module = fork.to_string();
            changed = true;
        }
    }
    for (interface, provider) in &effective.providers {
        if provider == upstream {
            next.providers.insert(interface.clone(), fork.to_string());
            changed = true;
        }
    }
    let prefix = format!("{upstream}:");
    let rewrite = |reference: &str| {
        reference
            .strip_prefix(&prefix)
            .map(|contribution| format!("{fork}:{contribution}"))
    };
    let mut slots = effective
        .node_slots
        .iter()
        .flat_map(|(instance, slots)| {
            slots
                .iter()
                .map(move |(slot, over)| (instance.clone(), slot.clone(), over.nodes.clone()))
        })
        .collect::<Vec<_>>();
    for (instance, slot, defaults) in slot_defaults {
        let overridden = effective
            .node_slots
            .get(instance)
            .is_some_and(|slots| slots.contains_key(slot));
        if !overridden {
            slots.push((
                instance.clone(),
                slot.clone(),
                defaults
                    .iter()
                    .enumerate()
                    .map(|(index, reference)| super::ComponentPlacement {
                        id: format!("default-{index}"),
                        contribution: reference.clone(),
                        props: Default::default(),
                    })
                    .collect(),
            ));
        }
    }
    for (instance, slot, mut nodes) in slots {
        let mut touched = false;
        for node in &mut nodes {
            if let Some(reference) = rewrite(&node.contribution) {
                node.contribution = reference;
                touched = true;
            }
        }
        if touched {
            next.node_slots
                .entry(instance)
                .or_default()
                .insert(slot, NodeSlotOverride { nodes });
            changed = true;
        }
    }
    (next, changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mesh-fork-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn three_way_merge_keeps_both_sides_and_marks_overlaps() {
        let base = "a\nb\nc\nd\ne\n";
        let (merged, clean) = merge3(base, "a\nB\nc\nd\ne\n", "a\nb\nc\nD\ne\n");
        assert!(clean);
        assert_eq!(merged, "a\nB\nc\nD\ne\n");
        let (merged, clean) = merge3(base, "a\nfork\nc\nd\ne\n", "a\nupstream\nc\nd\ne\n");
        assert!(!clean);
        assert_eq!(
            merged,
            "a\n<<<<<<< fork\nfork\n=======\nupstream\n>>>>>>> upstream\nc\nd\ne\n"
        );
        let (merged, clean) = merge3(base, "A\nb\nc\nd\ne\n", "a\nB\nc\nd\ne\n");
        assert!(clean, "adjacent edits merge");
        assert_eq!(merged, "A\nB\nc\nd\ne\n");
        let (merged, clean) = merge3(base, "a\nb\nfork\nc\nd\ne\n", "a\nb\nupstream\nc\nd\ne\n");
        assert!(!clean, "insertions at one point conflict");
        assert!(merged.contains("<<<<<<< fork\nfork\n=======\nupstream\n"));
        let (merged, clean) = merge3(base, "a\nsame\nc\nd\ne\n", "a\nsame\nc\nd\ne\n");
        assert!(clean);
        assert_eq!(merged, "a\nsame\nc\nd\ne\n");
        let (merged, clean) = merge3(base, "a\nb\nc\nd\ne\nfork tail\n", "upstream head\na\nb\nc\nd\ne\n");
        assert!(clean);
        assert_eq!(merged, "upstream head\na\nb\nc\nd\ne\nfork tail\n");
    }

    #[test]
    fn interface_modules_are_not_forked() {
        let root = temp_dir("interface");
        let upstream = root.join("upstream");
        std::fs::create_dir_all(&upstream).unwrap();
        std::fs::write(
            upstream.join("module.json"),
            r#"{"name": "@mesh/wm-interface", "version": "1.1.0",
                "mesh": {"kind": "interface", "interface": {"name": "mesh.wm"}}}"#,
        )
        .unwrap();
        let fork = root.join("fork");

        let error = create_fork(&upstream, "@mesh/wm-interface", "1.1.0", "@me/wm", &fork)
            .unwrap_err()
            .to_string();
        assert!(error.contains("interface module") && error.contains("mesh.wm"), "{error}");
        assert!(!fork.exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn forks_record_upstream_and_rebase_upstream_changes() {
        let root = temp_dir("rebase");
        let upstream = root.join("upstream");
        std::fs::create_dir_all(upstream.join("src")).unwrap();
        std::fs::write(
            upstream.join("module.json"),
            "{\n  \"name\": \"@mesh/clock\",\n  \"version\": \"0.1.0\"\n}\n",
        )
        .unwrap();
        std::fs::write(upstream.join("src/main.mesh"), "one\ntwo\nthree\nfour\n").unwrap();
        std::fs::write(upstream.join("src/style.css"), "a {}\n").unwrap();
        let fork = root.join("fork");

        let record = create_fork(&upstream, "@mesh/clock", "0.1.0", "@me/clock", &fork).unwrap();
        assert_eq!(fork_record(&fork).unwrap(), Some(record));
        assert!(std::fs::read_to_string(fork.join("module.json")).unwrap().contains("\"@me/clock\""));
        assert!(tree_files(&fork).unwrap().iter().all(|path| !path.starts_with(FORK_STATE_DIR)));

        std::fs::write(fork.join("src/main.mesh"), "ONE\ntwo\nthree\nfour\n").unwrap();
        std::fs::write(upstream.join("src/main.mesh"), "one\ntwo\nthree\nFOUR\n").unwrap();
        std::fs::write(upstream.join("src/style.css"), "a { color: red }\n").unwrap();
        std::fs::write(upstream.join("src/new.mesh"), "new\n").unwrap();
        std::fs::write(
            upstream.join("module.json"),
            "{\n  \"name\": \"@mesh/clock\",\n  \"version\": \"0.2.0\"\n}\n",
        )
        .unwrap();

        let diff = diff_fork(&fork, &upstream).unwrap();
        assert_eq!(diff.upstream.len(), 4);
        assert!(diff.local.iter().any(|change| change.path == Path::new("src/main.mesh")));

        let outcome = rebase_fork(&fork, &upstream, "0.2.0").unwrap();
        assert!(outcome.conflicts.is_empty(), "{outcome:?}");
        assert_eq!(
            std::fs::read_to_string(fork.join("src/main.mesh")).unwrap(),
            "ONE\ntwo\nthree\nFOUR\n"
        );
        assert_eq!(std::fs::read_to_string(fork.join("src/new.mesh")).unwrap(), "new\n");
        let manifest = std::fs::read_to_string(fork.join("module.json")).unwrap();
        assert!(manifest.contains("\"@me/clock\"") && manifest.contains("0.2.0"), "{manifest}");
        assert_eq!(fork_record(&fork).unwrap().unwrap().upstream_version, "0.2.0");
        assert!(diff_fork(&fork, &upstream).unwrap().upstream.is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rewiring_replaces_roots_providers_and_slot_references() {
        let profile: ShellProfile =
            serde_json::from_value(serde_json::json!({ "schemaVersion": 3 })).unwrap();
        let effective: ShellProfile = serde_json::from_value(serde_json::json!({
            "schemaVersion": 3,
            "roots": { "@mesh/bar#default": { "module": "@mesh/bar" } },
            "providers": { "mesh.clock": "@mesh/clock-backend" },
            "nodeSlots": { "@mesh/bar#default": { "start": { "nodes": [
                { "id": "a", "use": "@mesh/clock:clock" },
                { "id": "b", "use": "@mesh/other:item" }
            ] } } }
        }))
        .unwrap();
        let defaults = vec![(
            "@mesh/bar#default".to_string(),
            "end".to_string(),
            vec!["@mesh/clock:clock".to_string()],
        )];
        let (next, changed) = rewire_profile(&profile, &effective, &defaults, "@mesh/clock", "@me/clock");
        assert!(changed);
        let start = &next.node_slots["@mesh/bar#default"]["start"].nodes;
        assert_eq!(start[0].contribution, "@me/clock:clock");
        assert_eq!(start[1].contribution, "@mesh/other:item");
        assert_eq!(
            next.node_slots["@mesh/bar#default"]["end"].nodes[0].contribution,
            "@me/clock:clock"
        );
        assert!(next.roots.is_empty());

        let (next, _) = rewire_profile(&profile, &effective, &[], "@mesh/bar", "@me/bar");
        assert_eq!(next.roots["@mesh/bar#default"].module, "@me/bar");
        let (next, _) = rewire_profile(&profile, &effective, &[], "@mesh/clock-backend", "@me/clock-backend");
        assert_eq!(next.providers["mesh.clock"], "@me/clock-backend");
    }
}
