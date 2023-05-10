use std::cell::RefCell;
use std::rc::Rc;
use serde::{Deserialize, Serialize};
use serde::ser::SerializeStruct;

pub type RENode = Rc<RefCell<ENode>>;
#[macro_export]
macro_rules! db {
    ($e: expr) => {
        $e.deref().borrow()
    };
    (mut, $e: expr) => {
        $e.deref().borrow_mut()
    };
}

#[macro_export]
macro_rules! rr {
    ($e: expr) => {
        Rc::new(RefCell::new($e))
    };
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub struct ENode {
    pub idx: u32,
    pub nth: u8,
    pub weight: usize,
    pub children: Vec<RENode>,
    // metadata for partitioning
    pub _cumulated: usize,
    pub _claimed: bool,
}

impl Serialize for ENode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where S: serde::Serializer
    {

        let mut state = serializer.serialize_struct("ENode", 4)?;
        state.serialize_field("idx", &self.idx)?;
        state.serialize_field("nth", &self.nth)?;
        state.serialize_field("weight", &self.weight)?;
        state.end()
    }
}

// derserialize
impl<'de> Deserialize<'de> for ENode {
    fn deserialize<D>(deserializer: D) -> Result<ENode, D::Error>
        where D: serde::Deserializer<'de>
    {
        unreachable!();
    }
}

impl ENode {
    pub fn new_unclaimed(idx: u32) -> Self {
        ENode {
            idx,
            nth: 0,
            _cumulated: 0,
            _claimed: false,
            weight: 1,
            children: vec![],

        }
    }

    pub fn new_claimed(idx: u32, nth: u8) -> Self {
        ENode {
            idx,
            nth,
            _cumulated: 0,
            _claimed: true,
            weight: 1,
            children: vec![],

        }
    }
}
