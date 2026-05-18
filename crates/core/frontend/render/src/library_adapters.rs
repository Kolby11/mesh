pub const CURRENT_RENDERER_AUTHORITY: &str = "mesh-software-renderer";
pub const RENDERER_LIBRARY_STATUS_COUNT: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RendererLibraryStatus {
    pub id: &'static str,
    pub feature: &'static str,
    pub role: &'static str,
    pub enabled: bool,
    pub default_authority: &'static str,
}

pub fn renderer_library_statuses() -> [RendererLibraryStatus; RENDERER_LIBRARY_STATUS_COUNT] {
    [
        RendererLibraryStatus {
            id: "taffy",
            feature: "renderer-taffy",
            role: "layout",
            enabled: cfg!(feature = "renderer-taffy"),
            default_authority: CURRENT_RENDERER_AUTHORITY,
        },
        RendererLibraryStatus {
            id: "parley",
            feature: "renderer-parley",
            role: "text",
            enabled: cfg!(feature = "renderer-parley"),
            default_authority: CURRENT_RENDERER_AUTHORITY,
        },
        RendererLibraryStatus {
            id: "accesskit",
            feature: "renderer-accesskit",
            role: "accessibility",
            enabled: cfg!(feature = "renderer-accesskit"),
            default_authority: CURRENT_RENDERER_AUTHORITY,
        },
        RendererLibraryStatus {
            id: "anyrender",
            feature: "renderer-anyrender",
            role: "paint-experimental",
            enabled: cfg!(feature = "renderer-anyrender"),
            default_authority: CURRENT_RENDERER_AUTHORITY,
        },
        RendererLibraryStatus {
            id: "vello_encoding",
            feature: "renderer-vello-encoding",
            role: "paint-encoding-experimental",
            enabled: cfg!(feature = "renderer-vello-encoding"),
            default_authority: CURRENT_RENDERER_AUTHORITY,
        },
    ]
}

pub fn renderer_library_rollback_authority() -> &'static str {
    CURRENT_RENDERER_AUTHORITY
}
