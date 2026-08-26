//! PhysicsWorld scene-object constructors, split at the native binding layers.

mod adapters;
mod object;
mod shape;

use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Mutex},
};

use mlua::{Lua, Result as LuaResult, Table};

use crate::{
    BoundCompositePart, CollisionShape, DrawCallbacks, RenderBridge, ResourceRuntime,
    SpriteCatalogRegion,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConstructorKind {
    Box,
    Circle,
    Polygon,
    Line,
    NonPhysics,
}

impl ConstructorKind {
    const ALL: [Self; 5] = [
        Self::Box,
        Self::Circle,
        Self::Polygon,
        Self::Line,
        Self::NonPhysics,
    ];

    const fn script_name(self) -> &'static str {
        match self {
            Self::Box => "createBox",
            Self::Circle => "createCircle",
            Self::Polygon => "createPolygon",
            Self::Line => "createLineShape",
            Self::NonPhysics => "createNonPhysicsObject",
        }
    }

    const fn has_body(self) -> bool {
        !matches!(self, Self::NonPhysics)
    }

    const fn consumes_vertex_buffer(self) -> bool {
        matches!(self, Self::Polygon | Self::Line)
    }
}

struct ConstructorRequest {
    kind: ConstructorKind,
    name: String,
    sprite: String,
    x: f64,
    y: f64,
    shape_width: f64,
    shape_height: f64,
    shape_radius: f64,
    density: f64,
    friction: f64,
    restitution: f64,
    collision_enabled: bool,
    controllable: bool,
    z_order: f64,
}

struct PreparedConstruction {
    request: ConstructorRequest,
    collision_shape: CollisionShape,
    native_shape_width: f64,
    native_shape_height: f64,
    native_shape_radius: f64,
    dynamic_body: bool,
    mass: f64,
    sprite_bound: bool,
    sprite_region: Option<SpriteCatalogRegion>,
    composite_sprite: Option<Vec<BoundCompositePart>>,
}

pub(super) fn install(
    lua: &Lua,
    globals: &Table,
    render: Arc<Mutex<RenderBridge>>,
    resources: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<std::path::PathBuf>,
    draw_callbacks: Rc<RefCell<DrawCallbacks>>,
) -> LuaResult<()> {
    adapters::install(lua, globals, render, resources, data_root, draw_callbacks)
}
