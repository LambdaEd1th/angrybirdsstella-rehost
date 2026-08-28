//! Edge narrow-phase facade matching Purple's two collision leaves.

mod circle;
mod polygon;

#[cfg(test)]
pub(crate) use circle::circle_segment_manifold;
pub(crate) use circle::circle_segment_manifold_at_transforms;
#[cfg(test)]
pub(crate) use polygon::polygon_segment_manifold;
pub(crate) use polygon::polygon_segment_manifold_at_transforms;
