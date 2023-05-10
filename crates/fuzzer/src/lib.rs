#![feature(portable_simd)]

pub mod feedback;
pub mod fuzzing;
pub mod p2p;

extern crate libafl;
extern crate serde;
extern crate libafl_targets;
extern crate mpi;
extern crate execution_graph;