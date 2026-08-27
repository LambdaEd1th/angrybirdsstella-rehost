//! Native IAP provider boundary used by Telepods and the retired mobile store.

use crate::*;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

#[derive(Debug, Default)]
struct OfflineWallet {
    pending_products: VecDeque<String>,
}

pub(super) fn install(lua: &Lua, globals: &mlua::Table, data_root: Arc<PathBuf>) -> LuaResult<()> {
    let native = lua.create_table()?;
    let provider_state = lua.create_table()?;
    provider_state.set("initialized", false)?;
    let wallet = Rc::new(RefCell::new(OfflineWallet::default()));
    let telepod_products = Rc::new(load_telepod_product_ids(&data_root));

    native.set(
        "native_buyItem",
        lua.create_function(|_, args: MultiValue| {
            // Generated adapter sub_1000D2314 consumes one exact STRING-tag
            // argument. The retired StoreKit catalog cannot create a host
            // transaction, and the shipped facade treats an empty identifier
            // as PURCHASE_FAILED.
            native_required_string(&args, 0, "native_buyItem")?;
            Ok(String::new())
        })?,
    )?;
    native.set(
        "native_restorePurchases",
        lua.create_function(|_, _: MultiValue| Ok(()))?,
    )?;
    native.set(
        "native_getAvailableItems",
        lua.create_function(|lua, _: MultiValue| {
            // sub_1000CD838 serializes the provider catalog. No mobile store
            // provider exists on desktop, so its catalog is empty rather than
            // being fabricated from Telepod voucher products.
            lua.create_table()
        })?,
    )?;

    let initialized_query = provider_state.clone();
    native.set(
        "native_isPaymentInitialized",
        lua.create_function(move |_, _: MultiValue| initialized_query.get::<bool>("initialized"))?,
    )?;

    let fetch_wallet = Rc::clone(&wallet);
    native.set(
        "native_fetchWallet",
        lua.create_function(move |lua, _: MultiValue| {
            // IapManager::fetchWallet (sub_1000CEDAC/sub_1000CF4E4) processes
            // each voucher in this order: deliverItem(productId), followed by
            // onWalletProcessVoucher(voucherProductId, productId, source).
            // Moving the queue out before invoking Lua permits the shipped
            // callbacks to re-enter native_fetchWallet without a RefCell
            // borrow crossing the Lua boundary.
            let products = {
                let mut wallet = fetch_wallet.borrow_mut();
                wallet.pending_products.drain(..).collect::<Vec<_>>()
            };
            if products.is_empty() {
                return Ok(());
            }
            let native: mlua::Table = lua.globals().get("IAP")?;
            let deliver_item = native.get::<Value>("deliverItem")?;
            let process_voucher = native.get::<Value>("onWalletProcessVoucher")?;
            for product in products {
                if let Value::Function(callback) = &deliver_item {
                    callback.call::<()>(product.clone())?;
                }
                if let Value::Function(callback) = &process_voucher {
                    callback.call::<()>((product.clone(), product, "telepod"))?;
                }
            }
            Ok(())
        })?,
    )?;
    native.set(
        "native_useWalletValidation",
        lua.create_function(|_, _: MultiValue| {
            // Purple's direct member sub_1000CDD54 is literally `return 1`.
            Ok(true)
        })?,
    )?;

    let redeem_wallet = Rc::clone(&wallet);
    let redeem_products = Rc::clone(&telepod_products);
    native.set(
        "native_redeemCode",
        lua.create_function(move |lua, args: MultiValue| {
            let code = native_required_string(&args, 0, "native_redeemCode")?;
            let native: mlua::Table = lua.globals().get("IAP")?;
            let Value::Function(callback) = native.get::<Value>("onRedeemResponse")? else {
                return Ok(());
            };

            // The original RCS voucher endpoint is retired. Preserve its
            // callback ABI while allowing deterministic local redemption of
            // the 24 product identifiers shipped in telepod_configuration.
            // Unknown values follow provider error -31 / CODE_NOT_FOUND.
            if let Some(product) = resolve_offline_telepod_product(&code, &redeem_products) {
                redeem_wallet
                    .borrow_mut()
                    .pending_products
                    .push_back(product.clone());
                callback.call::<()>((code, "CODE_OK", product))?;
            } else {
                callback.call::<()>((code, "CODE_NOT_FOUND"))?;
            }
            Ok(())
        })?,
    )?;
    native.set(
        "native_refreshCatalog",
        lua.create_function(|_, _: MultiValue| Ok(()))?,
    )?;

    // Retain the provider state outside the visible table. The original
    // native object owns this state as C++ fields and publishes exactly the
    // eight methods above.
    lua.set_named_registry_value("stella.iap.provider_state", provider_state)?;
    globals.set("IAP", native)?;
    Ok(())
}

pub(crate) fn complete_initialization(lua: &Lua) -> LuaResult<bool> {
    let environment = game_environment(lua)?;
    let Value::Function(register_callbacks) =
        environment.get::<Value>("registerPaymentCallbacks")?
    else {
        return Ok(false);
    };
    register_callbacks.call::<()>(())?;

    let provider_state: mlua::Table = lua.named_registry_value("stella.iap.provider_state")?;
    if provider_state.get::<bool>("initialized")? {
        return Ok(false);
    }
    provider_state.set("initialized", true)?;

    let native: mlua::Table = lua.globals().get("IAP")?;
    // The success callback sub_1000CE5C4 fetches the wallet before notifying
    // Lua that payment initialization has completed.
    if let Value::Function(fetch_wallet) = native.get::<Value>("native_fetchWallet")? {
        fetch_wallet.call::<()>(())?;
    }
    if let Value::Function(callback) = native.get::<Value>("onPaymentInitialized")? {
        let bundle_id = environment
            .get::<Value>("g_iapBundleId")
            .ok()
            .and_then(|value| match value {
                Value::String(value) => value.to_str().ok().map(|value| value.to_string()),
                _ => None,
            })
            .unwrap_or_default();
        callback.call::<()>(bundle_id)?;
    }
    Ok(true)
}

fn load_telepod_product_ids(data_root: &Path) -> BTreeSet<String> {
    let path = data_root.join("config/telepod_configuration.json");
    let Ok(bytes) = std::fs::read(path) else {
        return BTreeSet::new();
    };
    let Ok(document) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return BTreeSet::new();
    };
    document
        .get("characters")
        .and_then(serde_json::Value::as_object)
        .into_iter()
        .flatten()
        .filter_map(|(_, character)| character.get("productId"))
        .filter_map(serde_json::Value::as_str)
        .map(str::to_owned)
        .collect()
}

fn resolve_offline_telepod_product(code: &str, products: &BTreeSet<String>) -> Option<String> {
    if products.contains(code) {
        return Some(code.to_owned());
    }
    // Host scanner backends may retain a URI or textual prefix around the
    // provider payload. Only accept an exact configured product token within
    // that payload; arbitrary numeric codes are never guessed.
    products
        .iter()
        .find(|product| code.contains(product.as_str()))
        .cloned()
}
