//! Animation hierarchy, affine, skin, entity-query and draw stages.

mod affine;
mod hierarchy;
mod queries;
mod render;
mod skin;

pub(crate) use hierarchy::animation_definition_contains_entity;
pub(crate) use queries::{
    animation_entity_has_sprite, animation_entity_local_transform, animation_entity_world_affine,
    animation_entity_world_bounds,
};
pub(crate) use render::animation_render_commands;
pub(crate) use skin::animation_skin_alias_attachment;
#[cfg(test)]
pub(crate) use skin::animation_slot_attachment;
