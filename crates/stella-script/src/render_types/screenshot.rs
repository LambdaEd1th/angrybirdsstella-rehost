//! Host ABI for Purple's deferred screenshot/share boundary.

/// One `GameLua::shareScreenShot` request after the original Lua argument has
/// crossed the native binding. Purple captures the framebuffer to a uniquely
/// numbered temporary PNG, then passes that path and the supplied title to
/// the platform share service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenshotShareRequest {
    pub sequence: u32,
    pub filename: String,
    pub title: String,
}
