use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Deref;
use std::rc::Rc;
use std::thread::sleep;
use fuzzer::p2p::P2P;
use mpi;
use mpi::topology::Communicator;
use mpi::point_to_point::{Destination, ReceiveFuture, Source, Status};
use mpi::Rank;
use fuzzer::fuzzing::fuzz_process_epoch;
use lazy_static::lazy_static;
use mpi::traits::Equivalence;
use execution_graph::db;
// 1.4.0
use execution_graph::dgraph::DGraph;
use execution_graph::partition::PartitionPlan;
use fuzzer::feedback::IGNORED;

/// Msg: 0..4 -> Pkt Len (big endian)
/// Msg: 4 -> Pkt Type
/// Msg: 5.. -> Pkt Data
///
/// 0 -> Share execution tree
/// 1 -> Share latest execution tree
/// 2 -> Share spills
#[derive(Equivalence)]
struct Wrapper {
    buf: [u8; 4096],
}


pub static mut __extern_ptrace: [u32; 4096] = [0; 4096];

// who owns what
pub static mut __partitions: [u32; 4096] = [0; 4096];

fn on_testcase_found(data: &[u8], intt: &[usize], p2p: &P2P) {
    for i in intt {
        let mut last: usize = 0;
        // find parents of interesting hits
        for t in unsafe {__extern_ptrace.iter()} {
            if *t as usize == *i {
                break;
            }
            last = *t as usize;
        }
        let owner_rank = unsafe { __partitions[last as usize] };
        if owner_rank != 0 {
            let mut msg = vec![0; data.len() + 5];
            let size = data.len() + 1;
            msg[0] = (size >> 24) as u8;
            msg[1] = (size >> 16) as u8;
            msg[2] = (size >> 8) as u8;
            msg[3] = size as u8;
            msg[4] = 2;
            msg[5..].copy_from_slice(&data[..]);
            p2p.send(msg, owner_rank as u32);
        } else {
            println!("Interesting hit not in partition");
        }
    }
}

fn on_execution_finished(p2p: &P2P, dgraph: Rc<RefCell<DGraph>>) {
    // inside partition!
    let mut appearance = vec![0; 4096];
    let mut trace = vec![];
    for i in unsafe { __extern_ptrace.iter() } {
        trace.push((*i as u32, appearance[*i as usize]));
        appearance[*i as usize] = (appearance[*i as usize] + 1) % 255;
    }
    db!(mut, dgraph).add_trace(trace)
}


pub fn get_u32(v: &[u8], offset: usize) -> u32 {
    let mut size = 0;
    for i in offset..(offset + 4) {
        size = size << 8;
        size += v[i] as u32;
    }
    size
}

fn sync_corpus(p2p: &P2P, dgraph: Rc<RefCell<DGraph>>) -> Vec<Vec<u8>> {
    // send execution tree
    let mut msg = vec![0; 4096];
    let mut serialized = db!(dgraph).serialize();
    let size = serialized.len() + 1;
    assert!(size < 4091);

    msg[0] = (size >> 24) as u8;
    msg[1] = (size >> 16) as u8;
    msg[2] = (size >> 8) as u8;
    msg[3] = size as u8;
    msg[4] = 0;
    msg[5..].copy_from_slice(&serialized[..]);

    p2p.send(msg, 0);


    // share with others
    let mut result = vec![];

    loop {
        let fut: ReceiveFuture<Wrapper> = p2p.world.any_process().immediate_receive();
        match fut.r#try() {
            Ok((msg, status)) => {
                let msg = msg.buf;
                let pkt_type = msg[4];

                if pkt_type == 1 {
                    let tree_size = get_u32(&msg, 0) - 1;
                    let tree = &msg[5..(5 + tree_size as usize)];
                    let lg = DGraph::deserialize(tree.to_vec());
                    db!(mut, dgraph).merge(&lg);
                    let pps = db!(mut, dgraph).partition(p2p.world.size() as usize);
                    let ignored_p: PartitionPlan = pps[(p2p.rank - 1) as usize].clone();
                    for v in ignored_p.plan.iter().map(|x| db!(x).idx).collect::<Vec<u32>>() {
                        unsafe {
                            IGNORED[v as usize] = false;
                        }
                    }

                    for rank in 0..pps.len() {
                        for p in pps[rank].plan.iter() {
                            unsafe { __partitions[db!(p).idx as usize] = rank as u32; }
                        }
                    }

                } else if pkt_type == 2 {
                    let testcase_size = get_u32(&msg, 0) - 1;
                    let testcase = &msg[5..(5 + testcase_size as usize)];
                    result.push(testcase.to_vec());
                }
            }
            Err(_) => {
                return result;
            }
        }
    }
}

fn main() {
    let universe = mpi::initialize().unwrap();
    let world = universe.world();
    let rank = world.rank();
    let mut p2p = P2P::new(world, rank);
    println!("Hello from process {} of {}", rank, world.size());

    let mut dgraph = DGraph::new();


    if rank > 0 {
        let ignored = vec![false; 4096];
        fuzz_process_epoch(
            &p2p,
            Rc::new(RefCell::new(dgraph)),
            on_testcase_found,
            on_execution_finished,
            sync_corpus,
            ignored,
        );
    } else {
        loop {
            let (msg, status) = p2p.recv_any();
            let pkt_type = msg[4];
            assert!(pkt_type == 0);

            if pkt_type == 0 {
                let from = status.source_rank();
                let msg_size = get_u32(&msg, 0) as usize - 1;
                dgraph.merge(
                    &DGraph::deserialize(msg[5..(5 + msg_size)].to_vec())
                );

                let mut msg = vec![0; 5];
                msg[4] = 1;
                let mut serialized = dgraph.serialize();
                let size = serialized.len() + 1;
                msg[0] = (size >> 24) as u8;
                msg[1] = (size >> 16) as u8;
                msg[2] = (size >> 8) as u8;
                msg[3] = size as u8;
                msg.append(&mut serialized);

                p2p.send(msg, from as u32);
            }
        }
    }
}
