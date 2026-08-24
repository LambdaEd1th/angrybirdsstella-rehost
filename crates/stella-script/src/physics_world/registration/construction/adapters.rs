//! Lua-to-native constructor adapters.
//!
//! These mirror sub_100086F24/sub_1000872D0/sub_100087704 and their nested
//! strict argument readers sub_100086F8C/sub_100087338/sub_10008776C.

use std::sync::{Arc, Mutex};

use mlua::{Lua, MultiValue, Result as LuaResult, Table};

use crate::{
    RenderBridge, ResourceRuntime, describe_value, native_required_boolean, native_required_number,
    native_required_string,
};

use super::{ConstructorKind, ConstructorRequest, object, shape};

pub(super) fn install(
    lua: &Lua,
    globals: &Table,
    render: Arc<Mutex<RenderBridge>>,
    resources: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<std::path::PathBuf>,
) -> LuaResult<()> {
    // Keep the installer order from sub_10002C274 visible instead of merging
    // the five adapters into a string-driven permissive shim.
    for kind in ConstructorKind::ALL {
        let scene_bridge = Arc::clone(&render);
        let resources = Arc::clone(&resources);
        let data_root = Arc::clone(&data_root);
        globals.set(
            kind.script_name(),
            lua.create_function(move |lua, args: MultiValue| {
                trace_call(kind, &args);
                let request = parse_request(kind, &args)?;
                let (sprite_region, mut composite_sprite) = {
                    let resources = resources.lock().expect("resource runtime lock poisoned");
                    (
                        resources.active_atlas_catalog_region(&request.sprite, &data_root),
                        resources.active_bound_composite(&request.sprite),
                    )
                };
                if sprite_region.is_none() && composite_sprite.is_none() {
                    composite_sprite = Some(Vec::new());
                }
                let mut prepared = shape::prepare(&scene_bridge, request);
                prepared.sprite_bound = true;
                prepared.sprite_region = sprite_region;
                prepared.composite_sprite = composite_sprite;
                object::commit(lua, &scene_bridge, prepared)
            })?,
        )?;
    }

    Ok(())
}

fn parse_request(kind: ConstructorKind, args: &MultiValue) -> LuaResult<ConstructorRequest> {
    let creator = kind.script_name();
    let name = native_required_string(args, 0, creator)?;
    let sprite = native_required_string(args, 1, creator)?;
    let number = |index| -> LuaResult<f64> {
        // All adapter locals are float, so Lua doubles are narrowed before the
        // native member call and then widened only for the Rust scene model.
        Ok(f64::from(
            native_required_number(args, index, creator)? as f32
        ))
    };

    let (
        x,
        y,
        shape_width,
        shape_height,
        shape_radius,
        density,
        friction,
        restitution,
        collision_enabled,
        controllable,
        z_order,
    ) = match kind {
        ConstructorKind::Circle => (
            number(2)?,
            number(3)?,
            0.0,
            0.0,
            number(4)?,
            number(5)?,
            number(6)?,
            number(7)?,
            native_required_boolean(args, 8, creator)?,
            native_required_boolean(args, 9, creator)?,
            number(10)?,
        ),
        ConstructorKind::Box | ConstructorKind::Polygon | ConstructorKind::Line => (
            number(2)?,
            number(3)?,
            number(4)?,
            number(5)?,
            0.0,
            number(6)?,
            number(7)?,
            number(8)?,
            native_required_boolean(args, 9, creator)?,
            native_required_boolean(args, 10, creator)?,
            number(11)?,
        ),
        ConstructorKind::NonPhysics => (
            number(2)?,
            number(3)?,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            false,
            false,
            number(4)?,
        ),
    };

    Ok(ConstructorRequest {
        kind,
        name,
        sprite,
        x,
        y,
        shape_width,
        shape_height,
        shape_radius,
        density,
        friction,
        restitution,
        collision_enabled,
        controllable,
        z_order,
    })
}

fn trace_call(kind: ConstructorKind, args: &MultiValue) {
    if std::env::var_os("STELLA_TRACE_NATIVE").is_none() {
        return;
    }
    let rendered = args
        .iter()
        .map(describe_value)
        .collect::<Vec<_>>()
        .join(", ");
    eprintln!("native {}({rendered})", kind.script_name());
}
