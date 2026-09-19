//! Explicit long audit of every shipped formal-chapter level, including gates
//! and bosses. BirdRun and minigames need their own original entry contexts;
//! they are not silently treated as ordinary chapter transitions here.
//!
//! Run all levels with `cargo test -p stella-script --lib
//! shipped_chapter_levels_follow_original_transitions -- --ignored --nocapture`.
//! Optional `STELLA_AUDIT_CHAPTER=Chapter02` and `STELLA_AUDIT_LIMIT=12`
//! select a bounded batch without changing the default complete audit.

use super::*;

const STARTUP_FRAMES: usize = 600;
const TRANSITION_FRAME_LIMIT: usize = 600;
const IDLE_FRAMES: usize = 180;

#[derive(Debug)]
struct LevelTarget {
    chapter: String,
    ordinal: usize,
    name: String,
}

impl std::fmt::Display for LevelTarget {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(output, "{}[{}] {}", self.chapter, self.ordinal, self.name)
    }
}

#[derive(Debug)]
struct SceneState {
    root: Option<String>,
    level: Option<String>,
    transition_gone: bool,
    level_completed: bool,
    level_failed: bool,
}

fn scene_state(probe: &Function, stage: &str) -> SceneState {
    let (root, level, transition_gone, level_completed, level_failed) = probe
        .call::<(Option<String>, Option<String>, bool, bool, bool)>(())
        .unwrap_or_else(|error| panic!("{stage} scene probe: {error}"));
    SceneState {
        root,
        level,
        transition_gone,
        level_completed,
        level_failed,
    }
}

fn live_world(runtime: &StellaLua) -> mlua::Table {
    game_environment(runtime.lua())
        .unwrap()
        .get::<mlua::Table>("objects")
        .unwrap()
        .get::<mlua::Table>("world")
        .unwrap()
}

fn wait_for_entry(
    runtime: &StellaLua,
    probe: &Function,
    target: &LevelTarget,
    restarted_world: Option<&mlua::Table>,
) -> usize {
    let phase = if restarted_world.is_some() {
        "restart"
    } else {
        "entry"
    };
    let stage = format!("{target} {phase}");
    for frame in 1..=TRANSITION_FRAME_LIMIT {
        advance(runtime, 1, &format!("{stage} frame {frame}"));
        let state = scene_state(probe, &stage);
        // A restart starts with the old target already visible. Retain its
        // world table so pointer reuse cannot turn an unexecuted restart into
        // a false success; shipped loadLevelInternal creates a fresh table.
        let reloaded =
            restarted_world.is_none_or(|old| old.to_pointer() != live_world(runtime).to_pointer());
        if state.transition_gone
            && state.root.as_deref() == Some("GameScene")
            && state.level.as_deref() == Some(target.name.as_str())
            && reloaded
        {
            return frame;
        }
    }
    panic!(
        "{stage} did not finish within {TRANSITION_FRAME_LIMIT} frames; actual={:?}",
        scene_state(probe, &stage)
    );
}

fn enumerate_chapter_levels(runtime: &StellaLua) -> Vec<LevelTarget> {
    let chapter_filter = std::env::var("STELLA_AUDIT_CHAPTER").ok();
    let level_limit = std::env::var("STELLA_AUDIT_LIMIT").ok().map(|value| {
        let limit = value
            .parse::<usize>()
            .expect("STELLA_AUDIT_LIMIT must be a positive integer");
        assert!(limit > 0, "STELLA_AUDIT_LIMIT must not be zero");
        limit
    });
    let actions = game_environment(runtime.lua())
        .unwrap()
        .get::<mlua::Table>("actions")
        .unwrap();
    let episodes = actions.get::<mlua::Table>("myEpisodes").unwrap();
    let all_levels = actions.get::<mlua::Table>("myLevels").unwrap();
    let metadata = actions.get::<mlua::Table>("myLevelsMetadata").unwrap();
    let mut targets = Vec::new();
    let mut selected_chapter = false;
    for chapter in episodes.sequence_values::<String>() {
        let chapter = chapter.expect("myEpisodes must contain chapter names");
        if chapter_filter.as_ref().is_some_and(|name| name != &chapter) {
            continue;
        }
        selected_chapter = true;
        let levels = all_levels
            .get::<mlua::Table>(chapter.as_str())
            .unwrap_or_else(|error| panic!("missing shipped order for {chapter}: {error}"));
        for (index, name) in levels.sequence_values::<String>().enumerate() {
            let ordinal = index + 1;
            let name = name.unwrap_or_else(|error| {
                panic!("{chapter}[{ordinal}] must be a level name: {error}")
            });
            let kind = metadata
                .get::<Option<mlua::Table>>(name.as_str())
                .unwrap()
                .and_then(|entry| entry.get::<Option<String>>("type").unwrap());
            if matches!(kind.as_deref(), Some("menu" | "comic")) {
                eprintln!("[level-audit] SKIP {chapter}[{ordinal}] {name}: metadata type {kind:?}");
                continue;
            }
            targets.push(LevelTarget {
                chapter: chapter.clone(),
                ordinal,
                name,
            });
        }
    }
    assert!(
        selected_chapter,
        "STELLA_AUDIT_CHAPTER must name a shipped formal chapter: {chapter_filter:?}"
    );
    assert!(!targets.is_empty(), "no shipped chapter levels selected");
    if let Some(limit) = level_limit {
        targets.truncate(limit);
    }
    targets
}

