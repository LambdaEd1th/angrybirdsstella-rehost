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
}
