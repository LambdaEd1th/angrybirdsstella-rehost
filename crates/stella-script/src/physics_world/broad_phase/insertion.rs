//! `b2DynamicTree::InsertLeaf` float32 perimeter-cost traversal and refit.

use super::{NativeDynamicTree, native_aabb_combine, native_aabb_perimeter};

impl NativeDynamicTree {
    pub(crate) fn insert_leaf(&mut self, leaf: i32) {
        self.insertion_count = self.insertion_count.wrapping_add(1);
        if self.root == -1 {
            self.root = leaf;
            self.nodes[leaf as usize].parent = -1;
            return;
        }

        let leaf_aabb = self.nodes[leaf as usize].aabb;
        let mut sibling = self.root;
        while !self.nodes[sibling as usize].is_leaf() {
            let child1 = self.nodes[sibling as usize].child1;
            let child2 = self.nodes[sibling as usize].child2;
            let area = native_aabb_perimeter(self.nodes[sibling as usize].aabb);
            let combined = native_aabb_combine(self.nodes[sibling as usize].aabb, leaf_aabb);
            let combined_area = native_aabb_perimeter(combined);
            let cost = 2.0_f32 * combined_area;
            let inheritance_cost = 2.0_f32 * (combined_area - area);
            let child1_combined = native_aabb_combine(leaf_aabb, self.nodes[child1 as usize].aabb);
            let cost1 = if self.nodes[child1 as usize].is_leaf() {
                native_aabb_perimeter(child1_combined) + inheritance_cost
            } else {
                native_aabb_perimeter(child1_combined)
                    - native_aabb_perimeter(self.nodes[child1 as usize].aabb)
                    + inheritance_cost
            };
            let child2_combined = native_aabb_combine(leaf_aabb, self.nodes[child2 as usize].aabb);
            let cost2 = if self.nodes[child2 as usize].is_leaf() {
                native_aabb_perimeter(child2_combined) + inheritance_cost
            } else {
                native_aabb_perimeter(child2_combined)
                    - native_aabb_perimeter(self.nodes[child2 as usize].aabb)
                    + inheritance_cost
            };
            if cost < cost1 && cost < cost2 {
                break;
            }
            sibling = if cost1 < cost2 { child1 } else { child2 };
        }

        let old_parent = self.nodes[sibling as usize].parent;
        let new_parent = self.allocate_node();
        self.nodes[new_parent as usize].parent = old_parent;
        self.nodes[new_parent as usize].user_data = None;
        self.nodes[new_parent as usize].aabb =
            native_aabb_combine(leaf_aabb, self.nodes[sibling as usize].aabb);
        self.nodes[new_parent as usize].height = self.nodes[sibling as usize].height + 1;
        self.nodes[new_parent as usize].child1 = sibling;
        self.nodes[new_parent as usize].child2 = leaf;
        if old_parent != -1 {
            if self.nodes[old_parent as usize].child1 == sibling {
                self.nodes[old_parent as usize].child1 = new_parent;
            } else {
                self.nodes[old_parent as usize].child2 = new_parent;
            }
        } else {
            self.root = new_parent;
        }
        self.nodes[sibling as usize].parent = new_parent;
        self.nodes[leaf as usize].parent = new_parent;

        let mut index = self.nodes[leaf as usize].parent;
        while index != -1 {
            index = self.balance(index);
            self.recompute_branch(index);
            index = self.nodes[index as usize].parent;
        }
    }
}
