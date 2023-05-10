use std::cell::RefCell;
use libafl::prelude::{MaxReducer, SimplePrintingMonitor, TimeFeedback};
use std::env;
use std::path::PathBuf;
use std::rc::Rc;
use libafl::prelude::TimeoutFeedback;
use std::time::Duration;
use libafl::{Evaluator, feedback_or, feedback_or_fast, Fuzzer, StdFuzzer};
use libafl::prelude::CrashFeedback;
use libafl::bolts::{AsSlice, current_nanos};
use libafl::corpus::{InMemoryCorpus, OnDiskCorpus};
use libafl::events::{NopEventManager, SimpleEventManager};
use libafl::executors::{ExitKind, InProcessExecutor, TimeoutExecutor};
use libafl::feedbacks::{DifferentIsNovel, MaxMapFeedback};
use libafl::inputs::{BytesInput, HasTargetBytes, UsesInput};
use libafl::mutators::{havoc_mutations, StdScheduledMutator};
use libafl::observers::{HitcountsMapObserver, StdMapObserver, TimeObserver};
use libafl::prelude::{CalibrationStage, IndexesLenTimeMinimizerScheduler, Named, Observer, StdPowerMutationalStage, StdRand, StdState, StdWeightedScheduler, tuple_list};
use libafl::prelude::powersched::PowerSchedule;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde::de::DeserializeOwned;
use libafl_targets::{libfuzzer_initialize, libfuzzer_test_one_input, EDGES_MAP, MAX_EDGES_NUM};
use feedback::DMapFeedback;
use p2p::P2P;
use execution_graph::dgraph::DGraph;


pub fn fuzz_process_epoch(
    p2p: &P2P,
    dgraph: Rc<RefCell<DGraph>>,
    on_testcase_found: fn (&[u8], &[usize], &P2P),
    on_execution_finished: fn(&P2P, Rc<RefCell<DGraph>>),
    sync_corpus: fn(p2p: &P2P, Rc<RefCell<DGraph>>) -> Vec<Vec<u8>>,
    ignored: Vec<bool>,
) {
    let edges_observer = unsafe {
        HitcountsMapObserver::new(StdMapObserver::from_mut_ptr(
            "edges",
            EDGES_MAP.as_mut_ptr(),
            MAX_EDGES_NUM,
        ))
    };

    let time_observer = TimeObserver::new("time");
    let mut map_feedback = DMapFeedback
        ::<DifferentIsNovel, _, MaxReducer, _, _, _>
    ::tracking(&edges_observer, true, p2p, dgraph.clone(), on_testcase_found, on_execution_finished);

    map_feedback.ignored = ignored;

    let calibration = CalibrationStage::new(&map_feedback);

    let mut feedback = feedback_or!(
        map_feedback,
        TimeFeedback::with_observer(&time_observer)
    );

    let mut objective = feedback_or_fast!(CrashFeedback::new(), TimeoutFeedback::new());

    let corpus_dirs = [PathBuf::from("corpus")];

    let mut state = StdState::new(
        StdRand::with_seed(current_nanos()),
        InMemoryCorpus::new(),
        OnDiskCorpus::new("solution").unwrap(),
        &mut feedback,
        &mut objective,
    ).unwrap();

    println!("We're a client, let's fuzz :)");

    let mutator = StdScheduledMutator::new(havoc_mutations());

    let power = StdPowerMutationalStage::new(mutator);

    let mut stages = tuple_list!(calibration, power);

    let scheduler = IndexesLenTimeMinimizerScheduler::new(StdWeightedScheduler::with_schedule(
        &mut state,
        &edges_observer,
        Some(PowerSchedule::FAST),
    ));

    let mut fuzzer = StdFuzzer::new(scheduler, feedback, objective);

    let mut harness = |input: &BytesInput| {
        let target = input.target_bytes();
        let buf = target.as_slice();
        libfuzzer_test_one_input(buf);
        ExitKind::Ok
    };

    let mut mgr = SimpleEventManager::new(
        SimplePrintingMonitor::new(),
    );

    let mut executor = TimeoutExecutor::new(
        InProcessExecutor::new(
            &mut harness,
            tuple_list!(edges_observer, time_observer),
            &mut fuzzer,
            &mut state,
            &mut mgr,
        ).unwrap(),
        // 10 seconds timeout
        Duration::new(10, 0),
    );

    let args: Vec<String> = env::args().collect();
    if libfuzzer_initialize(&args) == -1 {
        println!("Warning: LLVMFuzzerInitialize failed with -1");
    }


    let iters = 1000;
    loop {
        fuzzer.fuzz_loop_for(
            &mut stages,
            &mut executor,
            &mut state,
            &mut mgr,
            iters,
        ).unwrap();

        for inp in sync_corpus(p2p, dgraph.clone()) {
            fuzzer.evaluate_input(
                &mut state, &mut executor, &mut mgr,BytesInput::from(inp)
            ).unwrap();
        };
    }
}
