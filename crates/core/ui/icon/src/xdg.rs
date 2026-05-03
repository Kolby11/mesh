use crate::config::IconPackRoot;
use std::path::PathBuf;

pub fn find_icon_in_pack(pack: &IconPackRoot, asset_name: &str, size: u32) -> Option<PathBuf> {
    let path = icon::IconSearch::new_from(vec![pack.root.clone()])
        .search()
        .icons()
        .find_icon(asset_name, size.max(1), 1, &pack.theme)
        .map(|icon| icon.path().to_path_buf());

    path.or_else(|| find_direct_file(pack, asset_name))
}

fn find_direct_file(pack: &IconPackRoot, asset_name: &str) -> Option<PathBuf> {
    ["svg", "png", "jpg", "jpeg", "bmp"]
        .into_iter()
        .map(|ext| pack.root.join(format!("{asset_name}.{ext}")))
        .find(|candidate| candidate.is_file())
}
