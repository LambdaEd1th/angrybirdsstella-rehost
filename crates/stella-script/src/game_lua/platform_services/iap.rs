//! Native IAP provider boundary used by Telepods and the retired mobile store.

use crate::*;
use std::collections::VecDeque;

#[derive(Clone, Debug)]
struct WalletVoucher {
    voucher_product_id: String,
    product_id: String,
    source: String,
}

#[derive(Clone, Debug)]
enum Completion {
    Initialization,
    RedeemSuccess { code: String, product: String },
    RedeemFailure { code: String, status: String },
    Wallet,
}

#[derive(Debug, Default)]
struct IapState {
    // IapManager+0xA0: 0 uninitialized, 1 initializing, 2 initialized.
    initialization: u8,
    wallet_processing: bool,
    pending_vouchers: VecDeque<WalletVoucher>,
    completions: VecDeque<Completion>,
}

/// Native payment/provider state and application-thread completions.
#[derive(Clone, Debug, Default)]
pub(crate) struct IapRuntime {
    state: Arc<Mutex<IapState>>,
}

impl IapRuntime {
    fn is_initialized(&self) -> bool {
        self.state
            .lock()
            .expect("IAP state lock poisoned")
            .initialization
            == 2
    }

    fn begin_initialization(&self) -> bool {
        let mut state = self.state.lock().expect("IAP state lock poisoned");
        if state.initialization != 0 {
            return false;
        }
        state.initialization = 1;
        state.completions.push_back(Completion::Initialization);
        true
    }

    fn finish_initialization(&self) {
        self.state
            .lock()
            .expect("IAP state lock poisoned")
            .initialization = 2;
    }

    fn begin_wallet_fetch(&self, from_initialization: bool) {
        let mut state = self.state.lock().expect("IAP state lock poisoned");
        let provider_ready =
            state.initialization == 2 || (from_initialization && state.initialization == 1);
        if !provider_ready || state.wallet_processing {
            return;
        }
        state.wallet_processing = true;
        state.completions.push_back(Completion::Wallet);
    }

    fn queue_redeem(&self, code: String, product: Option<String>) {
        let mut state = self.state.lock().expect("IAP state lock poisoned");
        if state.initialization != 2 {
            // rcs::payment::PaymentImpl::redeemVoucher returns an immediate
            // provider-state error before retaining either callback.
            return;
        }
        state.completions.push_back(match product {
            Some(product) => Completion::RedeemSuccess { code, product },
            None => Completion::RedeemFailure {
                code,
                status: "CODE_NOT_FOUND".to_owned(),
            },
        });
    }

    fn take_pending(&self) -> VecDeque<Completion> {
        std::mem::take(
            &mut self
                .state
                .lock()
                .expect("IAP state lock poisoned")
                .completions,
        )
    }

    fn add_voucher(&self, voucher: WalletVoucher) {
        self.state
            .lock()
            .expect("IAP state lock poisoned")
            .pending_vouchers
            .push_back(voucher);
    }

    fn take_wallet_vouchers(&self) -> VecDeque<WalletVoucher> {
        std::mem::take(
            &mut self
                .state
                .lock()
                .expect("IAP state lock poisoned")
                .pending_vouchers,
        )
    }

