use std::collections::HashMap;
use std::ops::Deref;
use std::thread::sleep;
use fuzzer::p2p::P2P;
use mpi;
use mpi::topology::Communicator;
use mpi::point_to_point::{Destination, Source, Status};
use mpi::Rank;
use fuzzer::fuzzing::fuzz_process_epoch;

/// Msg: 0..4 -> Pkt Len (big endian)
/// Msg: 4 -> Pkt Type
/// Msg: 5.. -> Pkt Data
///
/// 0 -> Request corpus
/// 1 -> Corpus size
/// 2 -> Send corpus
/// 3 -> Testcase found

fn on_testcase_found(data: &[u8], _: &[usize], p2p: &P2P) {
    let data = data.to_vec();
    let size = data.len() + 1;
    assert!(size < 4091);
    let mut msg = vec![0; data.len() + 5];
    msg[0] = (size >> 24) as u8;
    msg[1] = (size >> 16) as u8;
    msg[2] = (size >> 8) as u8;
    msg[3] = size as u8;
    msg[4] = 3;
    msg[5..].copy_from_slice(&data[..]);
    p2p.send(msg, 0);
}

fn on_execution_finished(p2p: &P2P) {}


pub fn get_u32(v: &[u8], offset: usize) -> u32 {
    let mut size = 0;
    for i in offset..(offset + 4) {
        size = size << 8;
        size += v[i] as u32;
    }
    size
}

fn sync_corpus(p2p: &P2P) -> Vec<Vec<u8>> {

    p2p.send(vec![0,0,0,1, 0], 0);
    let (corpus_size, _) = p2p.recv(0);
    assert!(corpus_size[4] == 1);

    let corpus_size = get_u32(&corpus_size, 5) as usize;

    let mut res = vec![];

    while corpus_size < res.len() {
        let (msg, _) = p2p.recv(0);
        let testcase_size = (get_u32(&msg, 0) - 1) as usize;
        assert!(msg[4] == 2);
        let mut testcase = vec![];
        testcase.copy_from_slice(&msg[5..(testcase_size + 5)]);
        res.push(testcase);
    }
    res
}

fn main() {
    let universe = mpi::initialize().unwrap();
    let world = universe.world();
    let rank = world.rank();
    let mut p2p = P2P::new(world, rank);
    println!("Hello from process {} of {}", rank, world.size());

    if rank > 0 {
        fuzz_process_epoch(
            &p2p,
            on_testcase_found,
            on_execution_finished,
            sync_corpus
        );
    } else {
        let mut offsets: HashMap<u32, u32> = HashMap::new();
        let mut corpus = vec![];
        loop {
            let (msg, status) = p2p.recv_any();
            let pkt_type = msg[4];
            assert!(pkt_type == 0 || pkt_type == 3);

            if pkt_type == 0 {
                let from = status.source_rank() as u32;
                let offset = u32::from_le_bytes(offsets.entry(from).or_insert(0).deref().deref().to_be_bytes());
                let size = corpus.len() as u32 - offset;
                let mut msg = vec![0,0,0,5,1,0,0,0,0];
                msg[5] = (size >> 24) as u8;
                msg[6] = (size >> 16) as u8;
                msg[7] = (size >> 8) as u8;
                msg[8] = size as u8;
                p2p.send(msg, from);

                for i in offset..(corpus.len() as u32) {
                    let testcase: &Vec<u8> = &corpus[i as usize];
                    let size = testcase.len() + 1;
                    let mut msg = vec![0; testcase.len() + 5];
                    msg[0] = (size >> 24) as u8;
                    msg[1] = (size >> 16) as u8;
                    msg[2] = (size >> 8) as u8;
                    msg[3] = size as u8;
                    msg[4] = 2;
                    msg.copy_from_slice(&testcase[..]);
                    p2p.send(msg, from);
                }
            }

            if pkt_type == 3 {
                let testcase_size = get_u32(&msg, 0) as usize - 1;
                let mut testcase = vec![];
                testcase.copy_from_slice(&msg[5..(testcase_size + 5)]);
                corpus.push(testcase);
                println!("Corpus size: {}", corpus.len());
            }

        }
    }
}
