//! Native IAP provider boundary used by Telepods and the retired mobile store.

use super::skynest_account::IdentityLifetime;
use crate::*;
use std::collections::VecDeque;

mod initialization;
use initialization::ProviderMode;
pub(crate) use initialization::complete_initialization;

#[derive(Clone, Debug)]
struct WalletVoucher {
    voucher_product_id: String,
    product_id: String,
    source: String,
}

#[derive(Clone, Debug)]
struct StoreProduct {
    id: String,
    name: String,
    description: String,
    product_type: String,
    price: String,
    client_data: BTreeMap<String, String>,
}

#[derive(Clone, Debug)]
enum Completion {
    Initialization {
        succeeded: bool,
    },
    PurchaseStatus {
        transaction_id: String,
        purchase_id: String,
        product_id: String,
        status: String,
    },
    RedeemSuccess {
        code: String,
        product: String,
    },
    RedeemFailure {
        code: String,
        status: String,
    },
    Wallet,
}

#[derive(Debug)]
struct IapState {
    // IapManager+0xA0: 0 uninitialized, 1 initializing, 2 initialized.
    initialization: u8,
    wallet_processing: bool,
    provider: ProviderMode,
    generation: u64,
    identity: Option<IdentityLifetime>,
    retry_delay: f32,
    next_transaction_id: u64,
    products: Vec<StoreProduct>,
    pending_vouchers: VecDeque<WalletVoucher>,
    completions: VecDeque<(u64, Completion)>,
}

/// Native payment/provider state and application-thread completions.
#[derive(Clone, Debug)]
pub(crate) struct IapRuntime {
    state: Arc<Mutex<IapState>>,
    application_events: ApplicationEventScheduler,
}

impl IapRuntime {
    fn new(products: Vec<StoreProduct>, application_events: ApplicationEventScheduler) -> Self {
        Self {
            state: Arc::new(Mutex::new(IapState {
                initialization: 0,
                wallet_processing: false,
                provider: ProviderMode::OfflineVouchers,
                generation: 0,
                identity: None,
                retry_delay: 10.0,
                next_transaction_id: 1,
                products,
                pending_vouchers: VecDeque::new(),
                completions: VecDeque::new(),
            })),
            application_events,
        }
    }

    fn is_initialized(&self) -> bool {
        self.lock_state().initialization == 2
    }

    fn begin_wallet_fetch(&self, from_initialization: bool) {
        let mut state = self.lock_state();
        let provider_ready =
            state.initialization == 2 || (from_initialization && state.initialization == 1);
        if !provider_ready || state.wallet_processing {
            return;
        }
        state.wallet_processing = true;
        state.queue(Completion::Wallet);
        self.application_events.post(ApplicationEvent::Iap);
    }

    fn queue_redeem(&self, code: String, product: Option<String>) {
        let mut state = self.lock_state();
        if state.initialization != 2 {
            // rcs::payment::PaymentImpl::redeemVoucher returns an immediate
            // provider-state error before retaining either callback.
            return;
        }
        state.queue(match product {
            Some(product) => Completion::RedeemSuccess { code, product },
            None => Completion::RedeemFailure {
                code,
                status: "CODE_NOT_FOUND".to_owned(),
            },
        });
        self.application_events.post(ApplicationEvent::Iap);
    }

    fn available_products(&self) -> Vec<StoreProduct> {
        let state = self.lock_state();
        if state.provider == ProviderMode::LocalStore {
            state.products.clone()
        } else {
            Vec::new()
        }
    }

    fn begin_purchase(&self, product_id: &str) -> String {
        let mut state = self.lock_state();
        if state.initialization != 2
            || state.provider != ProviderMode::LocalStore
            || !state
                .products
                .iter()
                .any(|product| product.id == product_id)
        {
            return String::new();
        }
        let sequence = state.next_transaction_id;
        state.next_transaction_id = sequence.wrapping_add(1).max(1);
        let transaction_id = format!("local-transaction-{sequence}");
        let purchase_id = format!("local-purchase-{sequence}");
        state.pending_vouchers.push_back(WalletVoucher {
            voucher_product_id: purchase_id.clone(),
            product_id: product_id.to_owned(),
            // Shipped iap.lua stores transactionId under purchaseId in
            // triggerServerDelivery, then resolves that map through the
            // wallet voucher's source field.
            source: purchase_id.clone(),
        });
        state.queue(Completion::PurchaseStatus {
            transaction_id: transaction_id.clone(),
            purchase_id,
            product_id: product_id.to_owned(),
            status: "PURCHASE_SUCCEEDED".to_owned(),
        });
        self.application_events.post(ApplicationEvent::Iap);
        transaction_id
    }

    fn pop_pending(&self) -> Option<(u64, Completion)> {
        let mut state = self.lock_state();
        let (generation, completion) = state.completions.pop_front()?;
        (generation == state.generation).then_some((generation, completion))
    }

