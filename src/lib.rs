use std::ops::Index;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

const CONTENTION_THRESHOLD: usize = 2;
const RETRY_THRESHOLD: usize = 2;

pub struct ContentionMeasure(usize);
impl ContentionMeasure {
    pub fn detected(&mut self) {
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
    type Input;
    type Output;
    type Cas: CasDescriptor;
    type Cases: CasDescriptors<Self::Cas>;

    fn generator(&self, op: &Self::Input, contention: &mut ContentionMeasure) -> Self::Cases;
    fn wrap_up(
        &self,
        executed: Result<(), usize>,
        performed: &Self::Cases,
        contention: &mut ContentionMeasure,
    ) -> Result<Self::Output, ()>;
}

pub struct OperationRecord {
    completed: AtomicBool,
    at: AtomicUsize,
}

// A wait-free queue
pub struct HelpQueue;

impl HelpQueue {
    pub fn enqueue(&self, help: *const OperationRecord) {}

    pub fn peek(&self) -> Option<*const OperationRecord> {
        None
    }

    pub fn try_remove_front(&self, completed: *const OperationRecord) -> Result<(), ()> {
        Err(())
    }
}

pub struct WaitFreeSimulator<LF: NormalizedLockFree> {
    algorithm: LF,
    help: HelpQueue,
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

    fn help_first(&self) {
        if let Some(help) = self.help.peek() {}
    }

    pub fn run(&self, op: LF::Input) -> LF::Output {
        // fast path
        for retry in 0.. {
            if retry == 0 {
                let help = true;
                if help {
                    self.help_first();
                }
            } else {
                // help more
            }

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
        let i = 0;
        let or = OperationRecord {
            completed: AtomicBool::new(false),
            at: AtomicUsize::new(i),
        };
        self.help.enqueue(&or);
        while !or.completed.load(Ordering::SeqCst) {
            self.help_first();
        }

        todo!()
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
