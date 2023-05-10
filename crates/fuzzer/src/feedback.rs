use std::cell::RefCell;
use std::fmt::{Debug, Formatter};
use std::marker::PhantomData;
use std::rc::Rc;
use libafl::alloc::{
    string::{String, ToString},
    vec::Vec,
};
use std::simd::SimdOrd;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use libafl::{
    bolts::{tuples::Named, AsIter, AsMutSlice, AsSlice, HasRefCnt},
    corpus::Testcase,
    events::{Event, EventFirer},
    executors::ExitKind,
    feedbacks::{Feedback, HasObserverName},
    inputs::UsesInput,
    monitors::UserStats,
    observers::{MapObserver, Observer, ObserversTuple, UsesObserver},
    state::{HasClientPerfMonitor, HasMetadata, HasNamedMetadata},
    Error,
};
use libafl::feedbacks::{DifferentIsNovel, IsNovel, MapFeedbackMetadata, MapIndexesMetadata, MapNoveltiesMetadata, MaxReducer, Reducer};
use libafl::inputs::Input;
use libafl::prelude::HasBytesVec;
use p2p::P2P;
use execution_graph::dgraph::DGraph;
pub static mut IGNORED: [bool; 4096] = [false; 4096];

/// The prefix of the metadata names
pub const DMapFeedback_PREFIX: &str = "DMapFeedback_metadata_";

pub struct DMapFeedback<'a, N, O, R, S, T, I> {
    always_track: bool,
    indexes: bool,
    observer_name: String,
    on_testcase_found: fn(&[u8], &[usize], &P2P),
    on_execution_finished: fn(&P2P,  Rc<RefCell<DGraph>>),
    name: String,
    pub(crate) ignored: Vec<bool>,
    p2p: &'a P2P,
    graph: Rc<RefCell<DGraph>>,
    phantom: PhantomData<(N, O, R, S, T, I)>,
}

impl<'a, N, O, R, S, T, I> Clone for DMapFeedback<'a, N, O, R, S, T, I> {
    fn clone(&self) -> Self {
        todo!()
    }
}

impl<'a, N, O, R, S, T, I>  Debug for DMapFeedback<'a, N, O, R, S, T, I>  {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

impl<'a, N, O, R, S, T, I> UsesObserver<S> for DMapFeedback<'a, N, O, R, S, T, I>
    where
        S: UsesInput,
        O: Observer<S>,
{
    type Observer = O;
}

/// Specialize for the common coverage map size, maximization of u8s
impl<'a, N, O, R, S, T, I> Feedback<S> for DMapFeedback<'a, N, O, R, S, T, I>
    where
        T: PartialEq + Default + Copy + 'static + Serialize + DeserializeOwned + Debug,
        R: Reducer<T>,
        N: IsNovel<T>,
        O: MapObserver<Entry = T>,
        for<'it> O: AsIter<'it, Item = T>,
        I: Input + HasBytesVec,
        S: UsesInput<Input = I> + HasNamedMetadata + HasClientPerfMonitor + Debug,
{
    fn is_interesting<EM, OT>(
        &mut self,
        state: &mut S,
        manager: &mut EM,
        input: &I,
        observers: &OT,
        _exit_kind: &ExitKind,
    ) -> Result<bool, Error>
        where
            EM: EventFirer<State = S>,
            OT: ObserversTuple<S>,
    {
        (self.on_execution_finished)(self.p2p, self.graph.clone());
        let mut interesting = false;
        // TODO Replace with match_name_type when stable
        let observer = observers.match_name::<O>(&self.observer_name).unwrap();

        let map_state = state
            .named_metadata_map_mut()
            .get_mut::<MapFeedbackMetadata<T>>(&self.name)
            .unwrap();
        let len = observer.len();
        if map_state.history_map.len() < len {
            map_state.history_map.resize(len, observer.initial());
        }

        let history_map = map_state.history_map.as_slice();

        let initial = observer.initial();

        let mut interesting_hits = vec![];


        for (i, item) in observer
            .as_iter()
            .copied()
            .enumerate()
            .filter(|(_, item)| *item != initial)
        {
            if unsafe{IGNORED[i]} {
                continue;
            }
            let existing = unsafe { *history_map.get_unchecked(i) };
            let reduced = R::reduce(existing, item);
            if N::is_novel(existing, reduced) {
                interesting = true;
                interesting_hits.push(i);
            }
        }

        if interesting || self.always_track {
            (self.on_testcase_found)(input.bytes(), interesting_hits.as_slice(), self.p2p);
            let len = history_map.len();
            let filled = history_map.iter().filter(|&&i| i != initial).count();
            manager.fire(
                state,
                Event::UpdateUserStats {
                    name: "coverage".to_string(),
                    value: UserStats::Ratio(
                        filled as u64,
                        len as u64,
                    ),
                    phantom: PhantomData,
                },
            )?;
        }

        Ok(interesting)
    }
}

impl<'a, N, O, R, S, T, I> Named for DMapFeedback<'a, N, O, R, S, T, I> {
    #[inline]
    fn name(&self) -> &str {
        self.name.as_str()
    }
}

impl<'a, N, O, R, S, T, I> HasObserverName for DMapFeedback<'a, N, O, R, S, T, I>
    where
        T: PartialEq + Default + Copy + 'static + Serialize + DeserializeOwned + Debug,
        R: Reducer<T>,
        N: IsNovel<T>,
        O: MapObserver<Entry = T>,
        for<'it> O: AsIter<'it, Item = T>,
        S: HasNamedMetadata,
{
    #[inline]
    fn observer_name(&self) -> &str {
        self.observer_name.as_str()
    }
}

fn create_stats_name(name: &str) -> String {
    name.to_lowercase()
}

impl<'a, N, O, R, S, T, I> DMapFeedback<'a, N, O, R, S, T, I>
    where
        T: PartialEq + Default + Copy + 'static + Serialize + DeserializeOwned + Debug,
        R: Reducer<T>,
        O: MapObserver<Entry = T>,
        for<'it> O: AsIter<'it, Item = T>,
        N: IsNovel<T>,
        S: UsesInput + HasNamedMetadata + HasClientPerfMonitor + Debug,
{
    pub fn tracking(map_observer: &O, track_indexes: bool,
                    p2p: &'a P2P,
                    graph: Rc<RefCell<DGraph>>,
                    on_testcase_found: fn (&[u8], &[usize], &P2P), on_execution_finished: fn(&P2P,  Rc<RefCell<DGraph>>)) -> Self {
        Self {
            indexes: track_indexes,
            name: DMapFeedback_PREFIX.to_string() + map_observer.name(),
            observer_name: map_observer.name().to_string(),
            always_track: false,
            phantom: PhantomData,
            ignored: vec![],
            on_testcase_found,
            on_execution_finished,
            p2p,
            graph
        }
    }


}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_is_novel() {
        // sanity check

    }
}