//! Host ABI for Purple's platform-owned external actions.

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
}
