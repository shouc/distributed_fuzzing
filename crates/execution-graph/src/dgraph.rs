use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Deref;
use std::rc::Rc;
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize};
use partition::PartitionPlan;
use crate::nodes::{RENode, ENode};
use crate::{db, rr};

pub struct DGraph {
    root: RENode,
    available_nodes: HashMap<(u32, u8), RENode>,
    _counter: usize
}

#[derive(Serialize, Deserialize)]
pub struct SerializableDGraph {
    nodes: Vec<(u32, u8, usize)>,
    edges: Vec<(u32, u32)>,
}

impl DGraph {
    pub fn new() -> Self {
        let root = rr!(ENode::new_claimed(0, 0));
        let mut dg = DGraph {
            root,
            available_nodes: HashMap::new(),
            _counter: u32::MAX as usize,
        };
        dg.available_nodes.insert(
            (db!(dg.root).idx, db!(dg.root).nth),
            dg.root.clone(),
        );
        dg
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut nodes = vec![];
        for (key, node) in &self.available_nodes {
            nodes.push((db!(node).idx, db!(node).nth, db!(node).weight));
        }
        let mut edges = vec![];
        for (key, node) in &self.available_nodes {
            for child in &db!(node).children {
                edges.push((db!(node).idx, db!(child).idx));
            }
        }

        let sdg = SerializableDGraph {
            nodes,
            edges,
        };

        bincode::serialize(&sdg).unwrap()

    }


    pub fn deserialize(bytes: Vec<u8>) -> Self {
        let sdg: SerializableDGraph = bincode::deserialize(&bytes).unwrap();
        let mut dg = DGraph::new();
        let mut nodes = HashMap::new();
        for (idx, nth, weight) in sdg.nodes {
            let node = rr!(ENode::new_claimed(idx, 0));
            db!(mut, node).weight = weight;
            db!(mut, node).nth = nth;
            nodes.insert(idx, node.clone());
            dg.available_nodes.insert((idx, nth), node);
        }
        for (idx, child_idx) in sdg.edges {
            let node = nodes.get(&idx).unwrap();
            let child = nodes.get(&child_idx).unwrap();
            db!(mut, node).children.push(child.clone());
        }
        dg
    }

    pub fn merge(&mut self, other: &DGraph) {
        for (key, node) in &other.available_nodes {
            if !self.available_nodes.contains_key(key) {
                self.available_nodes.insert(key.clone(), node.clone());
            } else {
                let self_node = self.available_nodes.get(key).unwrap();
                db!(mut, self_node).weight += db!(node).weight;

                for child in &db!(node).children {
                    let mut is_child_exist = false;
                    for self_child in &db!(self_node).children {
                        if db!(self_child).idx == db!(child).idx {
                            is_child_exist = true;
                            break;
                        }
                    }
                    if !is_child_exist {
                        db!(mut, self_node).children.push(child.clone());
                    }
                }
            }
        }
    }
}

impl DGraph {
    pub fn add_trace(&mut self, mut trace: Vec<(u32, u8)>) {
        let last: Option<RENode> = None;
        for (idx, nth) in trace {
            if !self.available_nodes.contains_key(&(idx, nth)) {
                let new_node = rr!(ENode::new_claimed(idx, nth));
                self.available_nodes.insert((idx, nth), new_node.clone());
                db!(mut, last.clone().unwrap()).children.push(new_node);
            } else {
                let node = self.available_nodes.get(&(idx, nth)).unwrap();
                db!(mut, last.clone().unwrap()).weight += 1;
            }
        }
    }

    pub fn partition(&mut self, k: usize) -> Vec<PartitionPlan> {
        // pick k partitions from the graph
        let partition_size = self.available_nodes.len() / k;

        let mut partitions = vec![];

        let mut current_partition = vec![];
        let mut current_weight = 0;

        for (key, node) in &self.available_nodes {
            current_weight += db!(node).weight;
            current_partition.push(node.clone());
            if current_partition.len() == partition_size {
                partitions.push(PartitionPlan {
                    plan: current_partition,
                    weight: current_weight,
                    dependencies: Default::default(),
                });
                current_partition = vec![];
                current_weight = 0;
            }
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
        let mut tree = DGraph::new(node1);
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


        let mut tree = DGraph::new(node1);

        for i in tree.partition(2) {
            println!("partition: {:?} with weight {:?}",
                     i.plan.iter().map(|x| db!(x).idx).collect::<Vec<u32>>(), i.weight);
        }
    }
}
