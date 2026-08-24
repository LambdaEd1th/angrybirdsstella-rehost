//! Intrusive fixture-list behavior around CreateFixture/DestroyFixture.

use crate::*;

impl SceneObject {
    pub(crate) fn fixture_restitution(&self, fixture: usize) -> f64 {
        self.fixture_restitutions
            .get(fixture)
            .copied()
            .unwrap_or(self.restitution)
    }

    pub(crate) fn fixture_friction(&self, fixture: usize) -> f64 {
        self.fixture_frictions
            .get(fixture)
            .copied()
            .unwrap_or(self.friction)
    }

    pub(crate) fn fixture_density(&self, fixture: usize) -> f64 {
        self.fixture_densities
            .get(fixture)
            .copied()
            .unwrap_or(self.density)
    }

    /// Unlink `b2Body::m_fixtureList`'s head while retaining its proxy id for
    /// the later DestroyProxies step. Fixture vectors use creation order, so
    /// the native intrusive-list head is always their last element.
    pub(crate) fn unlink_head_fixture(&mut self) -> Option<(usize, Option<i32>)> {
        let fixture = self.collision_shape.fixture_count().checked_sub(1)?;
        match &mut self.collision_shape {
            CollisionShape::None => return None,
            CollisionShape::Box { .. } | CollisionShape::Circle { .. } => {
                self.collision_shape = CollisionShape::None;
            }
            CollisionShape::Polygon { vertices, fixtures } => {
                if fixtures.is_empty() {
                    vertices.clear();
                } else {
                    fixtures.pop();
                    // `vertices` is retained source-contour metadata, not a
                    // second fixture. Clear it when the explicit fixture list
                    // becomes empty so fixture_count mirrors m_fixtureCount.
                    if fixtures.is_empty() {
                        vertices.clear();
                    }
                }
            }
            CollisionShape::Line { vertices } => {
                vertices.pop();
                if vertices.len() < 2 {
                    vertices.clear();
                }
            }
        }
        self.fixture_densities.pop();
        self.fixture_frictions.pop();
        self.fixture_restitutions.pop();
        let proxy_id = self.fixture_proxy_ids.pop().flatten();
        Some((fixture, proxy_id))
    }

    /// Append one triangle using DirtMechanics' retained b2FixtureDef. Shape
    /// and per-fixture coefficient arrays remain in native creation order.
    pub(crate) fn append_dirt_fixture(
        &mut self,
        vertices: Vec<(f64, f64)>,
        density: f64,
        friction: f64,
        restitution: f64,
    ) -> usize {
        if !matches!(self.collision_shape, CollisionShape::Polygon { .. }) {
            self.collision_shape = CollisionShape::Polygon {
                vertices: Vec::new(),
                fixtures: Vec::new(),
            };
        }
        let CollisionShape::Polygon { fixtures, .. } = &mut self.collision_shape else {
            unreachable!();
        };
        let fixture = fixtures.len();
        fixtures.push(vertices);
        self.fixture_densities.push(density);
        self.fixture_frictions.push(friction);
        self.fixture_restitutions.push(restitution);
        self.fixture_proxy_ids.push(None);
        // These aggregate fields model the properties observed through the
        // current list head. Every Dirt triangle shares the captured def.
        self.density = density;
        self.friction = friction;
        self.restitution = restitution;
        self.sensor = false;
        fixture
    }
}
