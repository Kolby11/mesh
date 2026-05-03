#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltInIconFallback;

impl BuiltInIconFallback {
    pub const NAME: &'static str = "__mesh_builtin_missing_icon";
}
