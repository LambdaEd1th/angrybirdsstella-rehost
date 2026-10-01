//! Exact original StarTrack preparation, animation grant, and startup accounting.
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
pub(super) struct Rewards {
    currency: (u32, u32, u32),
    stars: u32,
    pending: Pending,
}

impl Rewards {
    pub(super) fn capture(runtime: &StellaLua) -> Self {
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

    pub(super) fn finish(self, runtime: &StellaLua, new_stars: u32, collected: u32) -> Self {
        // Original init prepares both endpoints. The animation only traverses
        // newly earned stars; any coin unlock grants all prepared rewards.
        let (prepared, crossed): (Table, bool) = runtime
            .lua()
            .load(
                r#"
            local first,last=...
            local prepared={} local crossed=false
            for count,amount in pairs(g_economyParameters.earning.starTrack.coinBundles) do
                local key=count
                count=tonumber(count)
                if count>=first and count<=last then
                    prepared['StarReward_'..key]=amount
                    if count>first then crossed=true end
                end
            end
            return prepared,crossed
        "#,
            )
            .set_environment(game_environment(runtime.lua()).unwrap())
            .call((self.stars, self.stars + new_stars))
            .unwrap();
        let mut pending = self.pending;
        for entry in prepared.pairs::<String, u32>() {
            let (name, amount) = entry.unwrap();
            pending.insert(name, (amount, "Level end".to_owned()));
        }
        let grant: u32 = if crossed {
            let total = pending.values().map(|(amount, _)| *amount).sum();
            pending.clear();
            total
        } else {
            0
        };
        let expected = Self {
            currency: (
                self.currency.0 + collected + grant,
                self.currency.1 + collected + grant,
                self.currency.2,
            ),
            stars: self.stars + new_stars,
            pending,
        };
        expected.assert_current(runtime);
        eprintln!("[poppy-reward] grant={grant} collected={collected} expected={expected:?}");
        expected
    }

    pub(super) fn assert_current(&self, runtime: &StellaLua) {
        assert_eq!(&Self::capture(runtime), self);
    }

    pub(super) fn after_startup_currency(&self) -> (u32, u32, u32) {
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
