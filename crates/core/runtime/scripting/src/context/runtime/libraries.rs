//! Luau library modules reachable through `require("@scope/name/file")`.
//! See [01 §6](../../../../../../docs/spec/01-module-system.md#6-scripting-model).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use mlua::{Lua, Table, Value as LuaValue};

/// Largest library source file the loader reads.
const MAX_LIBRARY_SOURCE_BYTES: u64 = 1024 * 1024;

/// Library roots one module may load: each `mesh.provides.libraries`
/// namespace of a module it declares in `mesh.uses.modules`, mapped to the
/// directory the contribution names.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LuauLibraries {
    roots: BTreeMap<String, PathBuf>,
}

impl LuauLibraries {
    pub fn insert(&mut self, namespace: impl Into<String>, root: impl Into<PathBuf>) {
        self.roots.insert(namespace.into(), root.into());
    }

    pub fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }

    /// The file a library specifier names, or `None` when `spec` is not a
    /// library specifier for a visible namespace. `@scope/name` loads
    /// `init.luau`; `@scope/name/a/b` loads `a/b.luau`.
    fn resolve(&self, spec: &str) -> Option<Result<PathBuf, String>> {
        let (namespace, rest) = split_library_specifier(spec)?;
        let root = self.roots.get(namespace)?;
        Some(library_file(root, rest.unwrap_or("init")))
    }
}

/// Split `@scope/name[/path]` into the namespace and the optional path.
fn split_library_specifier(spec: &str) -> Option<(&str, Option<&str>)> {
    let body = spec.strip_prefix('@')?;
    let scope_end = body.find('/')?;
    let after_scope = &body[scope_end + 1..];
    match after_scope.find('/') {
        Some(name_end) => {
            let namespace_len = 1 + scope_end + 1 + name_end;
            Some((&spec[..namespace_len], Some(&spec[namespace_len + 1..])))
        }
        None => Some((spec, None)),
    }
}

/// Whether `spec` has the library shape `@scope/name/path`, which is never a
/// component import.
pub(super) fn is_library_path_specifier(spec: &str) -> bool {
    matches!(split_library_specifier(spec), Some((_, Some(_))))
}

fn library_file(root: &Path, rest: &str) -> Result<PathBuf, String> {
    let rest = rest.strip_suffix(".luau").unwrap_or(rest);
    let valid_segment = |segment: &str| {
        !segment.is_empty()
            && segment != "."
            && segment != ".."
            && segment
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    };
    if !rest.split('/').all(valid_segment) {
        return Err(format!("invalid library path '{rest}'"));
    }
    let candidate = root.join(format!("{rest}.luau"));
    let canonical_root = root
        .canonicalize()
        .map_err(|error| format!("library root {}: {error}", root.display()))?;
    let canonical = candidate
        .canonicalize()
        .map_err(|error| format!("{}: {error}", candidate.display()))?;
    if !canonical.starts_with(&canonical_root) {
        return Err(format!("{} escapes its library root", candidate.display()));
    }
    Ok(canonical)
}

/// Load a library for one context. Each context evaluates a library once, in
/// a private environment that reads through to the requiring environment, so
/// library globals never become the component's reactive members.
pub(super) fn require_library(
    lua: &Lua,
    libraries: &LuauLibraries,
    cache: &Table,
    requiring_env: &Table,
    spec: &str,
) -> Option<mlua::Result<LuaValue>> {
    let path = match libraries.resolve(spec)? {
        Ok(path) => path,
        Err(message) => return Some(Err(mlua::Error::runtime(format!("require {spec}: {message}")))),
    };
    Some(load(lua, cache, requiring_env, spec, &path))
}

fn load(
    lua: &Lua,
    cache: &Table,
    requiring_env: &Table,
    spec: &str,
    path: &Path,
) -> mlua::Result<LuaValue> {
    let key = path.to_string_lossy().into_owned();
    match cache.raw_get::<LuaValue>(key.as_str())? {
        LuaValue::LightUserData(_) => {
            return Err(mlua::Error::runtime(format!(
                "require {spec}: circular library require"
            )));
        }
        LuaValue::Nil => {}
        loaded => return Ok(loaded),
    }
    let size = std::fs::metadata(path)
        .map_err(|error| mlua::Error::runtime(format!("require {spec}: {error}")))?
        .len();
    if size > MAX_LIBRARY_SOURCE_BYTES {
        return Err(mlua::Error::runtime(format!(
            "require {spec}: library source exceeds {MAX_LIBRARY_SOURCE_BYTES} bytes"
        )));
    }
    let source = std::fs::read_to_string(path)
        .map_err(|error| mlua::Error::runtime(format!("require {spec}: {error}")))?;
    let env = lua.create_table()?;
    let meta = lua.create_table()?;
    meta.raw_set("__index", requiring_env.clone())?;
    env.set_metatable(Some(meta))?;
    cache.raw_set(key.as_str(), LuaValue::LightUserData(mlua::LightUserData(std::ptr::null_mut())))?;
    let result = lua
        .load(source)
        .set_name(format!("@{spec}"))
        .set_environment(env)
        .eval::<LuaValue>();
    let value = match result {
        Ok(LuaValue::Nil) => LuaValue::Boolean(true),
        Ok(value) => value,
        Err(error) => {
            cache.raw_set(key.as_str(), LuaValue::Nil)?;
            return Err(error);
        }
    };
    cache.raw_set(key.as_str(), value.clone())?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_namespaces_from_library_paths() {
        assert_eq!(
            split_library_specifier("@mesh/kit/volume"),
            Some(("@mesh/kit", Some("volume")))
        );
        assert_eq!(
            split_library_specifier("@mesh/kit/a/b"),
            Some(("@mesh/kit", Some("a/b")))
        );
        assert_eq!(split_library_specifier("@mesh/kit"), Some(("@mesh/kit", None)));
        assert_eq!(split_library_specifier("mesh.audio"), None);
        assert!(is_library_path_specifier("@alice/kit/x"));
        assert!(!is_library_path_specifier("@mesh/audio-popover"));
    }

    #[test]
    fn library_paths_stay_inside_their_root() {
        let root = std::env::temp_dir().join(format!("mesh-library-root-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("volume.luau"), "return 1").unwrap();
        assert!(library_file(&root, "volume").is_ok());
        assert!(library_file(&root, "volume.luau").is_ok());
        assert!(library_file(&root, "../volume").is_err());
        assert!(library_file(&root, "").is_err());
        assert!(library_file(&root, "missing").is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
