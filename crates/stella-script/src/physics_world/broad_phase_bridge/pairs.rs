//! `b2BroadPhase::UpdatePairs<b2ContactManager>` at `0x10086BDB0`.

use crate::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum NativeContactShapeType {
    Circle,
    Edge,
    Polygon,
}

impl NativeContactShapeType {
    fn of(shape: &CollisionShape) -> Option<Self> {
        match shape {
            CollisionShape::None => None,
            CollisionShape::Circle { .. } => Some(Self::Circle),
            CollisionShape::Line { .. } => Some(Self::Edge),
            CollisionShape::Box { .. } | CollisionShape::Polygon { .. } => Some(Self::Polygon),
        }
    }
}

impl RenderBridge {
    /// Apply Purple's `b2ContactFactory::Create` fixture ordering after the
    /// broad phase has sorted a pair by proxy id.  The factory registers only
    /// one primary direction for unlike shapes and calls that constructor
    /// with swapped arguments from the mirrored table entry.  Edge/edge has
    /// no registration at all.
    fn native_contact_factory_key(
        &self,
        first_name: String,
        first_fixture: usize,
        second_name: String,
        second_fixture: usize,
    ) -> Option<ContactKey> {
        let first_type = NativeContactShapeType::of(&self.scene.get(&first_name)?.collision_shape)?;
        let second_type =
            NativeContactShapeType::of(&self.scene.get(&second_name)?.collision_shape)?;
        let swap = match (first_type, second_type) {
            (NativeContactShapeType::Circle, NativeContactShapeType::Circle)
            | (NativeContactShapeType::Polygon, NativeContactShapeType::Polygon)
            | (NativeContactShapeType::Edge, NativeContactShapeType::Circle)
            | (NativeContactShapeType::Polygon, NativeContactShapeType::Circle)
            | (NativeContactShapeType::Edge, NativeContactShapeType::Polygon) => false,
            (NativeContactShapeType::Circle, NativeContactShapeType::Edge)
            | (NativeContactShapeType::Circle, NativeContactShapeType::Polygon)
            | (NativeContactShapeType::Polygon, NativeContactShapeType::Edge) => true,
            (NativeContactShapeType::Edge, NativeContactShapeType::Edge) => return None,
        };
        Some(if swap {
            (second_name, first_name, second_fixture, first_fixture)
        } else {
            (first_name, second_name, first_fixture, second_fixture)
        })
    }

    pub(crate) fn find_new_broad_phase_contacts(&mut self) {
        let moved = std::mem::take(&mut self.moved_proxy_ids);
        let mut pairs = BTreeMap::new();
        for proxy_id in moved {
            let Some(fat) = self.dynamic_tree.proxy_aabb(proxy_id) else {
                continue;
            };
            let Some((name, fixture)) = self.dynamic_tree.proxy_user_data(proxy_id).cloned() else {
                continue;
            };
            for other_proxy in self.dynamic_tree.query(fat) {
                if proxy_id == other_proxy {
                    continue;
                }
                let Some((other_name, other_fixture)) =
                    self.dynamic_tree.proxy_user_data(other_proxy).cloned()
                else {
                    continue;
                };
                if name == other_name {
                    continue;
                }
                let (proxy_pair, proxy_ordered) = if proxy_id <= other_proxy {
                    (
                        (proxy_id, other_proxy),
                        (name.clone(), fixture, other_name, other_fixture),
                    )
                } else {
                    (
                        (other_proxy, proxy_id),
                        (other_name, other_fixture, name.clone(), fixture),
                    )
                };
                let Some(contact_key) = self.native_contact_factory_key(
                    proxy_ordered.0,
                    proxy_ordered.1,
                    proxy_ordered.2,
                    proxy_ordered.3,
                ) else {
                    continue;
                };
                pairs.entry(proxy_pair).or_insert(contact_key);
            }
        }
        for (_, key) in pairs {
            let allowed = self
                .scene
                .get(&key.0)
                .zip(self.scene.get(&key.1))
                .is_some_and(|(first, second)| {
                    (first.dynamic_body || second.dynamic_body)
                        && Self::native_objects_should_collide(first, second)
                        && !self.joints.values().any(|joint| {
                            joint.is_physical
                                && !joint.collide_connected
                                && ((joint.first == key.0 && joint.second == key.1)
                                    || (joint.first == key.1 && joint.second == key.0))
                        })
                });
            if allowed && self.broad_phase_contacts.insert(key.clone()) {
                let order = self.allocate_physics_creation_order();
                self.insert_native_contact_order(key.clone(), order);
                // AddPair links the new contact first, then performs the
                // inlined SetAwake(true) sequence on both endpoint bodies.
                // This applies to sensor contacts too; Contact::Update's
                // later sensor transition itself still does not wake them.
                for name in [&key.0, &key.1] {
                    if let Some(object) = self.scene.get_mut(name)
                        && object.sleeping
                    {
                        object.wake();
                    }
                }
            }
        }
        let stale = self
            .contact_creation_order
            .keys()
            .filter(|key| {
                !self.broad_phase_contacts.contains(*key)
                    && !self.active_contacts.contains_key(*key)
            })
            .cloned()
            .collect::<Vec<_>>();
        for key in stale {
            self.remove_native_contact_order(&key);
        }
    }
}
