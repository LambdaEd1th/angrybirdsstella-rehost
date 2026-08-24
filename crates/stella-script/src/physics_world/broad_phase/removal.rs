//! `b2DynamicTree::RemoveLeaf` parent splicing, free-list return and refit.

use super::NativeDynamicTree;

impl NativeDynamicTree {
    pub(crate) fn remove_leaf(&mut self, leaf: i32) {
        if leaf == self.root {
            self.root = -1;
            return;
        }
        let parent = self.nodes[leaf as usize].parent;
        let grand_parent = self.nodes[parent as usize].parent;
        let sibling = if self.nodes[parent as usize].child1 == leaf {
            self.nodes[parent as usize].child2
        } else {
            self.nodes[parent as usize].child1
        };
        if grand_parent != -1 {
            if self.nodes[grand_parent as usize].child1 == parent {
                self.nodes[grand_parent as usize].child1 = sibling;
            } else {
                self.nodes[grand_parent as usize].child2 = sibling;
            }
            self.nodes[sibling as usize].parent = grand_parent;
            self.free_node(parent);
            let mut index = grand_parent;
            while index != -1 {
                index = self.balance(index);
                self.recompute_branch(index);
                index = self.nodes[index as usize].parent;
            }
        } else {
            self.root = sibling;
            self.nodes[sibling as usize].parent = -1;
            self.free_node(parent);
        }
    }
}
