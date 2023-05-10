
use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Deref;
use std::rc::Rc;
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize};
use partition::PartitionPlan;
use crate::nodes::{RENode, ENode};
use crate::{db, rr};

pub struct ETree {
    root: RENode,
    _counter: usize
}

impl ETree {
    pub fn new(root: RENode) -> Self {
        ETree {
            root,
            _counter: u32::MAX as usize,
        }
    }
}

impl ETree {
    pub fn add_trace(&mut self, mut trace: Vec<(u32, u8)>) {
        let mut current_node = self.root.clone();
        let (start_idx, start_nth) = trace.remove(0);
        assert_eq!(db!(current_node).idx, start_idx);
        assert_eq!(db!(current_node).nth, start_nth);
        for (idx, nth) in trace {
            let mut found = None;
            for child in &db!(current_node).children {
                if idx == db!(child).idx && nth == db!(child).nth {
                    found = Some(child.clone());
                    break;
                }
            }
            match found {
                None => {
                    let (is_current_unclaimed, next_node) = {
                        let mut is_current_unclaimed = false;
                        let mut next_node = None;
                        for child in &db!(current_node).children {
                            if !(db!(child)._claimed) {
                                is_current_unclaimed = true;
                            }
                            if is_current_unclaimed {
                                db!(mut, child)._claimed = true;
                                db!(mut, child).nth = nth;
                                db!(mut, child).idx = idx;
                                next_node = Some(child.clone());
                                break;
                            }
                        }

                        if !is_current_unclaimed {
                            let new_node = rr!(ENode::new_claimed(idx, nth));
                            db!(mut, current_node).children.push(new_node.clone());
                            next_node = Some(new_node);
                        }
                        (is_current_unclaimed, next_node.expect("next node should be found"))
                    };
                    while db!(current_node).children.len() < 2 {
                        db!(mut, current_node).children.push(
                            rr!(ENode::new_unclaimed(
                                self._counter as u32
                            )),
                        );
                        self._counter -= 1;
                    }
                    current_node = next_node;

                }
                Some(child) => {
                    current_node = child;
                }
            }
        }
    }


    pub fn _dfs_weight_helper(node: &RENode, parent_weight: usize) {
        let cumulated = {
            let node_ref = db!(node);
            let mut cumulated = parent_weight + node_ref.weight as usize;
            if node_ref.children.len() > 0 {
                node_ref.children.iter().for_each(
                    |child: &RENode| Self::_dfs_weight_helper(child, cumulated)
                )
            }
            cumulated
        };
        db!(mut, node)._cumulated = cumulated;
    }

    pub fn _update_weight(&mut self) {
        Self::_dfs_weight_helper(&self.root, 0);
    }

    pub fn _dfs_grab_node_helper(node: &RENode, parts: &mut Vec<PartitionPlan>, trace: Vec<RENode>) {
        let node_ref = db!(node);

        // is leaf
        if node_ref.children.len() == 0 {
            parts.push(PartitionPlan {
                plan: vec![trace, vec![node.clone()]].concat(),
                weight: node_ref._cumulated as usize,
                dependencies: HashMap::new(),
            });
        }
        // is not leaf
        else {
            let mut new_trace = trace.clone();
            new_trace.push(node.clone());
            for child in &node_ref.children {
                Self::_dfs_grab_node_helper(child, parts, (new_trace.clone()));
            }
        }
    }

    pub fn partition(&mut self, k: usize) -> Vec<PartitionPlan> {
        // stage 1: update cumulated weight
        self._update_weight();

        // stage 2: find all leaves
        let mut leaves = vec![];
        Self::_dfs_grab_node_helper(&self.root, &mut leaves, vec![]);

        let mut total_weight = 0;

        for leave in &leaves {
            total_weight += db!(leave.plan.last().unwrap())._cumulated;
        }

        let approx_partition_weight = total_weight / k;

        #[cfg(test)] {
            println!("total weight: {}", total_weight);
            println!("approx_partition_weight: {}", approx_partition_weight);
            leaves.iter().map(|leaf| {
                println!("leaf: {:?} with weight {:?}", leaf.plan.iter().map(|node| {
                    db!(node).idx
                }).collect::<Vec<_>>(), leaf.weight);
            }).collect::<Vec<_>>();
        }

        // stage3: merge until we have k partitions
        let mut partitions: Vec<PartitionPlan> = vec![];

        // we 'll use a greedy algorithm to merge leaves

        // first, sort leaves by weight
        leaves.sort_by(|a, b| b.weight.cmp(&a.weight));
        // then, merge leaves until we have k partitions
        for leaf in leaves {
            let mut merged = false;
            for partition in &mut partitions {
                if partition.weight + leaf.weight <= approx_partition_weight {
                    partition.merge(&leaf);
                    merged = true;
                    break;
                }
            }
            if !merged {
                partitions.push(leaf);
            }
        }
        // if we still have more than k partitions, we merge from last to first
        while partitions.len() > k {
            let last_p = partitions.pop().expect("no partition to pop");
            partitions.last_mut().unwrap().merge(&last_p);
        }
        partitions
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert() {
        // 1 -> 2
        //   -> 3 -> 4
        //        -> 5

        macro_rules! create_enode {
            ($idx: expr) => {
                ENode {
                    idx: $idx,
                    nth: 0,
                    weight: 1,
                    _cumulated: 0,
                    _claimed: false,
                    children: vec![],
                }
            };
        }

        let node1 = rr!(create_enode!(1));
        let mut tree = ETree::new(node1);
        tree.add_trace(vec![(1, 0), (2, 0)]);
        tree.add_trace(vec![(1, 0), (3, 0), (4, 0)]);
        tree.add_trace(vec![(1, 0), (3, 0), (5, 0)]);

        // dfs print
        fn dfs_print(node: &RENode, depth: usize) {
            let node_ref = db!(node);
            println!("{}{}: {}", " ".repeat(depth * 2), node_ref.idx, node_ref._cumulated);
            for child in &node_ref.children {
                dfs_print(child, depth + 1);
            }
        }

        dfs_print(&tree.root, 0);
    }

    #[test]
    fn test_partition() {
        // 1 -> 2
        //   -> 3 -> 4
        //        -> 5

        macro_rules! create_enode {
            ($idx: expr) => {
                ENode {
                    idx: $idx,
                    nth: 0,
                    weight: 1,
                    _cumulated: 0,
                    _claimed: false,
                    children: vec![],
                }
            };
        }

        let node1 = rr!(create_enode!(1));
        let node2 = rr!(create_enode!(2));
        let node3 = rr!(create_enode!(3));
        let node4 = rr!(create_enode!(4));
        let node5 = rr!(create_enode!(5));
        db!(mut, node1).children.push(node2.clone());
        db!(mut, node1).children.push(node3.clone());
        db!(mut, node3).children.push(node4.clone());
        db!(mut, node3).children.push(node5.clone());


        let mut tree = ETree::new(node1);

        for i in tree.partition(2) {
            println!("partition: {:?} with weight {:?}",
                     i.plan.iter().map(|x| db!(x).idx).collect::<Vec<u32>>(), i.weight);
        }
    }
}