#[test]
#[ignore = "long audit: all formal chapter entries, settling frames and first-level restarts"]
fn shipped_chapter_levels_follow_original_transitions() {
    let sandbox = ShippedDataSandbox::new("chapter-level-flow-audit");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.enable_local_services().unwrap();
    runtime.boot("scripts/game.lua").unwrap();
    advance(&runtime, STARTUP_FRAMES, "chapter audit startup");
    let targets = enumerate_chapter_levels(&runtime);
    let environment = game_environment(runtime.lua()).unwrap();
    let probe = runtime
        .lua()
        .load(
            r#"return function()
                local root = menuManager:getRoot()
                return root and root.name, levelName,
                    notificationsFrame:getChild("levelLoadTransition") == nil,
                    not not g_levelCompleted, not not g_levelFailed
            end"#,
        )
        .set_environment(environment.clone())
        .eval::<Function>()
        .unwrap();
    let restart = runtime
        .lua()
        .load("return function() menuManager:getRoot():triggerRestart() end")
        .set_environment(environment.clone())
        .eval::<Function>()
        .unwrap();
    let mut restarted_chapters = BTreeSet::new();
    let audit_started = std::time::Instant::now();
    eprintln!(
        "[level-audit] {} targets; {} idle frames per entry; appdata={}",
        targets.len(),
        IDLE_FRAMES,
        sandbox.root.display()
    );
    for (index, target) in targets.iter().enumerate() {
        let started = std::time::Instant::now();
        eprintln!(
            "[level-audit] BEGIN {}/{} {target}",
            index + 1,
            targets.len()
        );
        assert!(
            scene_state(&probe, &target.to_string()).transition_gone,
            "{target}: a previous transition is still active"
        );
        environment
            .get::<mlua::Table>("LevelLoad")
            .unwrap()
            .get::<Function>("transitionToLevel")
            .unwrap()
            .call::<()>((target.chapter.as_str(), target.ordinal))
            .unwrap_or_else(|error| panic!("{target} transitionToLevel: {error}"));
        let entry_frames = wait_for_entry(&runtime, &probe, target, None);
        advance(&runtime, IDLE_FRAMES, &format!("{target} idle"));
        let mut settled = scene_state(&probe, &format!("{target} settled"));
        if settled.level.as_deref() != Some(target.name.as_str())
            || settled.level_completed
            || settled.level_failed
        {
            // A level can naturally finish while unattended. Preserve and
            // report that original result route; do not patch scores, stop
            // timers, force the scene back, or call it a wrong-entry failure.
            eprintln!("[level-audit] ROUTE-AFTER-IDLE {target}: {settled:?}");
        }
        if !restarted_chapters.contains(&target.chapter)
            && settled.root.as_deref() == Some("GameScene")
            && settled.level.as_deref() == Some(target.name.as_str())
            && settled.transition_gone
            && !settled.level_completed
            && !settled.level_failed
        {
            let old_world = live_world(&runtime);
            restart
                .call::<()>(())
                .unwrap_or_else(|error| panic!("{target} triggerRestart: {error}"));
            let restart_frames = wait_for_entry(&runtime, &probe, target, Some(&old_world));
            advance(&runtime, IDLE_FRAMES, &format!("{target} restart idle"));
            settled = scene_state(&probe, &format!("{target} restart settled"));
            restarted_chapters.insert(target.chapter.clone());
            eprintln!("[level-audit] RESTART {target}: {restart_frames} transition frames");
        }
        for frame in 1..=TRANSITION_FRAME_LIMIT {
            if settled.transition_gone {
                break;
            }
            advance(
                &runtime,
                1,
                &format!("{target} result transition frame {frame}"),
            );
            settled = scene_state(&probe, &format!("{target} result route"));
        }
        assert!(
            settled.transition_gone,
            "{target}: stuck result route {settled:?}"
        );
        assert!(
            runtime.fallback_calls.lock().unwrap().is_empty(),
            "{target}: invoked a missing-global fallback"
        );
        assert!(
            runtime.compatibility_bindings.lock().unwrap().is_empty(),
            "{target}: invoked a compatibility binding"
        );
        eprintln!(
            "[level-audit] DONE {}/{} {target}: entry={entry_frames}f idle={IDLE_FRAMES}f elapsed={:.2}s final={settled:?}",
            index + 1,
            targets.len(),
            started.elapsed().as_secs_f64()
        );
    }
    for target in &targets {
        assert!(
            restarted_chapters.contains(&target.chapter),
            "{} never reached a restartable settled level",
            target.chapter
        );
    }
    eprintln!(
        "[level-audit] COMPLETE {} chapter levels in {:.2}s; restarted {:?}",
        targets.len(),
        audit_started.elapsed().as_secs_f64(),
        restarted_chapters
    );
}
