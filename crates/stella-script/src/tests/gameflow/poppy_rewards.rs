//! Pending StarTrack reward snapshots and startup accounting.
use super::*;
use mlua::Table;
use std::collections::BTreeMap;

type Pending = BTreeMap<String, (u32, String)>;

fn pending(runtime: &StellaLua) -> Pending {
    let table: Table = runtime
        .lua()
        .load("return SettingsWrapper:getPendingRewards('coins')")
        .set_environment(game_environment(runtime.lua()).unwrap())
        .eval()
        .unwrap();
    table
        .pairs::<String, Table>()
        .map(|entry| {
            let (name, reward) = entry.unwrap();
            (
                name,
                (reward.get("amount").unwrap(), reward.get("screen").unwrap()),
            )
        })
        .collect()
}

#[derive(Debug, PartialEq)]
struct Rewards {
    currency: (u32, u32, u32),
    stars: u32,
    pending: Pending,
}

impl Rewards {
    fn capture(runtime: &StellaLua) -> Self {
        let (stars, coins, gained, used) = runtime
            .lua()
            .load("return SettingsWrapper:getNumber('starsCollected'),Coins:getAmount(),settings.iap.gained.coins,settings.iap.used.coins")
            .set_environment(game_environment(runtime.lua()).unwrap())
            .eval()
            .unwrap();
        Self {
            currency: (coins, gained, used),
            stars,
            pending: pending(runtime),
        }
    }

    fn assert_current(&self, runtime: &StellaLua) {
        assert_eq!(&Self::capture(runtime), self);
    }

    fn after_startup_currency(&self) -> (u32, u32, u32) {
        let grant: u32 = self.pending.values().map(|(amount, _)| *amount).sum();
        (
            self.currency.0 + grant,
            self.currency.1 + grant,
            self.currency.2,
        )
    }
}

#[test]
fn pending_coin_snapshot_preserves_the_original_ungranted_reward() {
    let runtime = StellaLua::new("/tmp/stella-poppy-rewards-snapshot").unwrap();
    runtime.lua().load(r#"
        settings={iap={gained={coins=229},used={coins=60}}}
        Coins={getAmount=function() return 169 end}
        local pending={StarReward_30={amount=25,screen='Level end'}}
        SettingsWrapper={
            getNumber=function(_,key) if key~='starsCollected' then error('unexpected setting') end return 32 end,
            getPendingRewards=function(_,kind) if kind~='coins' then error('unexpected consumable') end return pending end
        }
    "#).set_environment(game_environment(runtime.lua()).unwrap()).exec().unwrap();
    let observed = Rewards::capture(&runtime);
    assert_eq!(observed.currency, (169, 229, 60));
    assert_eq!(observed.stars, 32);
    assert_eq!(
        observed.pending,
        BTreeMap::from([("StarReward_30".to_owned(), (25, "Level end".to_owned()))])
    );
    assert_eq!(observed.after_startup_currency(), (194, 254, 60));
    // Computing the expected startup balance must not grant or clear anything.
    observed.assert_current(&runtime);
}
