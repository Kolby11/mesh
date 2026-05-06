use crate::config::IconPackRoot;
use std::path::{Path, PathBuf};

pub fn find_icon_in_pack(pack: &IconPackRoot, asset_name: &str, size: u32) -> Option<PathBuf> {
    let path = search_for_pack(pack)
        .search()
        .icons()
        .find_icon(asset_name, size.max(1), 1, theme_name(pack))
        .map(|icon| icon.path().to_path_buf());

    path.or_else(|| find_direct_file(pack, asset_name))
}

fn search_for_pack(pack: &IconPackRoot) -> icon::IconSearch {
    match &pack.root {
        Some(root) => icon::IconSearch::new_from(vec![xdg_base_dir_for_root(root)]),
        None => icon::IconSearch::new(),
    }
}

fn xdg_base_dir_for_root(root: &Path) -> PathBuf {
    if root.join("index.theme").is_file() {
        return root.parent().unwrap_or(root).to_path_buf();
    }
    root.to_path_buf()
}

fn theme_name(pack: &IconPackRoot) -> &str {
    if pack.theme != "hicolor" {
        return &pack.theme;
    }
    if let Some(root) = &pack.root {
        if root.join("index.theme").is_file() {
            if let Some(name) = root.file_name().and_then(|name| name.to_str()) {
                return name;
            }
        }
    }
    &pack.theme
}

fn find_direct_file(pack: &IconPackRoot, asset_name: &str) -> Option<PathBuf> {
    let Some(root) = &pack.root else {
        return None;
    };
    ["svg", "png", "jpg", "jpeg", "bmp"]
        .into_iter()
        .map(|ext| root.join(format!("{asset_name}.{ext}")))
        .find(|candidate| candidate.is_file())
}
