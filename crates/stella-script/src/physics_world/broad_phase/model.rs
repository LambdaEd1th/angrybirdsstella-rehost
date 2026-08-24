//! AABB primitives and Purple's logical 40-byte dynamic-tree node model.

pub(crate) type NativeAabb = (f32, f32, f32, f32);

pub(crate) fn native_aabb_combine(first: NativeAabb, second: NativeAabb) -> NativeAabb {
    (
        first.0.min(second.0),
        first.1.min(second.1),
        first.2.max(second.2),
        first.3.max(second.3),
    )
}

pub(crate) fn native_aabb_perimeter(aabb: NativeAabb) -> f32 {
    2.0_f32 * ((aabb.2 - aabb.0) + (aabb.3 - aabb.1))
}

pub(crate) fn native_aabb_overlaps(first: NativeAabb, second: NativeAabb) -> bool {
    first.0 <= second.2 && second.0 <= first.2 && first.1 <= second.3 && second.1 <= first.3
}

pub(crate) fn native_aabb_contains(container: NativeAabb, candidate: NativeAabb) -> bool {
    container.0 <= candidate.0
        && container.1 <= candidate.1
        && candidate.2 <= container.2
        && candidate.3 <= container.3
}

/// Exact 40-byte logical node represented by Purple's embedded
/// `b2DynamicTree`. The native union at offset 24 is split into `parent` and
/// `next` here; only one is live according to `height == -1`.
#[derive(Debug, Clone)]
pub(crate) struct NativeDynamicTreeNode {
    pub(crate) aabb: NativeAabb,
    pub(crate) user_data: Option<(String, usize)>,
    pub(crate) parent: i32,
    pub(crate) next: i32,
    pub(crate) child1: i32,
    pub(crate) child2: i32,
    pub(crate) height: i32,
}

impl NativeDynamicTreeNode {
    pub(crate) fn free(next: i32) -> Self {
        Self {
            aabb: (0.0, 0.0, 0.0, 0.0),
            user_data: None,
            parent: -1,
            next,
            child1: -1,
            child2: -1,
            height: -1,
        }
    }

    pub(crate) fn is_leaf(&self) -> bool {
        self.child1 == -1
    }
}

/// Purple ships the stock height-balanced Box2D dynamic AABB tree recovered
/// at `sub_100860E30` through `sub_100861B50`. Keeping the actual internal
/// nodes is important not only for leaf ids but also for LIFO query traversal.
#[derive(Debug)]
pub(crate) struct NativeDynamicTree {
    pub(crate) root: i32,
    pub(crate) nodes: Vec<NativeDynamicTreeNode>,
    pub(crate) node_count: usize,
    pub(crate) free_list: i32,
    pub(crate) insertion_count: u32,
}

impl Default for NativeDynamicTree {
    fn default() -> Self {
        let capacity = 16;
        let mut nodes = Vec::with_capacity(capacity);
        for index in 0..capacity {
            nodes.push(NativeDynamicTreeNode::free(if index + 1 < capacity {
                (index + 1) as i32
            } else {
                -1
            }));
        }
        Self {
            root: -1,
            nodes,
            node_count: 0,
            free_list: 0,
            insertion_count: 0,
        }
    }
}
