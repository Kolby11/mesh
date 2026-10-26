//! `mesh.launch_app`: start an application from its XDG desktop entry
//! ([Desktop Entry Specification](https://specifications.freedesktop.org/desktop-entry-spec/latest/)),
//! independent of the compositor.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// What `Exec` and its neighbours say to run.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct LaunchPlan {
    pub(super) argv: Vec<String>,
    pub(super) working_dir: Option<PathBuf>,
}

/// Launch the application whose desktop file ID is `desktop_id` (with or
/// without the `.desktop` suffix), detached from the shell.
pub(super) fn launch(desktop_id: &str) -> Result<(), String> {
    let id = validated_desktop_id(desktop_id)?;
    let path = application_dirs()
        .into_iter()
        .map(|dir| dir.join(format!("{id}.desktop")))
        .find(|path| path.is_file())
        .ok_or_else(|| format!("no desktop entry for {id}"))?;
    let contents = std::fs::read_to_string(&path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let plan = launch_plan(&contents, &path)?;
    spawn_detached(&plan)
}

fn validated_desktop_id(desktop_id: &str) -> Result<&str, String> {
    let id = desktop_id.strip_suffix(".desktop").unwrap_or(desktop_id);
    let valid = !id.is_empty()
        && !id.starts_with('.')
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
    if valid {
        Ok(id)
    } else {
        Err(format!("invalid desktop file id {desktop_id:?}"))
    }
}

/// `$XDG_DATA_HOME/applications` then each `$XDG_DATA_DIRS` entry, in
/// precedence order.
fn application_dirs() -> Vec<PathBuf> {
    let env = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
    let data_home = env("XDG_DATA_HOME").map(PathBuf::from).or_else(|| {
        env("HOME").map(|home| Path::new(&home).join(".local/share"))
    });
    let data_dirs = env("XDG_DATA_DIRS").unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    data_home
        .into_iter()
        .chain(
            data_dirs
                .split(':')
                .filter(|dir| dir.starts_with('/'))
                .map(PathBuf::from),
        )
        .map(|dir| dir.join("applications"))
        .collect()
}

pub(super) fn launch_plan(contents: &str, path: &Path) -> Result<LaunchPlan, String> {
    let entry = desktop_entry_group(contents);
    let value = |key: &str| {
        entry
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| *value)
    };
    if value("Type").is_some_and(|kind| kind != "Application") {
        return Err(format!("{} is not an application", path.display()));
    }
    if value("Hidden") == Some("true") {
        return Err(format!("{} is hidden", path.display()));
    }
    if value("Terminal") == Some("true") {
        return Err(format!(
            "{} needs a terminal, which launch_app does not open",
            path.display()
        ));
    }
    let exec = value("Exec").ok_or_else(|| format!("{} has no Exec", path.display()))?;
    let argv = expand_exec(
        &unescape_string(exec),
        value("Icon"),
        value("Name"),
        path,
    )?;
    if argv.is_empty() {
        return Err(format!("{} has an empty Exec", path.display()));
    }
    let working_dir = value("Path")
        .filter(|dir| !dir.is_empty())
        .map(|dir| PathBuf::from(unescape_string(dir)));
    Ok(LaunchPlan { argv, working_dir })
}

/// Unlocalized `key=value` pairs of the `[Desktop Entry]` group.
fn desktop_entry_group(contents: &str) -> Vec<(&str, &str)> {
    let mut in_entry = false;
    let mut pairs = Vec::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            if !key.contains('[') {
                pairs.push((key, value.trim()));
            }
        }
    }
    pairs
}

/// The spec's escapes for `string` values: `\s`, `\n`, `\t`, `\r`, `\\`.
fn unescape_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Split an (already string-unescaped) `Exec` value into arguments and expand
/// its field codes for a launch with no files or URLs.
fn expand_exec(
    exec: &str,
    icon: Option<&str>,
    name: Option<&str>,
    path: &Path,
) -> Result<Vec<String>, String> {
    let mut argv = Vec::new();
    for (word, quoted) in split_exec(exec)? {
        if !quoted {
            match word.as_str() {
                "%f" | "%F" | "%u" | "%U" | "%d" | "%D" | "%n" | "%N" | "%v" | "%m" => continue,
                "%i" => {
                    if let Some(icon) = icon.filter(|icon| !icon.is_empty()) {
                        argv.push("--icon".to_string());
                        argv.push(icon.to_string());
                    }
                    continue;
                }
                _ => {}
            }
        }
        argv.push(expand_inline_codes(&word, name, path)?);
    }
    Ok(argv)
}

fn expand_inline_codes(word: &str, name: Option<&str>, path: &Path) -> Result<String, String> {
    let mut out = String::with_capacity(word.len());
    let mut chars = word.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('%') => out.push('%'),
            Some('c') => out.push_str(name.unwrap_or_default()),
            Some('k') => out.push_str(&path.to_string_lossy()),
            Some('f' | 'F' | 'u' | 'U' | 'd' | 'D' | 'n' | 'N' | 'v' | 'm' | 'i') => {}
            Some(other) => return Err(format!("invalid field code %{other} in Exec")),
            None => return Err("Exec ends in a lone %".into()),
        }
    }
    Ok(out)
}

