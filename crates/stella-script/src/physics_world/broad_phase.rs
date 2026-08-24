//! Purple's embedded Box2D AABB helpers and dynamic broad-phase tree.

mod allocation;
mod balance;
mod insertion;
mod model;
mod proxy;
mod removal;

pub(crate) use model::{
    NativeAabb, NativeDynamicTree, NativeDynamicTreeNode, native_aabb_combine,
    native_aabb_contains, native_aabb_overlaps, native_aabb_perimeter,
};
