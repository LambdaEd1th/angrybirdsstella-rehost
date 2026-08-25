//! `drawSprite` and forwarding `drawCompoSprite` native members.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    resource_api: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    for method in ["drawSprite", "drawCompoSprite"] {
        let draw_bridge = Arc::clone(&render);
        let draw_resources = Arc::clone(&resource_runtime);
        let draw_data_root = Arc::clone(&data_root);
        resource_api.set(
            method,
            lua.create_function(move |_, args: MultiValue| {
                if let Some(draw) = parse_draw_sprite_args(&args)? {
                    let resources = draw_resources
                        .lock()
                        .expect("resource runtime lock poisoned");
                    let mut bridge = draw_bridge.lock().expect("render bridge lock poisoned");
                    if let Some(command) = native_resource_sprite_command(
                        &resources,
                        &draw_data_root,
                        draw,
                        bridge.state,
                    ) {
                        bridge.push_render_command(command);
                    }
                }
                Ok(())
            })?,
        )?;
    }
    Ok(())
}

/// Prepare the deferred equivalent of `ResourceManager::drawSprite` /
/// `drawCompoSprite` (`sub_10045C0AC`) using the live GL-context state.
/// Keeping this path shared is important for native helpers that call the
/// ResourceManager directly instead of re-entering Lua.
pub(crate) fn native_resource_sprite_command(
    resources: &ResourceRuntime,
    data_root: &Path,
    draw: ParsedSpriteDraw,
    mut state: RenderState,
) -> Option<RenderCommand> {
    let geometry = resources.active_geometry(&draw.sprite)?;
    // Purple's immediate call already owns the AtlasSprite's image pointer
    // here. Keep that exact resolved region on the deferred wgpu command even
    // if Lua releases or shadows the sheet later in the same frame.
    let bound_region = resources.active_atlas_catalog_region(&draw.sprite, data_root);
    let bound_composite = resources.active_bound_composite(&draw.sprite);
    let (anchor_x, anchor_y) = sprite_draw_anchor_offset_from_geometry(
        geometry,
        draw.horizontal_anchor,
        draw.vertical_anchor,
    );
    let is_atlas_sprite = bound_region.is_some();
    // AtlasSprite::draw first converts the requested anchor to the raw
    // rectangle origin. For HPIVOT/VPIVOT that means x - atlasPivotX and
    // y - atlasPivotY. GL_Context then rotates that raw rectangle around the
    // independent live render-state pivot.
    //
    // The deferred renderer normally stores atlas vertices relative to their
    // SPRT pivot. Keeping that implicit subtraction here as well would apply
    // (I - R) * pivot a second time. Preserve the native split by submitting
    // the raw rectangle and moving the one atlas-pivot subtraction into x/y.
    let (x, y) = if is_atlas_sprite {
        state.sprite_pivot = Some([0.0, 0.0]);
        (
            draw.x + anchor_x + geometry.min_x,
            draw.y + anchor_y + geometry.min_y,
        )
    } else {
        (draw.x + anchor_x, draw.y + anchor_y)
    };
    state.draw_size = draw.draw_size;
    Some(RenderCommand {
        order: 0,
        sprite: draw.sprite,
        texture: None,
        texture_scale: 1.0,
        masked_texture_binding: None,
        bound_region,
        bound_composite,
        shader: None,
        clip_holes: Vec::new(),
        dirt: None,
        x,
        y,
        state,
        world_space: false,
    })
}
