use std::marker::PhantomData;
use std::ops::Index;
use std::sync::atomic::{AtomicPtr, Ordering};

const CONTENTION_THRESHOLD: usize = 2;
const RETRY_THRESHOLD: usize = 2;

pub struct ContentionMeasure(usize);
impl ContentionMeasure {
    pub fn detected(&mut self) -> Result<(), usize> {
        self.0 += 1;
    }

    pub fn use_slow_path(&self) -> bool {
        self.0 > CONTENTION_THRESHOLD
    }
}

pub trait CasDescriptor {
    fn execute(&self) -> Result<(), ()>;
}

pub trait CasDescriptors<D>: Index<usize, Output = D>
where
    D: CasDescriptor,
{
    fn len(&self) -> usize;
}

pub trait NormalizedLockFree {
    type Input: Clone;
    type Output: Clone;
    type Cas: CasDescriptor;
    type Cases: CasDescriptors<Self::Cas> + Clone;

    fn generator(&self, op: &Self::Input, contention: &mut ContentionMeasure) -> Self::Cases;
    fn wrap_up(
        &self,
        executed: Result<(), usize>,
        performed: &Self::Cases,
        contention: &mut ContentionMeasure,
    ) -> Result<Self::Output, ()>;
}

pub struct OperationRecordBox<LF: NormalizedLockFree> {
    val: AtomicPtr<OperationRecord<LF>>,
}

enum OperationState<LF: NormalizedLockFree> {
    PreCas,
    ExecuteCas(LF::Cases),
    PostCas(LF::Cases, Result<(), usize>),
    Completed(LF::Output),
}

struct OperationRecord<LF: NormalizedLockFree> {
    owner: std::thread::ThreadId,
    input: LF::Input,
    state: OperationState<LF>,
}

// A wait-free queue
struct HelpQueue<LF> {
    _o: PhantomData<LF>,
}

impl<LF: NormalizedLockFree> HelpQueue<LF> {
    // TODO: Implement based on Appendix A
    fn enqueue(&self, help: *const OperationRecordBox<LF>) {
        let _ = help;
        todo!()
    }

    fn peek(&self) -> Option<*const OperationRecordBox<LF>> {
        None
    }

    fn try_remove_front(&self, front: *const OperationRecordBox<LF>) -> Result<(), ()> {
        let _ = front;
        Err(())
    }
}

pub struct WaitFreeSimulator<LF: NormalizedLockFree> {
    algorithm: LF,
    help: HelpQueue<LF>,
}

impl<LF: NormalizedLockFree> WaitFreeSimulator<LF> {
    fn cas_execute(
        &self,
        descriptors: &LF::Cases,
        contention: &mut ContentionMeasure,
    ) -> Result<(), usize> {
        let len = descriptors.len();
        for i in 0..len {
            if descriptors[i].execute().is_err() {
                contention.detected();
                return Err(i);
            }
        }
        todo!()
    }

    // Guarantees that on return, orb is no longer in the help queue.
    fn help_op(&self, orb: &OperationRecordBox<LF>) {
        loop {
            let or = unsafe { &*orb.val.load(Ordering::SeqCst) };
            let updated_or = match &or.state {
                OperationState::Completed(..) => {
                    let _ = self.help.try_remove_front(orb);
                    return;
                }
                OperationState::PreCas => {
                    let cas_list = self
                        .algorithm
                        .generator(&or.input, &mut ContentionMeasure(0));
                    Box::new(OperationRecord {
                        owner: or.owner.clone(),
                        input: or.input.clone(),
                        state: OperationState::ExecuteCas(cas_list),
                    })
                }
                OperationState::ExecuteCas(cas_list) => {
                    let outcome = self.cas_execute(cas_list, &mut ContentionMeasure(0));
                    Box::new(OperationRecord {
                        owner: or.owner.clone(),
                        input: or.input.clone(),
                        state: OperationState::PostCas(cas_list.clone(), outcome),
                    })
                }
                OperationState::PostCas(cas_list, outcome) => {
                    if let Ok(result) =
                        self.algorithm
                            .wrap_up(*outcome, cas_list, &mut ContentionMeasure(0))
                    {
                        Box::new(OperationRecord {
                            owner: or.owner.clone(),
                            input: or.input.clone(),
                            state: OperationState::Completed(result),
                        })
                    } else {
                        // we need to start from the generator
                        Box::new(OperationRecord {
                            owner: or.owner.clone(),
                            input: or.input.clone(),
                            state: OperationState::PreCas,
                        })
                    }
                }
            };
            let updated_or = Box::into_raw(updated_or);

            if orb
                .val
                .compare_exchange(
                    or as *const OperationRecord<_> as *mut OperationRecord<_>,
                    updated_or,
                    Ordering::SeqCst,
                    Ordering::Relaxed,
                )
                .is_err()
            {
                // Never got shared, so safe to drop.
                let _ = unsafe { Box::from_raw(updated_or) };
            }
        }
    }

    fn help_first(&self) {
        if let Some(help) = self.help.peek() {
            self.help_op(unsafe { &*help });
        }
    }

    pub fn run(&self, op: LF::Input) -> LF::Output {
        let help = true;
        if help {
            self.help_first();
        }

        // fast path
        for retry in 0.. {
            let mut contention = ContentionMeasure(0);
            let cases = self.algorithm.generator(&op, &mut contention);
            if contention.use_slow_path() {
                break;
            }
            let result = self.cas_execute(&cases, &mut contention);
            match self.algorithm.wrap_up(result, &cases, &mut contention) {
                Ok(outcome) => return outcome,
                Err(()) => {}
            }
            if contention.use_slow_path() {
                break;
            }

            if retry > RETRY_THRESHOLD {
                break;
            }
        }
        // slow path: ask for help.
        let orb = OperationRecordBox {
            val: AtomicPtr::new(Box::into_raw(Box::new(OperationRecord {
                owner: std::thread::current().id(),
                input: op,
                state: OperationState::PreCas,
            }))),
        };
        self.help.enqueue(&orb);
        loop {
            let or = unsafe { &*orb.val.load(Ordering::SeqCst) };
            if let OperationState::Completed(t) = &or.state {
                t.clone();
                todo!()
            } else {
                self.help_first();
            }
        }
    }
}

// struct WaitFreeLinkedList<T> {
//     simulator: WaitFreeSimulator<LockFreeLinkedList<T>>,
// }

// struct LockFreeLinkedList<T> {
//     t: T,
// }

// impl<T> NormalizedLockFree for LockFreeLinkedList<T> {}

// impl<T> WaitFreeLinkedList<T> {
//     pub fn push_front(&self, t: T) {
//         let i = self.simulator.enqueue(Insert(t));
//         self.simulator.wait_for(i);
//     }
// }
