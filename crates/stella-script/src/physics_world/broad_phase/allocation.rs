//! Dynamic-tree node allocation, doubling growth and free-list reuse.

use super::{NativeDynamicTree, NativeDynamicTreeNode};

impl NativeDynamicTree {
    pub(crate) fn allocate_node(&mut self) -> i32 {
        if self.free_list == -1 {
            let old_capacity = self.nodes.len();
            let new_capacity = old_capacity * 2;
            self.nodes.reserve(old_capacity);
            for index in old_capacity..new_capacity {
                self.nodes
                    .push(NativeDynamicTreeNode::free(if index + 1 < new_capacity {
                        (index + 1) as i32
                    } else {
                        -1
                    }));
            }
            self.free_list = old_capacity as i32;
        }
        let node_id = self.free_list;
        let next = self.nodes[node_id as usize].next;
        self.free_list = next;
        self.nodes[node_id as usize] = NativeDynamicTreeNode {
            aabb: (0.0, 0.0, 0.0, 0.0),
            user_data: None,
            parent: -1,
            next: -1,
            child1: -1,
            child2: -1,
            height: 0,
        };
        self.node_count += 1;
        node_id
    }

    pub(crate) fn free_node(&mut self, node_id: i32) {
        self.nodes[node_id as usize] = NativeDynamicTreeNode::free(self.free_list);
        self.free_list = node_id;
        self.node_count = self.node_count.saturating_sub(1);
    }
}
