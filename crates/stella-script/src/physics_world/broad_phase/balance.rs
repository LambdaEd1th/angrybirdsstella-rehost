//! `b2DynamicTree::Balance` rotations and branch AABB/height refitting.

use super::{NativeDynamicTree, native_aabb_combine};

impl NativeDynamicTree {
    pub(crate) fn recompute_branch(&mut self, node_id: i32) {
        let child1 = self.nodes[node_id as usize].child1;
        let child2 = self.nodes[node_id as usize].child2;
        if child1 == -1 || child2 == -1 {
            return;
        }
        self.nodes[node_id as usize].height = 1 + self.nodes[child1 as usize]
            .height
            .max(self.nodes[child2 as usize].height);
        self.nodes[node_id as usize].aabb = native_aabb_combine(
            self.nodes[child1 as usize].aabb,
            self.nodes[child2 as usize].aabb,
        );
    }

    pub(crate) fn replace_parent_child(&mut self, parent: i32, old_child: i32, new_child: i32) {
        if parent == -1 {
            self.root = new_child;
        } else if self.nodes[parent as usize].child1 == old_child {
            self.nodes[parent as usize].child1 = new_child;
        } else {
            self.nodes[parent as usize].child2 = new_child;
        }
    }

    pub(crate) fn balance(&mut self, node_id: i32) -> i32 {
        let node = self.nodes[node_id as usize].clone();
        if node.is_leaf() || node.height < 2 {
            return node_id;
        }
        let child1 = node.child1;
        let child2 = node.child2;
        let balance = self.nodes[child2 as usize].height - self.nodes[child1 as usize].height;
        if balance > 1 {
            let upper = self.nodes[child2 as usize].clone();
            let grand1 = upper.child1;
            let grand2 = upper.child2;
            self.nodes[child2 as usize].child1 = node_id;
            self.nodes[child2 as usize].parent = node.parent;
            self.nodes[node_id as usize].parent = child2;
            self.replace_parent_child(node.parent, node_id, child2);
            if self.nodes[grand1 as usize].height > self.nodes[grand2 as usize].height {
                self.nodes[child2 as usize].child2 = grand1;
                self.nodes[node_id as usize].child2 = grand2;
                self.nodes[grand2 as usize].parent = node_id;
                self.recompute_branch(node_id);
                self.recompute_branch(child2);
            } else {
                self.nodes[child2 as usize].child2 = grand2;
                self.nodes[node_id as usize].child2 = grand1;
                self.nodes[grand1 as usize].parent = node_id;
                self.recompute_branch(node_id);
                self.recompute_branch(child2);
            }
            return child2;
        }
        if balance < -1 {
            let upper = self.nodes[child1 as usize].clone();
            let grand1 = upper.child1;
            let grand2 = upper.child2;
            self.nodes[child1 as usize].child1 = node_id;
            self.nodes[child1 as usize].parent = node.parent;
            self.nodes[node_id as usize].parent = child1;
            self.replace_parent_child(node.parent, node_id, child1);
            if self.nodes[grand1 as usize].height > self.nodes[grand2 as usize].height {
                self.nodes[child1 as usize].child2 = grand1;
                self.nodes[node_id as usize].child1 = grand2;
                self.nodes[grand2 as usize].parent = node_id;
                self.recompute_branch(node_id);
                self.recompute_branch(child1);
            } else {
                self.nodes[child1 as usize].child2 = grand2;
                self.nodes[node_id as usize].child1 = grand1;
                self.nodes[grand1 as usize].parent = node_id;
                self.recompute_branch(node_id);
                self.recompute_branch(child1);
            }
            return child1;
        }
        node_id
    }
}
