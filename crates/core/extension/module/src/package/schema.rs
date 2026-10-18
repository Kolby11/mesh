//! Editor field names taken from the runtime's serde declarations.

use super::*;
use serde::{
    Deserialize,
    de::{self, Visitor},
};

#[derive(Debug)]
struct Names(&'static [&'static str]);
impl std::fmt::Display for Names {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("schema names")
    }
}
impl std::error::Error for Names {}
impl de::Error for Names {
    fn custom<T: std::fmt::Display>(_: T) -> Self {
        Self(&[])
    }
}
struct NameProbe;
impl<'de> de::Deserializer<'de> for NameProbe {
    type Error = Names;
    fn deserialize_any<V: Visitor<'de>>(self, _: V) -> Result<V::Value, Names> {
        Err(Names(&[]))
    }
    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        fields: &'static [&'static str],
        _: V,
    ) -> Result<V::Value, Names> {
        Err(Names(fields))
    }
    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _: &'static str,
        variants: &'static [&'static str],
        _: V,
    ) -> Result<V::Value, Names> {
        Err(Names(variants))
    }
    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes byte_buf
        option unit unit_struct newtype_struct seq tuple tuple_struct map identifier ignored_any
    }
}
fn names<T: for<'de> Deserialize<'de>>() -> &'static [&'static str] {
    match T::deserialize(NameProbe) {
        Err(Names(names)) => names,
        Ok(_) => &[],
    }
}

/// Canonical module kind strings accepted by the runtime parser.
pub fn module_kind_names() -> &'static [&'static str] {
    names::<ModuleKind>()
}

/// Canonical keys for editor-visible structured manifest sections. `*` denotes
/// a map entry or an array element. Unknown sections remain freeform.
pub fn manifest_field_names(root_graph: bool, path: &[&str]) -> Option<Vec<&'static str>> {
    let fields = if root_graph {
        match path {
            ["mesh"] => names::<root::RootMeshSection>(),
            ["mesh", "modules", "*"] => names::<InstalledModuleEntry>(),
            _ => return None,
        }
    } else {
        match path {
            [] => names::<ModuleManifest>(),
            ["repository"] => names::<ModuleRepository>(),
            ["mesh"] => names::<MeshModuleSection>(),
            ["mesh", "entrypoints"] => names::<MeshEntrypoints>(),
            ["mesh", "capabilities"] => names::<crate::manifest::CapabilitiesSection>(),
            ["mesh", "uses"] => names::<MeshUses>(),
            ["mesh", "uses", "resources"] => names::<MeshResourceUses>(),
            ["mesh", "dependencies"] => names::<MeshDependencies>(),
            ["mesh", "i18n"] => names::<MeshI18nSupport>(),
            ["mesh", "provides"] => names::<MeshProvides>(),
            ["mesh", "contributes"] => names::<MeshContributes>(),
            ["mesh", "interface"] | ["mesh", "interfaces", "*"] => {
                names::<MeshInterfaceDeclaration>()
            }
            ["mesh", "implements", "*"] => names::<MeshProvidesDeclaration>(),
            ["mesh", "fonts"] => names::<crate::manifest::FontsSection>(),
            ["mesh", "font_pack"] => names::<crate::manifest::FontPackSection>(),
            ["mesh", "icons"] => names::<crate::manifest::IconsSection>(),
            ["mesh", "surface"] => names::<crate::manifest::SurfaceLayoutSection>(),
            _ => return None,
        }
    };
    Some(
        fields
            .iter()
            .copied()
            .filter(|name| {
                !(path == ["mesh"]
                    && !root_graph
                    && matches!(*name, "keybinds" | "icon_pack" | "surface_layout"))
            })
            .collect(),
    )
}
