//! Scene-object collision shape storage shared by fixtures and queries.

/// Skin radius stored in every edge and polygon shape by Purple's Box2D build.
pub(crate) const BOX2D_POLYGON_RADIUS: f64 = 0.002;

#[derive(Debug, Clone)]
pub(crate) enum CollisionShape {
    None,
    Box {
        width: f64,
        height: f64,
    },
    Circle {
        radius: f64,
    },
    Polygon {
        vertices: Vec<(f64, f64)>,
        fixtures: Vec<Vec<(f64, f64)>>,
    },
    Line {
        vertices: Vec<(f64, f64)>,
    },
}

impl CollisionShape {
    pub(crate) fn fixture_count(&self) -> usize {
        match self {
            Self::None => 0,
            Self::Box { .. } | Self::Circle { .. } => 1,
            Self::Polygon { vertices, fixtures } => {
                fixtures.len().max(usize::from(!vertices.is_empty()))
            }
            Self::Line { vertices } => vertices.len().saturating_sub(1),
        }
    }
}
