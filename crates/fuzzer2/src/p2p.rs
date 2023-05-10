use mpi;
use mpi::collective::Root;
use mpi::point_to_point::{Destination, Source, Status};
use mpi::Rank;
use mpi::topology::{Communicator, Process, SystemCommunicator};

pub struct P2P {
    pub world: SystemCommunicator,
    pub rank: Rank,
}

impl P2P {
    pub fn new(world: SystemCommunicator, rank: Rank) -> Self {
        P2P {
            world,
            rank,
        }
    }


    pub fn send(&self, msg: Vec<u8>, dest: u32) {
        return mpi::request::scope(|scope| {
            self.world
                .process_at_rank(dest as Rank)
                .immediate_send(scope, msg.as_slice())
                .wait();
        });
    }

    pub fn recv(&self, from: u32) -> (Vec<u8>, Status) {
        println!("Receiving message from process {}", from);
        let mut msg = vec![0; 32];
        let status = self.world
            .process_at_rank(from as Rank)
            .receive_into(msg.as_mut_slice());
        println!("Received message from process {} with tag {}",
                 status.source_rank(),
                 status.tag());
        (msg.to_vec(), status)
    }

    pub fn recv_any(&mut self) -> (Vec<u8>, Status) {
        let mut msg = vec![0; 4096];
        let status = self.world
            .any_process()
            .receive_into(msg.as_mut_slice());
        println!("Received message from process {} with tag {}",
                 status.source_rank(),
                 status.tag());
        (msg.to_vec(), status)
    }
}

