//! Host ABI for Purple's platform-owned external actions.

use std::collections::BTreeMap;

/// Event broadcast by Purple's process-global Analytics provider registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyticsEvent {
    /// Milliseconds since the Unix epoch, reconstructed from Purple's
    /// wall-clock/monotonic offset pair.
    pub timestamp_ms: u64,
    /// Event name after the native ASCII-space-to-underscore pass.
    pub name: String,
    /// Native `std::map<string, string>` parameter payload.
    pub parameters: BTreeMap<String, String>,
}

/// Game Center controller selected by FusionGamerServices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GamerServicesView {
    Achievements,
    Leaderboards,
}

/// One external action accepted by the original Lua/native boundary and
/// handed to the desktop application host in call order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlatformActionRequest {
    /// `game::LuaResources::openURL` forwards this string to UIApplication.
    OpenUrl { url: String },
    /// StoreKit product presentation used by ForceUpdate and cross-promotion.
    OpenAppStoreProduct {
        product_id: String,
        product_type: u32,
    },
    /// `GameLua::playVideo` hands a bundle-relative movie path to the
    /// platform media service.
    PlayVideo { path: String },
    /// Purple presents a platform Game Center controller. The portable local
    /// provider hands an equivalent read-only snapshot to the desktop host.
    ShowGamerServices {
        view: GamerServicesView,
        entries: Vec<(String, String)>,
    },
}