/// Split on unquoted spaces. Inside double quotes, `\"`, `` \` ``, `\$` and
/// `\\` are escapes; the bool marks a word that was quoted.
fn split_exec(exec: &str) -> Result<Vec<(String, bool)>, String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    let mut in_quotes = false;
    let mut chars = exec.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
                quoted = true;
            }
            '\\' if in_quotes => match chars.next() {
                Some(escaped @ ('"' | '`' | '$' | '\\')) => word.push(escaped),
                Some(other) => {
                    word.push('\\');
                    word.push(other);
                }
                None => return Err("Exec ends inside an escape".into()),
            },
            ' ' | '\t' if !in_quotes => {
                if !word.is_empty() || quoted {
                    words.push((std::mem::take(&mut word), quoted));
                }
                quoted = false;
            }
            _ => word.push(c),
        }
    }
    if in_quotes {
        return Err("Exec has an unterminated quote".into());
    }
    if !word.is_empty() || quoted {
        words.push((word, quoted));
    }
    Ok(words)
}

/// Spawn in a new process group with no inherited stdio, so the app neither
/// shares the shell's signals nor holds its pipes, and reap it off-thread.
fn spawn_detached(plan: &LaunchPlan) -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    let mut command = Command::new(&plan.argv[0]);
    command
        .args(&plan.argv[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    if let Some(dir) = plan.working_dir.as_ref() {
        command.current_dir(dir);
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("cannot start {}: {error}", plan.argv[0]))?;
    std::thread::Builder::new()
        .name("mesh-launch-reap".into())
        .spawn(move || {
            let _ = child.wait();
        })
        .map_err(|error| format!("cannot reap {}: {error}", plan.argv[0]))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(entry: &str) -> Result<LaunchPlan, String> {
        launch_plan(entry, Path::new("/apps/demo.desktop"))
    }

    #[test]
    fn exec_field_codes_expand_for_a_launch_without_files() {
        let entry = "[Desktop Entry]\nType=Application\nName=Demo App\nIcon=demo\n\
                     Exec=demo --name=%c %i --open %U --from=%k 100%%\n";
        assert_eq!(
            plan(entry).unwrap().argv,
            [
                "demo",
                "--name=Demo App",
                "--icon",
                "demo",
                "--open",
                "--from=/apps/demo.desktop",
                "100%"
            ]
        );
    }

    #[test]
    fn exec_quoting_and_escapes_follow_the_spec() {
        let entry = "[Desktop Entry]\nExec=\"/opt/My App/run\" \"say \\\\\"hi\\\\\"\" a\\sb \"\"\n";
        assert_eq!(
            plan(entry).unwrap().argv,
            ["/opt/My App/run", "say \"hi\"", "a", "b", ""]
        );
    }

    #[test]
    fn only_the_desktop_entry_group_and_unlocalized_keys_count() {
        let entry = "[Desktop Action new]\nExec=wrong\n[Desktop Entry]\nExec[sk]=zle\n\
                     Exec=right\nPath=/tmp/work\n";
        assert_eq!(
            plan(entry).unwrap(),
            LaunchPlan {
                argv: vec!["right".into()],
                working_dir: Some("/tmp/work".into())
            }
        );
    }

    #[test]
    fn entries_that_cannot_launch_are_refused() {
        assert!(plan("[Desktop Entry]\nType=Link\nExec=x\n").is_err());
        assert!(plan("[Desktop Entry]\nHidden=true\nExec=x\n").is_err());
        assert!(plan("[Desktop Entry]\nTerminal=true\nExec=htop\n").is_err());
        assert!(plan("[Desktop Entry]\nName=x\n").is_err());
        assert!(plan("[Desktop Entry]\nExec=\"open\n").is_err());
        assert!(plan("[Desktop Entry]\nExec=x %z\n").is_err());
    }

    #[test]
    fn spawned_apps_run_detached_in_their_working_directory() {
        let dir = std::env::temp_dir().join(format!("mesh-launch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let marker = dir.join("launched");
        let _ = std::fs::remove_file(&marker);
        let entry = "[Desktop Entry]\nExec=sh -c \"pwd > launched\"\nPath=DIR\n"
            .replace("DIR", &dir.to_string_lossy());
        spawn_detached(&plan(&entry).unwrap()).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !marker.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
        let written = std::fs::read_to_string(&marker).unwrap();
        assert_eq!(written.trim(), dir.to_string_lossy());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn desktop_ids_cannot_name_paths() {
        assert_eq!(validated_desktop_id("org.kde.dolphin.desktop"), Ok("org.kde.dolphin"));
        for id in ["", "../evil", "a/b", ".hidden", "x y"] {
            assert!(validated_desktop_id(id).is_err(), "{id:?}");
        }
    }
}
