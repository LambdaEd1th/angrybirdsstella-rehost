use super::*;
use serde_json::json;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

struct Fixture {
    root: PathBuf,
    runtime: StellaLua,
    rating: AppraterRuntime,
    now: Arc<AtomicI64>,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "stella-apprater-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("data")).unwrap();
        let runtime = StellaLua::new(root.join("data")).unwrap();
        let now = Arc::new(AtomicI64::new(1_000_000));
        let clock = Arc::clone(&now);
        let mut rating = runtime.apprater.clone();
        rating.clock = Arc::new(move || clock.load(Ordering::Relaxed));
        Self {
            root,
            runtime,
            rating,
            now,
        }
    }

    fn store(&self) -> RegistryNamespace {
        RegistryNamespace::open(self.rating.path.clone(), &["fusion", "Apprater"]).unwrap()
    }

    fn get(&self, key: &str) -> Option<serde_json::Value> {
        self.store().get(key).unwrap()
    }

    fn show(&self, allowed: bool) {
        self.rating
            .show_alert(allowed, "Game resumed".to_owned())
            .unwrap();
    }

    fn ready(&self) -> AppRatingPrompt {
        for _ in 0..6 {
            self.show(true);
        }
        self.runtime.app_rating_prompt().unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn apprater_lua_adapter_is_strict_counts_false_calls_and_returns_no_values() {
    let f = Fixture::new();
    assert!(!f.rating.path.exists());
    for args in [
        "",
        "nil, 'reason'",
        "1, 'reason'",
        "true",
        "true, 4",
        "{}, 'reason'",
        "false, false",
    ] {
        assert!(
            f.runtime
                .lua
                .load(format!("Apprater.showAlert({args})"))
                .exec()
                .is_err(),
            "accepted {args}"
        );
    }
    assert!(!f.rating.path.exists());
    for _ in 0..6 {
        let count: i32 = f
            .runtime
            .lua
            .load("return select('#', Apprater.showAlert(false, 'silent', nil, 17))")
            .eval()
            .unwrap();
        assert_eq!(count, 0);
    }
    assert_eq!(f.get("tryCount"), Some(json!(6)));
    assert_eq!(f.get("versionString"), Some(json!("1.1.6")));
    assert!(f.runtime.app_rating_prompt().is_none());
    f.runtime
        .lua
        .load("Apprater.showAlert(true, 'prefix' .. string.char(0) .. 'ignored')")
        .exec()
        .unwrap();
    let prompt = f.runtime.app_rating_prompt().unwrap();
    assert_eq!(
        prompt.buttons.iter().map(|b| b.choice).collect::<Vec<_>>(),
        [
            AppRatingChoice::Later,
            AppRatingChoice::Decline,
            AppRatingChoice::Rate
        ]
    );
    assert!(
        f.runtime
            .answer_app_rating(prompt.id, AppRatingChoice::Decline)
            .unwrap()
    );
    assert_eq!(
        f.runtime
            .render
            .lock()
            .unwrap()
            .analytics_events
            .last()
            .unwrap()
            .parameters["shown_because"],
        "prefix"
    );
}

#[test]
fn apprater_sixth_attempt_owns_one_prompt_and_visible_calls_still_increment() {
    let f = Fixture::new();
    for expected in 1..6 {
        f.show(true);
        assert!(f.runtime.app_rating_prompt().is_none());
        assert_eq!(f.get("tryCount"), Some(json!(expected)));
    }
    f.show(true);
    let prompt = f.runtime.app_rating_prompt().unwrap();
    f.store().set("versionString", json!("9.2.0")).unwrap();
    f.show(true);
    f.show(false);
    assert_eq!(f.runtime.app_rating_prompt(), Some(prompt.clone()));
    assert_eq!(f.get("tryCount"), Some(json!(8)));
    // needToPrompt's visible guard runs before version handling.
    assert_eq!(f.get("versionString"), Some(json!("9.2.0")));
    assert_eq!(f.get("promptCount"), None);
    assert!(
        !f.runtime
            .answer_app_rating(prompt.id + 1, AppRatingChoice::Rate)
            .unwrap()
    );
    assert_eq!(f.get("userHasRated"), None);
    assert!(
        f.runtime
            .render
            .lock()
            .unwrap()
            .platform_action_requests
            .is_empty()
    );
}

#[test]
fn apprater_later_waits_two_days_and_survives_a_new_vm() {
    let f = Fixture::new();
    let prompt = f.ready();
    assert!(f.rating.answer(prompt.id, AppRatingChoice::Later).unwrap());
    assert_eq!(f.get("tryCount"), Some(json!(0)));
    assert_eq!(f.get("userPromptedLater"), Some(json!(true)));
    assert_eq!(f.get("promptCount"), Some(json!(1)));
    assert_eq!(f.get("storedTime"), Some(json!(1_000_000.0)));
    let second = StellaLua::new(f.root.join("data")).unwrap();
    let mut rating = second.apprater.clone();
    rating.clock = f.rating.clock.clone();
    f.now.store(1_000_000 + 172_799, Ordering::Relaxed);
    rating.show_alert(true, "not yet".to_owned()).unwrap();
    assert!(second.app_rating_prompt().is_none());
    f.now.store(1_000_000 + 172_800, Ordering::Relaxed);
    rating.show_alert(true, "due".to_owned()).unwrap();
    let prompt = second.app_rating_prompt().unwrap();
    assert!(rating.answer(prompt.id, AppRatingChoice::Later).unwrap());
    assert_eq!(f.get("promptCount"), Some(json!(2)));
    let render = second.render.lock().unwrap();
    assert_eq!(render.analytics_events[0].name, "AppRater");
    assert_eq!(
        render.analytics_events[0].parameters,
        BTreeMap::from([
            ("times_seen".to_owned(), "2".to_owned()),
            ("answer".to_owned(), "LATER".to_owned()),
            ("app_rating_launched".to_owned(), "0".to_owned()),
            ("shown_because".to_owned(), "due".to_owned()),
        ])
    );
    assert!(render.platform_action_requests.is_empty());
}

#[test]
fn apprater_decline_and_rate_suppress_prompts_and_only_rate_queues_store_url() {
    for choice in [AppRatingChoice::Decline, AppRatingChoice::Rate] {
        let f = Fixture::new();
        let prompt = f.ready();
        assert!(f.rating.answer(prompt.id, choice).unwrap());
        assert!(!f.rating.answer(prompt.id, choice).unwrap());
        assert_eq!(f.get("promptCount"), Some(json!(1)));
        let key = if choice == AppRatingChoice::Rate {
            "userHasRated"
        } else {
            "userHasDeclined"
        };
        assert_eq!(f.get(key), Some(json!(true)));
        assert_eq!(f.get("tryCount"), Some(json!(6)));
        f.now.store(2_000_000, Ordering::Relaxed);
        for _ in 0..8 {
            f.show(true);
        }
        assert!(f.runtime.app_rating_prompt().is_none());
        let render = f.runtime.render.lock().unwrap();
        assert_eq!(render.analytics_events.len(), 1);
        if choice == AppRatingChoice::Rate {
            assert_eq!(render.platform_action_requests, [PlatformActionRequest::OpenUrl { url: "itms-apps://itunes.apple.com/WebObjects/MZStore.woa/wa/viewContentsUserReviews?onlyLatestVersion=true&pageNumber=0&sortOrdering=1&type=Purple+Software&id=875251011&mt=8&at=10lcoX".to_owned() }]);
        } else {
            assert!(render.platform_action_requests.is_empty());
        }
    }
}

#[test]
fn apprater_version_reset_distinguishes_patch_major_minor_empty_and_unusual_strings() {
    for (previous, resets) in [
        ("1.1.5", false),
        ("1.1", false),
        ("1.2.6", true),
        ("2.1.6", true),
        ("", false),
        ("1", true),
        (".1.5", true),
        ("..5", true),
    ] {
        let f = Fixture::new();
        let store = f.store();
        for key in ["userHasDeclined", "userHasRated", "userPromptedLater"] {
            store.set(key, json!(true)).unwrap();
        }
        store.set("versionString", json!(previous)).unwrap();
        store.set("tryCount", json!(50)).unwrap();
        store.set("promptCount", json!(9)).unwrap();
        store.set("storedTime", json!(123.0)).unwrap();
        f.show(false);
        assert_eq!(f.get("versionString"), Some(json!("1.1.6")), "{previous}");
        assert_eq!(
            f.get("tryCount"),
            Some(json!(if resets { 0 } else { 51 })),
            "{previous}"
        );
        assert_eq!(f.get("userHasRated"), Some(json!(!resets)), "{previous}");
        assert_eq!(f.get("userHasDeclined"), Some(json!(!resets)), "{previous}");
        assert_eq!(
            f.get("userPromptedLater"),
            Some(json!(!resets)),
            "{previous}"
        );
        assert_eq!(
            f.get("storedTime"),
            Some(json!(if resets { 1_000_000.0 } else { 123.0 })),
            "{previous}"
        );
        assert_eq!(f.get("promptCount"), Some(json!(9)));
    }
}

#[test]
fn apprater_clock_rewind_wrong_leaf_types_and_integer_overflow_match_native() {
    let f = Fixture::new();
    let store = f.store();
    store.set("storedTime", json!(2_000_000.0)).unwrap();
    store.set("tryCount", json!(2_147_483_647_i64)).unwrap();
    f.show(true);
    assert_eq!(f.get("storedTime"), Some(json!(1_000_000.0)));
    assert_eq!(f.get("tryCount"), Some(json!(-2_147_483_648_i64)));
    assert!(f.runtime.app_rating_prompt().is_none());
    store.set("tryCount", json!(5.9)).unwrap();
    for key in ["userHasDeclined", "userHasRated", "userPromptedLater"] {
        store.set(key, json!("true")).unwrap();
    }
    store.set("storedTime", json!("bad")).unwrap();
    f.show(true);
    let prompt = f.runtime.app_rating_prompt().unwrap();
    assert_eq!(f.get("tryCount"), Some(json!(6)));
    store.set("promptCount", json!(2_147_483_647_i64)).unwrap();
    f.rating
        .answer(prompt.id, AppRatingChoice::Decline)
        .unwrap();
    assert_eq!(f.get("promptCount"), Some(json!(-2_147_483_648_i64)));
}

#[test]
fn apprater_clock_reads_keep_the_version_reset_timestamp_from_the_eligibility_check() {
    let f = Fixture::new();
    let store = RatingStore::open(f.rating.path.clone()).unwrap();
    let ticks = AtomicI64::new(10_000);
    let clock = || ticks.fetch_add(1, Ordering::Relaxed);
    // Missing/zero storedTime short-circuits the comparison clock read.
    store.add_try(&clock).unwrap();
    assert_eq!(ticks.load(Ordering::Relaxed), 10_001);
    assert_eq!(f.get("storedTime"), Some(json!(10_000.0)));
    f.store().set("versionString", json!("9.2.0")).unwrap();
    assert!(!store.need_to_prompt(false, "1.1.6", &clock).unwrap());
    assert_eq!(ticks.load(Ordering::Relaxed), 10_002);
    assert_eq!(f.get("storedTime"), Some(json!(10_001.0)));
    f.store().set("storedTime", json!(20_000.0)).unwrap();
    store.add_try(&clock).unwrap();
    assert_eq!(ticks.load(Ordering::Relaxed), 10_004);
    assert_eq!(f.get("storedTime"), Some(json!(10_003.0)));
}

#[test]
fn apprater_registry_failures_do_not_replace_damage_or_present_success() {
    let f = Fixture::new();
    std::fs::create_dir_all(f.rating.path.parent().unwrap()).unwrap();
    std::fs::write(&f.rating.path, b"damaged native registry").unwrap();
    assert!(f.rating.show_alert(true, "must fail".to_owned()).is_err());
    assert!(f.runtime.app_rating_prompt().is_none());
    assert_eq!(
        std::fs::read(&f.rating.path).unwrap(),
        b"damaged native registry"
    );
    assert!(f.runtime.render.lock().unwrap().analytics_events.is_empty());
}

#[test]
fn apprater_resolves_the_current_native_localization_group() {
    let f = Fixture::new();
    f.rating
        .resources
        .lock()
        .unwrap()
        .text_group_sets
        .insert("TEXTS_BASIC".to_owned());
    let translations = BTreeMap::from([
        (
            "TEXT_APPRATER_MESSAGE_TITLE_SHORT".to_owned(),
            "喜欢这个游戏吗？".to_owned(),
        ),
        ("TEXT_APPRATER_RATE_LATER".to_owned(), "稍后提醒".to_owned()),
        (
            "TEXT_APPRATER_CANCEL_BUTTON".to_owned(),
            "不再提示".to_owned(),
        ),
        ("TEXT_APPRATER_RATE_BUTTON".to_owned(), "评分".to_owned()),
    ]);
    {
        let mut locale = f.rating.locale.lock().unwrap();
        locale.current = "zh_CN".to_owned();
        locale.loaded.insert(
            "zh_CN".to_owned(),
            BTreeMap::from([("TEXTS_BASIC".to_owned(), translations)]),
        );
    }
    let prompt = f.ready();
    assert_eq!(prompt.message, "喜欢这个游戏吗？");
    assert_eq!(
        prompt.buttons.map(|b| b.title),
        ["稍后提醒", "不再提示", "评分"]
    );
}
