//! `createThemeSprite` record construction.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "createThemeSprite",
        lua.create_function(move |_, args: MultiValue| {
            // sub_100086928 consumes exactly: name, sprite, x, y, scaleX,
            // scaleY, angle, layer, scaleSpeed, horFlip, velX, velY.
            let name = theme_required_string(&args, 0, "createThemeSprite")?;
            let sprite = theme_required_string(&args, 1, "createThemeSprite")?;
            let x = theme_required_f32(&args, 2, "createThemeSprite")?;
            let y = theme_required_f32(&args, 3, "createThemeSprite")?;
            let scale_x = theme_required_f32(&args, 4, "createThemeSprite")?;
            let scale_y = theme_required_f32(&args, 5, "createThemeSprite")?;
            let angle = theme_required_f32(&args, 6, "createThemeSprite")?;
            let layer_index =
                native_theme_layer(theme_required_f32(&args, 7, "createThemeSprite")?);
            let scale_speed = theme_required_f32(&args, 8, "createThemeSprite")?;
            let horizontal_flip = theme_required_bool(&args, 9, "createThemeSprite")?;
            let velocity_x = theme_required_f32(&args, 10, "createThemeSprite")?;
            let velocity_y = theme_required_f32(&args, 11, "createThemeSprite")?;
            render
                .lock()
                .expect("render bridge lock poisoned")
                .theme_sprites
                .insert(
                    (layer_index, name),
                    NativeThemeSprite {
                        sprite,
                        x: f64::from(x),
                        y: f64::from(y),
                        scale_x: f64::from(scale_x),
                        scale_y: f64::from(scale_y),
                        angle: f64::from(angle),
                        angular_velocity: 0.0,
                        horizontal_flip,
                        velocity_x: f64::from(velocity_x),
                        velocity_y: f64::from(velocity_y),
                        scale_speed: f64::from(scale_speed),
                        original_x: f64::from(x),
                        original_y: f64::from(y),
                        animation_start_timer: 0.0,
                        is_animation: false,
                        animation_frames: Vec::new(),
                        animation_frame_time: 0.0,
                        animation_timer: 0.0,
                        animation_frame: 0,
                        animation_looping: false,
                    },
                );
            Ok(())
        })?,
    )
}
