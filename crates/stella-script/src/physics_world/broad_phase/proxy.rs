//! Fat-proxy creation, destruction, movement and stack-ordered queries.

use super::{NativeAabb, NativeDynamicTree, native_aabb_contains, native_aabb_overlaps};

impl NativeDynamicTree {
    pub(crate) fn create_proxy(&mut self, tight: NativeAabb, user_data: (String, usize)) -> i32 {
        let proxy_id = self.allocate_node();
        self.nodes[proxy_id as usize].aabb = (
            tight.0 - 0.1_f32,
            tight.1 - 0.1_f32,
            tight.2 + 0.1_f32,
            tight.3 + 0.1_f32,
        );
        self.nodes[proxy_id as usize].user_data = Some(user_data);
        self.nodes[proxy_id as usize].height = 0;
        self.insert_leaf(proxy_id);
        proxy_id
    }

    pub(crate) fn destroy_proxy(&mut self, proxy_id: i32) {
        if proxy_id < 0
            || self
                .nodes
                .get(proxy_id as usize)
                .is_none_or(|node| node.height < 0)
        {
            return;
        }
        self.remove_leaf(proxy_id);
        self.free_node(proxy_id);
    }

    pub(crate) fn move_proxy(
        &mut self,
        proxy_id: i32,
        swept: NativeAabb,
        displacement: (f32, f32),
    ) -> bool {
        if native_aabb_contains(self.nodes[proxy_id as usize].aabb, swept) {
            return false;
        }
        self.remove_leaf(proxy_id);
        let mut fat = (
            swept.0 - 0.1_f32,
            swept.1 - 0.1_f32,
            swept.2 + 0.1_f32,
            swept.3 + 0.1_f32,
        );
        let extension_x = displacement.0 + displacement.0;
        let extension_y = displacement.1 + displacement.1;
        if extension_x < 0.0 {
            fat.0 += extension_x;
        } else {
            fat.2 += extension_x;
        }
        if extension_y < 0.0 {
            fat.1 += extension_y;
        } else {
            fat.3 += extension_y;
        }
        self.nodes[proxy_id as usize].aabb = fat;
        self.insert_leaf(proxy_id);
        true
    }

    pub(crate) fn proxy_aabb(&self, proxy_id: i32) -> Option<NativeAabb> {
        self.nodes
            .get(proxy_id as usize)
            .filter(|node| node.height >= 0 && node.is_leaf())
            .map(|node| node.aabb)
    }

    pub(crate) fn proxy_user_data(&self, proxy_id: i32) -> Option<&(String, usize)> {
        self.nodes
            .get(proxy_id as usize)
            .and_then(|node| node.user_data.as_ref())
    }

    pub(crate) fn query(&self, query: NativeAabb) -> Vec<i32> {
        let mut leaves = Vec::new();
        let mut stack = vec![self.root];
        while let Some(node_id) = stack.pop() {
            if node_id == -1 {
                continue;
            }
            let node = &self.nodes[node_id as usize];
            if !native_aabb_overlaps(node.aabb, query) {
                continue;
            }
            if node.is_leaf() {
                leaves.push(node_id);
            } else {
                // `b2DynamicTree::Query<b2BroadPhase>` pushes child1 then
                // child2; its LIFO stack consequently visits child2 first.
                stack.push(node.child1);
                stack.push(node.child2);
            }
        }
        leaves
    }

    /// Candidate traversal from Purple's instantiated
    /// `b2DynamicTree::RayCast<b2WorldRayCastWrapper>` (`sub_10086F834`). The
    /// public world callback always returns `1.0f`, so the native segment AABB
    /// never shortens while this stack walk is in progress.
    pub(crate) fn ray_cast_candidates(
        &self,
        start: (f32, f32),
        end: (f32, f32),
        max_fraction: f32,
    ) -> Vec<i32> {
        let direction = (end.0 - start.0, end.1 - start.1);
        let length = direction
            .0
            .mul_add(direction.0, direction.1 * direction.1)
            .sqrt();
        let mut unit = direction;
        if length >= f32::EPSILON {
            let inverse_length = 1.0_f32 / length;
            unit.0 *= inverse_length;
            unit.1 *= inverse_length;
        }
        let absolute = (unit.0.abs(), unit.1.abs());
        let perpendicular = (-unit.1, unit.0);
        let segment_end = (
            direction.0.mul_add(max_fraction, start.0),
            direction.1.mul_add(max_fraction, start.1),
        );
        let segment_aabb = (
            start.0.min(segment_end.0),
            start.1.min(segment_end.1),
            start.0.max(segment_end.0),
            start.1.max(segment_end.1),
        );

        let mut leaves = Vec::new();
        let mut stack = vec![self.root];
        while let Some(node_id) = stack.pop() {
            if node_id == -1 {
                continue;
            }
            let node = &self.nodes[node_id as usize];
            if segment_aabb.0 - node.aabb.2 > 0.0_f32
                || segment_aabb.1 - node.aabb.3 > 0.0_f32
                || node.aabb.0 - segment_aabb.2 > 0.0_f32
                || node.aabb.1 - segment_aabb.3 > 0.0_f32
            {
                continue;
            }

            let center = (
                (-(node.aabb.2 + node.aabb.0)).mul_add(0.5_f32, start.0),
                (-(node.aabb.3 + node.aabb.1)).mul_add(0.5_f32, start.1),
            );
            let extents = (
                (node.aabb.2 - node.aabb.0) * 0.5_f32,
                (node.aabb.3 - node.aabb.1) * 0.5_f32,
            );
            let mut separation = center.0.mul_add(perpendicular.0, unit.0 * center.1);
            // The native FCMP/B.GT pair leaves a positive value alone and
            // negates zero, negative and unordered values.
            if separation <= 0.0_f32 || separation.is_nan() {
                separation = -separation;
            }
            let radius = absolute.1.mul_add(extents.0, absolute.0 * extents.1);
            if separation - radius > 0.0_f32 {
                continue;
            }

            if node.is_leaf() {
                leaves.push(node_id);
            } else {
                // Purple pushes child1 followed by child2; its LIFO stack
                // visits child2 before child1.
                stack.push(node.child1);
                stack.push(node.child2);
            }
        }
        leaves
    }
}
