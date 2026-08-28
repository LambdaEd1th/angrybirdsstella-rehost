//! Per-member-function shape preparation before the shared object tail.
//!
//! The branches correspond to sub_100034740, sub_100034FB0, sub_1000357A4,
//! sub_1000364E0 and sub_100036D38. Polygon and line creators consume the
//! native staging buffer populated by the separate vertex bindings.

use std::sync::{Arc, Mutex};

use crate::{CollisionShape, RenderBridge, native_polygon_fixtures};

use super::{ConstructorKind, ConstructorRequest, PreparedConstruction};

pub(super) fn prepare(
    render: &Arc<Mutex<RenderBridge>>,
    request: ConstructorRequest,
) -> PreparedConstruction {
    let buffered_vertices = if request.kind.consumes_vertex_buffer() {
        render
            .lock()
            .expect("render bridge lock poisoned")
            .vertex_buffer
            .clone()
    } else {
        Vec::new()
    };
    trace_vertices(&request, &buffered_vertices);

    let collision_shape = match request.kind {
        ConstructorKind::Circle => CollisionShape::Circle {
            radius: request.shape_radius,
        },
        ConstructorKind::Box => CollisionShape::Box {
            width: request.shape_width,
            height: request.shape_height,
        },
        ConstructorKind::Polygon => {
            let (vertices, fixtures) = native_polygon_fixtures(&buffered_vertices);
            if std::env::var_os("STELLA_TRACE_SHAPES").is_some() {
                eprintln!(
                    "native-shape-fixtures createPolygon {:?} fixtures={fixtures:?}",
                    request.name
                );
            }
            CollisionShape::Polygon { vertices, fixtures }
        }
        ConstructorKind::Line => CollisionShape::Line {
            vertices: buffered_vertices,
        },
        ConstructorKind::NonPhysics => CollisionShape::None,
    };

    let native_shape_width = match &collision_shape {
        CollisionShape::Box { width, .. } => *width,
        CollisionShape::Polygon { .. } | CollisionShape::Line { .. } => request.shape_width,
        CollisionShape::Circle { .. } | CollisionShape::None => 0.0,
    };
    let native_shape_height = match &collision_shape {
        CollisionShape::Box { height, .. } => *height,
        CollisionShape::Polygon { .. } | CollisionShape::Line { .. } => request.shape_height,
        CollisionShape::Circle { .. } | CollisionShape::None => 0.0,
    };
    let native_shape_radius = match &collision_shape {
        CollisionShape::Circle { radius } => *radius,
        _ => 0.0,
    };
    let dynamic_body =
        request.kind.has_body() && request.density != 0.0 && request.density != 100.0;
    let mass = native_initial_mass(&collision_shape, dynamic_body, request.density);

    PreparedConstruction {
        request,
        collision_shape,
        native_shape_width,
        native_shape_height,
        native_shape_radius,
        dynamic_body,
        mass,
        sprite_bound: false,
        sprite_region: None,
        composite_sprite: None,
    }
}

fn native_initial_mass(shape: &CollisionShape, dynamic_body: bool, density: f64) -> f64 {
    if !dynamic_body {
        return 0.0;
    }
    let density = density as f32;
    let mass = match shape {
        CollisionShape::None | CollisionShape::Line { .. } => 0.0_f32,
        CollisionShape::Box { width, height } => {
            let half_width = (*width as f32) * 0.5_f32;
            let half_height = (*height as f32) * 0.5_f32;
            native_polygon_mass_f32(
                &[
                    (f64::from(-half_width), f64::from(-half_height)),
                    (f64::from(half_width), f64::from(-half_height)),
                    (f64::from(half_width), f64::from(half_height)),
                    (f64::from(-half_width), f64::from(half_height)),
                ],
                density,
            )
        }
        CollisionShape::Circle { radius } => {
            let radius = radius.abs() as f32;
            let radius_squared = radius * radius;
            density * std::f32::consts::PI * radius_squared
        }
        CollisionShape::Polygon { fixtures, .. } => {
            fixtures.iter().rev().fold(0.0_f32, |mass, fixture| {
                mass + native_polygon_mass_f32(fixture, density)
            })
        }
    };
    // b2Body::ResetMassData assigns unit mass to a dynamic body whose
    // aggregate fixture mass is not positive.
    if mass > 0.0_f32 { f64::from(mass) } else { 1.0 }
}

fn native_polygon_mass_f32(vertices: &[(f64, f64)], density: f32) -> f32 {
    if density == 0.0_f32 || vertices.len() < 3 {
        return 0.0;
    }
    let mut reference = (0.0_f32, 0.0_f32);
    for &(x, y) in vertices {
        reference.0 += x as f32;
        reference.1 += y as f32;
    }
    let inverse_count = 1.0_f32 / vertices.len() as f32;
    reference.0 *= inverse_count;
    reference.1 *= inverse_count;
    let mut area = 0.0_f32;
    for index in 0..vertices.len() {
        let first = (
            vertices[index].0 as f32 - reference.0,
            vertices[index].1 as f32 - reference.1,
        );
        let second_vertex = vertices[(index + 1) % vertices.len()];
        let second = (
            second_vertex.0 as f32 - reference.0,
            second_vertex.1 as f32 - reference.1,
        );
        let cross = (-first.1).mul_add(second.0, first.0 * second.1);
        area = cross.mul_add(0.5_f32, area);
    }
    density * area
}

fn trace_vertices(request: &ConstructorRequest, vertices: &[(f64, f64)]) {
    if std::env::var_os("STELLA_TRACE_SHAPES").is_some() && request.kind.consumes_vertex_buffer() {
        eprintln!(
            "native-shape {} {:?} vertices={vertices:?}",
            request.kind.script_name(),
            request.name
        );
    }
}
