//! Scene appearance, wrapper transform and draw bindings.

use std::sync::{Arc, Mutex};

use mlua::{Lua, MultiValue, Result as LuaResult, Table, Value};

use crate::*;

pub(super) fn install_transforms(
    lua: &Lua,
    animation_native: &Table,
    animation_runtime: Arc<Mutex<AnimationRuntime>>,
) -> LuaResult<()> {
    let runtime = Arc::clone(&animation_runtime);
    animation_native.set(
        "setTranslation",
        lua.create_function(move |_, (tag, x, y): (String, f64, f64)| {
            let mut runtime = runtime.lock().expect("animation runtime lock poisoned");
            // sub_1000145FC resolves an existing scene and writes two float
            // members. Unknown tags only produce the native warning.
            if let Some(transform) = runtime.transforms.get_mut(&tag) {
                transform.x = f64::from(x as f32);
                transform.y = f64::from(y as f32);
            }
            if let Some(matrix) = runtime.matrices.get_mut(&tag) {
                matrix.set_translation(x, y);
            }
            Ok(())
        })?,
    )?;
    let runtime = Arc::clone(&animation_runtime);
    animation_native.set(
        "setRotation",
        lua.create_function(move |_, (tag, angle): (String, f64)| {
            let mut runtime = runtime.lock().expect("animation runtime lock poisoned");
            if let Some(transform) = runtime.transforms.get_mut(&tag) {
                transform.angle = f64::from(angle as f32);
                transform.scale_x = 1.0;
                transform.scale_y = 1.0;
            }
            if let Some(matrix) = runtime.matrices.get_mut(&tag) {
                matrix.set_rotation(angle);
            }
            Ok(())
        })?,
    )?;
    let runtime = Arc::clone(&animation_runtime);
    animation_native.set(
        "setScale",
        lua.create_function(move |_, (tag, scale_x, scale_y): (String, f64, f64)| {
            let mut runtime = runtime.lock().expect("animation runtime lock poisoned");
            if let Some(transform) = runtime.transforms.get_mut(&tag) {
                transform.scale_x = f64::from(scale_x as f32);
                transform.scale_y = f64::from(scale_y as f32);
            }
            if let Some(matrix) = runtime.matrices.get_mut(&tag) {
                matrix.set_scale(scale_x, scale_y);
            }
            if runtime.definitions.contains_key(&tag) {
                // sub_100014978 rounds the product in float32, propagates its
                // sign bit to every descendant through sub_10043CAAC, then
                // resets only the wrapper scene root to false. Rotation and
                // translation setters deliberately leave the descendants'
                // retained flag untouched.
                runtime
                    .descendant_reflections
                    .insert(tag, ((scale_x as f32) * (scale_y as f32)) < 0.0_f32);
            }
            Ok(())
        })?,
    )?;
    Ok(())
}

pub(super) fn install_draw(
    lua: &Lua,
    animation_native: &Table,
    animation_runtime: Arc<Mutex<AnimationRuntime>>,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    let runtime = Arc::clone(&animation_runtime);
    let animation_draw_bridge = Arc::clone(&render);
    animation_native.set(
        "draw",
        lua.create_function(move |_, tag: String| {
            let mut commands = {
                let runtime = runtime.lock().expect("animation runtime lock poisoned");
                animation_render_commands(&runtime, &tag)
            };
            let mut bridge = animation_draw_bridge
                .lock()
                .expect("render bridge lock poisoned");
            let clip_rect = bridge.state.clip_rect;
            for command in &mut commands {
                command.state.clip_rect = clip_rect;
            }
            bridge.extend_render_commands(commands);
            Ok(())
        })?,
    )?;
    Ok(())
}

pub(super) fn install_skin(
    lua: &Lua,
    animation_native: &Table,
    animation_runtime: Arc<Mutex<AnimationRuntime>>,
) -> LuaResult<()> {
    let runtime = Arc::clone(&animation_runtime);
    animation_native.set(
        "setSkin",
        lua.create_function(move |_, (tag, skin): (String, String)| {
            let mut runtime = runtime.lock().expect("animation runtime lock poisoned");
            let supported = runtime
                .definitions
                .get(&tag)
                .is_some_and(|definition| definition.skins.contains_key(&skin));
            if std::env::var_os("STELLA_TRACE_ANIMATION").is_some() {
                eprintln!("animation-native set-skin tag={tag} skin={skin} supported={supported}");
            }
            if supported {
                runtime.skins.insert(tag, skin);
            } else if runtime.definitions.contains_key(&tag) {
                // AnimationSkins::setSkin logs a missing name and clears its
                // current-skin pointer. Existing SpriteComponents keep their
                // concrete bindings until a later EntityTarget application;
                // that application then falls through to the default skin.
                runtime.skins.remove(&tag);
            }
            // setSkin is registered through the void two-string adapter at
            // sub_10001D43C; support is observable only through later draws.
            Ok(())
        })?,
    )?;
    Ok(())
}

pub(super) fn install_shader(
    lua: &Lua,
    animation_native: &Table,
    animation_runtime: Arc<Mutex<AnimationRuntime>>,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
) -> LuaResult<()> {
    let runtime = Arc::clone(&animation_runtime);
    animation_native.set(
        "setShader",
        lua.create_function(move |_, args: MultiValue| {
            let Some(tag) = args.front().and_then(value_string) else {
                return Ok(());
            };
            let shader = if args.len() == 2 {
                match args.get(1) {
                    Some(Value::Table(table)) => {
                        let mut resources = resource_runtime
                            .lock()
                            .expect("resource runtime lock poisoned");
                        Some(sprite_shader_from_lua(
                            table.clone(),
                            &mut resources.shader_cache,
                        )?)
                    }
                    _ => None,
                }
            } else {
                None
            };
            let mut runtime = runtime.lock().expect("animation runtime lock poisoned");
            // AnimationWrapper::setShader at 0x100013F44 only mutates a scene
            // returned by findScene(tag); unknown tags merely emit a warning.
            if !runtime.definitions.contains_key(&tag) {
                return Ok(());
            }
            if std::env::var_os("STELLA_TRACE_ANIMATION").is_some() {
                eprintln!("animation-native set-shader tag={tag} shader={shader:?}");
            }
            if let Some(shader) = shader {
                runtime.shaders.insert(tag, shader);
            } else {
                runtime.shaders.remove(&tag);
            }
            Ok(())
        })?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_scale_reflection_survives_rotation_until_the_next_scale_call() {
        let lua = Lua::new();
        let native = lua.create_table().unwrap();
        let runtime = Arc::new(Mutex::new(AnimationRuntime::default()));
        {
            let mut state = runtime.lock().unwrap();
            state
                .definitions
                .insert("scene".to_owned(), AnimationDefinition::default());
            state
                .transforms
                .insert("scene".to_owned(), AnimationTransform::default());
            state
                .matrices
                .insert("scene".to_owned(), AnimationAffine::default());
        }
        install_transforms(&lua, &native, Arc::clone(&runtime)).unwrap();
        let set_scale = native.get::<mlua::Function>("setScale").unwrap();
        let set_rotation = native.get::<mlua::Function>("setRotation").unwrap();

        set_scale.call::<()>(("scene", -1.0, 1.0)).unwrap();
        assert!(runtime.lock().unwrap().descendant_reflections["scene"]);
        set_rotation.call::<()>(("scene", 0.375)).unwrap();
        assert!(runtime.lock().unwrap().descendant_reflections["scene"]);
        set_scale.call::<()>(("scene", 2.0, 3.0)).unwrap();
        assert!(!runtime.lock().unwrap().descendant_reflections["scene"]);
    }
}
