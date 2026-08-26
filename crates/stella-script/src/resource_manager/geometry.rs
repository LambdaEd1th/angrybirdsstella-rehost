//! Sprite and composite geometry owned by Purple's `game::LuaResources`.

#[cfg(test)]
mod assets;
mod composite;
mod draw;
mod lines;
mod model;
mod pivot;

#[cfg(test)]
pub(crate) use assets::load_sprite_geometry;
pub(crate) use composite::{composite_part_lua_table, update_composite_part_from_lua};
pub(crate) use draw::{
    ParsedSpriteDraw, SpriteHorizontalAnchor, SpriteVerticalAnchor, parse_draw_sprite_args,
    sprite_draw_anchor_offset_from_geometry,
};
pub(crate) use model::{NativeSpriteMetrics, NativeSpritePlacement, SpriteGeometry};
pub(crate) use pivot::{native_composite_metrics, native_composite_metrics_from_parts};
