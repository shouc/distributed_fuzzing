use std::collections::HashMap;
use db;
use nodes::RENode;
use std::ops::Deref;

#[derive(Clone, Debug)]
pub struct PartitionPlan {
    pub plan: Vec<RENode>,
    pub weight: usize,
    pub dependencies: HashMap<usize, Vec<usize>>,
}

impl PartitionPlan {
    pub fn merge(&mut self, other: &PartitionPlan) {
        self.plan = vec![self.plan.clone(), other.plan.clone()].concat();
        self.weight += other.weight;
        for (node, deps) in &other.dependencies {
            self.dependencies.insert(node.clone(), deps.clone());
        }
    }

    pub fn serialize(&self) -> Vec<usize> {
        let mut result = vec![];
        for node in &self.plan {
            result.push(db!(node).idx as usize);
        }
        result
    }
}