    pub(crate) fn discard_completion(&self) {
        let _ = self.pop_pending();
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

    fn finish_wallet_fetch(&self, generation: u64) {
        let mut state = self.lock_state();
        if state.generation == generation {
            state.wallet_processing = false;
        }
    }
}

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    data_root: Arc<PathBuf>,
    application_events: ApplicationEventScheduler,
) -> LuaResult<IapRuntime> {
    let runtime = IapRuntime::new(load_local_store_products(&data_root), application_events);
    let native = lua.create_table()?;
    let telepod_products = Arc::new(load_telepod_product_ids(&data_root));

    native.set(
        "native_buyItem",
        lua.create_function({
            let runtime = runtime.clone();
            move |_, args: MultiValue| {
                // Generated adapter sub_1000D2314 consumes one exact STRING-tag
                // argument. A disconnected provider returns an empty
                // identifier; the opt-in local provider schedules the same
                // status-and-wallet chain as a validated native purchase.
                let product_id = native_required_string(&args, 0, "native_buyItem")?;
                Ok(runtime.begin_purchase(&product_id))
            }
        })?,
    )?;
    native.set(
        "native_restorePurchases",
        lua.create_function(|_, _: MultiValue| Ok(()))?,
    )?;
    native.set(
        "native_getAvailableItems",
        lua.create_function({
            let runtime = runtime.clone();
            move |lua, _: MultiValue| {
                // sub_1000CD838 serializes exactly these six Product fields.
                let catalog = lua.create_table()?;
                for (index, product) in runtime.available_products().into_iter().enumerate() {
                    let item = lua.create_table()?;
                    item.set("id", product.id)?;
                    item.set("name", product.name)?;
                    item.set("description", product.description)?;
                    item.set("type", product.product_type)?;
                    item.set("price", product.price)?;
                    let client_data = lua.create_table()?;
                    for (key, value) in product.client_data {
                        client_data.set(key, value)?;
                    }
                    item.set("clientData", client_data)?;
                    catalog.raw_set(index + 1, item)?;
                }
                Ok(catalog)
            }
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

/// Deliver payment-provider, redeem and wallet functors on the app thread.
pub(crate) fn dispatch_completion(lua: &Lua, runtime: &IapRuntime) -> LuaResult<()> {
    let Some((generation, completion)) = runtime.pop_pending() else {
        return Ok(());
    };
    if matches!(completion, Completion::Initialization { succeeded: false }) {
        // 0x1000CE850 has no Lua error continuation.
        runtime.fail_initialization(generation);
        return Ok(());
    }
    let native: mlua::Table = lua.globals().get("IAP")?;
    match completion {
        Completion::Initialization { succeeded: true } => {
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
            runtime.finish_initialization(generation);
        }
        Completion::Initialization { succeeded: false } => unreachable!(),
        Completion::PurchaseStatus {
            transaction_id,
            purchase_id,
            product_id,
            status,
        } => {
            // sub_1000CE94C reads Purchase fields in this exact order.
            // Shipped iap.lua sees success, calls native_fetchWallet and
            // leaves its listener pending until the following snapshot.
            native
                .get::<mlua::Function>("onPurchaseStatusChanged")?
                .call::<()>((transaction_id, purchase_id, product_id, status))?;
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
                if !runtime.generation_is_current(generation) {
                    return Ok(());
                }
                // sub_1000CF4E4 preserves this exact delivery order.
                native
                    .get::<mlua::Function>("deliverItem")?
                    .call::<()>(voucher.product_id.clone())?;
                if !runtime.generation_is_current(generation) {
                    return Ok(());
                }
                native
                    .get::<mlua::Function>("onWalletProcessVoucher")?
                    .call::<()>((
                        voucher.voucher_product_id,
                        voucher.product_id,
                        voucher.source,
                    ))?;
            }
            runtime.finish_wallet_fetch(generation);
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

fn load_local_store_products(data_root: &Path) -> Vec<StoreProduct> {
    let Ok(bytes) = std::fs::read(data_root.join("config/economy.json")) else {
        return Vec::new();
    };
    let Ok(document) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return Vec::new();
    };
    let Some(bundles) = document
        .pointer("/purchases/currency/bundles")
        .and_then(serde_json::Value::as_object)
    else {
        return Vec::new();
    };
    let mut products = bundles
        .iter()
        .filter_map(|(id, amount)| Some((id.clone(), amount.as_u64()?)))
        .collect::<Vec<_>>();
    products.sort_by_key(|(id, _)| {
        id.rsplit_once('.')
            .and_then(|(_, suffix)| suffix.parse::<u32>().ok())
            .unwrap_or(u32::MAX)
    });
    products
        .into_iter()
        .map(|(id, amount)| StoreProduct {
            name: id.clone(),
            description: String::new(),
            product_type: "CONSUMABLE".to_owned(),
            price: "0".to_owned(),
            client_data: BTreeMap::from([
                ("type".to_owned(), "coins".to_owned()),
                ("coins".to_owned(), amount.to_string()),
            ]),
            id,
        })
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