    fn finish_wallet_fetch(&self) {
        self.state
            .lock()
            .expect("IAP state lock poisoned")
            .wallet_processing = false;
    }
}

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    data_root: Arc<PathBuf>,
) -> LuaResult<IapRuntime> {
    let runtime = IapRuntime::default();
    let native = lua.create_table()?;
    let telepod_products = Arc::new(load_telepod_product_ids(&data_root));

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

    let initialized_runtime = runtime.clone();
    native.set(
        "native_isPaymentInitialized",
        lua.create_function(move |_, _: MultiValue| Ok(initialized_runtime.is_initialized()))?,
    )?;

    let fetch_runtime = runtime.clone();
    native.set(
        "native_fetchWallet",
        lua.create_function(move |_, _: MultiValue| {
            // sub_1000CEDAC sets its processing byte before asking the Wallet
            // provider to complete through a retained asynchronous functor.
            fetch_runtime.begin_wallet_fetch(false);
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

    let redeem_runtime = runtime.clone();
    let redeem_products = Arc::clone(&telepod_products);
    native.set(
        "native_redeemCode",
        lua.create_function(move |_, args: MultiValue| {
            let code = native_required_string(&args, 0, "native_redeemCode")?;

            // The original RCS voucher endpoint is retired. Preserve its
            // callback ABI while allowing deterministic local redemption of
            // the 24 product identifiers shipped in telepod_configuration.
            // Unknown values follow provider error -31 / CODE_NOT_FOUND.
            let product = resolve_offline_telepod_product(&code, &redeem_products);
            redeem_runtime.queue_redeem(code, product);
            Ok(())
        })?,
    )?;
    native.set(
        "native_refreshCatalog",
        lua.create_function(|_, _: MultiValue| Ok(()))?,
    )?;

    globals.set("IAP", native)?;
    Ok(runtime)
}

pub(crate) fn complete_initialization(lua: &Lua, runtime: &IapRuntime) -> LuaResult<bool> {
    let environment = game_environment(lua)?;
    let Value::Function(register_callbacks) =
        environment.get::<Value>("registerPaymentCallbacks")?
    else {
        return Ok(false);
    };
    if !runtime.begin_initialization() {
        return Ok(false);
    }
    register_callbacks.call::<()>(())?;
    Ok(true)
}

/// Deliver payment-provider, redeem and wallet functors on the app thread.
pub(crate) fn dispatch_completions(lua: &Lua, runtime: &IapRuntime) -> LuaResult<()> {
    let native: mlua::Table = lua.globals().get("IAP")?;
    // Snapshot the queue. Redeem success calls into shipped iap.lua, which
    // starts native_fetchWallet; that new request must complete next frame.
    for completion in runtime.take_pending() {
        match completion {
            Completion::Initialization => {
                // sub_1000CE5C4 starts wallet retrieval, calls Lua, and only
                // then publishes IapManager state 2.
                runtime.begin_wallet_fetch(true);
                let environment = game_environment(lua)?;
                let bundle_id = environment
                    .get::<Value>("g_iapBundleId")
                    .ok()
                    .and_then(|value| match value {
                        Value::String(value) => value.to_str().ok().map(|value| value.to_string()),
                        _ => None,
                    })
                    .unwrap_or_default();
                native
                    .get::<mlua::Function>("onPaymentInitialized")?
                    .call::<()>(bundle_id)?;
                runtime.finish_initialization();
            }
            Completion::RedeemSuccess { code, product } => {
                runtime.add_voucher(WalletVoucher {
                    voucher_product_id: product.clone(),
                    product_id: product.clone(),
                    source: "telepod".to_owned(),
                });
                native
                    .get::<mlua::Function>("onRedeemResponse")?
                    .call::<()>((code, "CODE_OK", product))?;
            }
            Completion::RedeemFailure { code, status } => {
                native
                    .get::<mlua::Function>("onRedeemResponse")?
                    .call::<()>((code, status))?;
            }
            Completion::Wallet => {
                let vouchers = runtime.take_wallet_vouchers();
                for voucher in vouchers {
                    // sub_1000CF4E4 preserves this exact delivery order.
                    native
                        .get::<mlua::Function>("deliverItem")?
                        .call::<()>(voucher.product_id.clone())?;
                    native
                        .get::<mlua::Function>("onWalletProcessVoucher")?
                        .call::<()>((
                            voucher.voucher_product_id,
                            voucher.product_id,
                            voucher.source,
                        ))?;
                }
                runtime.finish_wallet_fetch();
            }
        }
    }
    Ok(())
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
