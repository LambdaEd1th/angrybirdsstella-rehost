//! Sprite bounds and native placement data model.

use stella_assets::ka3d::CompositePart;

#[derive(Debug, Clone, Copy)]
pub(crate) struct SpriteGeometry {
    pub(crate) min_x: f64,
    pub(crate) min_y: f64,
    pub(crate) max_x: f64,
    pub(crate) max_y: f64,
}

/// Integer size/pivot fields returned by Purple's concrete `game::Sprite`.
/// AtlasSprite stores these directly as signed 16-bit values; CompoSprite
/// rebuilds them from transformed, FCVTZS-truncated part bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativeSpriteMetrics {
    pub(crate) width: i32,
    pub(crate) height: i32,
    pub(crate) pivot_x: i32,
    pub(crate) pivot_y: i32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct NativeSpritePlacement {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) scale_x: f64,
    pub(crate) scale_y: f64,
    pub(crate) angle: f64,
}

impl SpriteGeometry {
    pub(crate) fn width(self) -> f64 {
        self.max_x - self.min_x
    }

    pub(crate) fn height(self) -> f64 {
        self.max_y - self.min_y
    }

    pub(crate) fn include(&mut self, other: Self) {
        self.min_x = self.min_x.min(other.min_x);
        self.min_y = self.min_y.min(other.min_y);
        self.max_x = self.max_x.max(other.max_x);
        self.max_y = self.max_y.max(other.max_y);
    }

    pub(crate) fn transformed(self, part: &CompositePart) -> Self {
        let angle = f64::from(part.angle);
        let cosine = angle.cos();
        let sine = angle.sin();
        let transform = |x: f64, y: f64| {
            let x = x * f64::from(part.scale_x * part.flip_x);
            let y = y * f64::from(part.scale_y * part.flip_y);
            (
                f64::from(part.x) + x * cosine - y * sine,
                f64::from(part.y) + x * sine + y * cosine,
            )
        };
        let points = [
            transform(self.min_x, self.min_y),
            transform(self.max_x, self.min_y),
            transform(self.min_x, self.max_y),
            transform(self.max_x, self.max_y),
        ];
        Self {
            min_x: points
                .iter()
                .map(|point| point.0)
                .fold(f64::INFINITY, f64::min),
            min_y: points
                .iter()
                .map(|point| point.1)
                .fold(f64::INFINITY, f64::min),
            max_x: points
                .iter()
                .map(|point| point.0)
                .fold(f64::NEG_INFINITY, f64::max),
            max_y: points
                .iter()
                .map(|point| point.1)
                .fold(f64::NEG_INFINITY, f64::max),
        }
    }
}
