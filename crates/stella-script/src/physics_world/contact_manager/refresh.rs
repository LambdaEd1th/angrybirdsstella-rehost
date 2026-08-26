//! ContactManager::Collide traversal, Contact::Update and listener records.

use crate::SceneObject;

mod events;
mod traversal;
mod update;

#[derive(Debug, Clone, Copy)]
struct NativeContactEventKinematics {
    first_mass: f64,
    first_velocity_x: f64,
    first_velocity_y: f64,
    second_mass: f64,
    second_velocity_x: f64,
    second_velocity_y: f64,
}

impl NativeContactEventKinematics {
    fn capture(first: &SceneObject, second: &SceneObject) -> Self {
        Self {
            first_mass: if first.dynamic_body {
                f64::from(first.body_mass)
            } else {
                0.0
            },
            first_velocity_x: first.velocity_x,
            first_velocity_y: first.velocity_y,
            second_mass: if second.dynamic_body {
                f64::from(second.body_mass)
            } else {
                0.0
            },
            second_velocity_x: second.velocity_x,
            second_velocity_y: second.velocity_y,
        }
    }
}
