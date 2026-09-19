//! Ordered Entity::remove/setParent events used by AnimationWrapper::load.

use super::*;

pub(super) fn prepare(
    runtime: &mut AnimationRuntime,
    tag: String,
    asset: AnimationAsset,
    (sprite_geometry, sprite_metrics, sprite_regions): AnimationResourceSnapshot,
) -> (Option<u64>, u64) {
    if !runtime.root_present {
        runtime.root_generation += 1;
        runtime.root_present = true;
    }
    let previous = runtime.scene_generations.get(&tag).copied();
    runtime
        .skin_sets
        .insert(tag.clone(), asset.definition.skins.clone());
    runtime.next_scene_generation += 1;
    let generation = runtime.next_scene_generation;
    // The new AnimationSkins pointer overwrites wrapper +0x58 before the
    // first scheduler drain, independently of which scene findScene sees.
    if let Some(skin) = asset
        .definition
        .skins
        .keys()
        .find(|skin| skin.as_str() == "default")
        .or_else(|| asset.definition.skins.keys().next())
    {
        runtime.skins.insert(tag.clone(), skin.clone());
    } else {
        runtime.skins.remove(&tag);
    }
    runtime.pending_scene_attachments.insert(
        generation,
        PendingAnimationScene {
            tag,
            generation,
            root_generation: runtime.root_generation,
            asset,
            sprite_geometry,
            sprite_metrics,
            sprite_regions,
        },
    );
    (previous, generation)
}

pub(super) fn attach(
    runtime: &mut AnimationRuntime,
    callbacks: &Table,
    generation: u64,
) -> LuaResult<()> {
    let Some(scene) = runtime.pending_scene_attachments.remove(&generation) else {
        return Ok(());
    };
    // setParent retains its concrete root. A closeAll may have replaced the
    // global root in the meantime; attaching to that old root cannot revive
    // a scene in the new tree.
    if !runtime.root_present || scene.root_generation != runtime.root_generation {
        return Ok(());
    }
    if runtime.definitions.contains_key(&scene.tag) {
        runtime
            .shadow_scenes
            .entry(scene.tag.clone())
            .or_default()
            .push_back(scene);
        return Ok(());
    }
    install(runtime, callbacks, scene)
}

fn install(
    runtime: &mut AnimationRuntime,
    callbacks: &Table,
    scene: PendingAnimationScene,
) -> LuaResult<()> {
    let tag = scene.tag;
    let skin = runtime.skins.get(&tag).cloned();
    let skin_set = runtime.skin_sets.get(&tag).cloned();
    install_animation_asset(
        runtime,
        tag.clone(),
        scene.asset,
        scene.sprite_geometry,
        scene.sprite_metrics,
        scene.sprite_regions,
    );
    runtime
        .scene_generations
        .insert(tag.clone(), scene.generation);
    if let Some(skin_set) = skin_set {
        runtime.skin_sets.insert(tag.clone(), skin_set);
    } else {
        runtime.skin_sets.remove(&tag);
    }
    // Do not roll the wrapper's latest skin pointer back to the skin that
    // belonged to an older queued load.
    if let Some(skin) = skin {
        runtime.skins.insert(tag.clone(), skin);
    } else {
        runtime.skins.remove(&tag);
    }
    // The newly constructed component has no callback, even if its tag was
    // used by a component that is still held by an outer event snapshot.
    callbacks.raw_set(tag, Value::Nil)
}

pub(super) fn remove(
    runtime: &mut AnimationRuntime,
    callbacks: &Table,
    generation: u64,
) -> LuaResult<()> {
    let tag = runtime
        .scene_generations
        .iter()
        .find_map(|(tag, current)| (*current == generation).then(|| tag.clone()));
    if let Some(tag) = tag {
        remove_animation_scene(runtime, callbacks, &tag)?;
        if let Some(scene) = runtime
            .shadow_scenes
            .get_mut(&tag)
            .and_then(|scenes| scenes.pop_front())
        {
            install(runtime, callbacks, scene)?;
        }
    } else {
        for scenes in runtime.shadow_scenes.values_mut() {
            scenes.retain(|scene| scene.generation != generation);
        }
    }
    runtime.shadow_scenes.retain(|_, scenes| !scenes.is_empty());
    Ok(())
}
