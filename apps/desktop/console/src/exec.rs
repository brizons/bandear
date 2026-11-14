//! The console's flows run as futures on this single-threaded executor, polled only when a reply,
//! a timer or an event comes in: the console stays a state machine whose every step is an intent
//! or an answer, and the web console's async flows port line for line, as deterministic as its
//! tests need.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Context, Poll, Wake, Waker};

type Task = Pin<Box<dyn Future<Output = ()>>>;

/// The tasks woken and waiting to be polled.
type ReadyQueue = Arc<Mutex<VecDeque<usize>>>;

struct TaskWaker {
    task: usize,
    ready: ReadyQueue,
}

impl Wake for TaskWaker {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.ready
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push_back(self.task);
    }
}

/// Runs spawned futures on the thread that owns it, each time [Executor::drive] is called.
#[derive(Default)]
pub struct Executor {
    tasks: RefCell<HashMap<usize, Task>>,
    next: Cell<usize>,
    ready: ReadyQueue,
    driving: Cell<bool>,
}

impl Executor {
    /// Adds `task`, to be polled at the next drive (at once when the executor is driving now).
    pub fn spawn(&self, task: impl Future<Output = ()> + 'static) {
        let id = self.next.get();
        self.next.set(id + 1);
        self.tasks.borrow_mut().insert(id, Box::pin(task));
        self.ready
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push_back(id);
    }

    /// Polls every woken task until none is left woken. A drive asked for while one runs (a task
    /// that spawns or wakes another) is the running one's: it picks the new work up.
    pub fn drive(&self) {
        if self.driving.replace(true) {
            return;
        }
        // Reset even if a task panics, so the executor stays usable for whoever catches it.
        struct Done<'a>(&'a Cell<bool>);
        impl Drop for Done<'_> {
            fn drop(&mut self) {
                self.0.set(false);
            }
        }
        let _done = Done(&self.driving);
        loop {
            let next = self
                .ready
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .pop_front();
            let Some(id) = next else {
                break;
            };
            // Out of the map while it is polled, so it may spawn freely.
            let Some(mut task) = self.tasks.borrow_mut().remove(&id) else {
                continue;
            };
            let waker = Waker::from(Arc::new(TaskWaker {
                task: id,
                ready: Arc::clone(&self.ready),
            }));
            if task
                .as_mut()
                .poll(&mut Context::from_waker(&waker))
                .is_pending()
            {
                self.tasks.borrow_mut().insert(id, task);
            }
        }
    }

    /// How many tasks wait for something.
    pub fn pending(&self) -> usize {
        self.tasks.borrow().len()
    }
}

/// A value one side fills and the other awaits: a reply, a timer, a turn, an idle lane.
pub struct Slot<T>(Rc<RefCell<SlotState<T>>>);

struct SlotState<T> {
    value: Option<T>,
    waker: Option<Waker>,
}

impl<T> Clone for Slot<T> {
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

impl<T> Default for Slot<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Slot<T> {
    pub fn new() -> Self {
        Self(Rc::new(RefCell::new(SlotState {
            value: None,
            waker: None,
        })))
    }

    /// Fills the slot and wakes whoever awaits it.
    pub fn fill(&self, value: T) {
        let waker = {
            let mut state = self.0.borrow_mut();
            state.value = Some(value);
            state.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    /// The value, once filled.
    pub fn wait(&self) -> SlotFuture<T> {
        SlotFuture(self.clone())
    }
}

/// Resolves to a [Slot]'s value once it is filled.
pub struct SlotFuture<T>(Slot<T>);

impl<T> Future for SlotFuture<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<T> {
        let mut state = (self.0).0.borrow_mut();
        match state.value.take() {
            Some(value) => Poll::Ready(value),
            None => {
                state.waker = Some(context.waker().clone());
                Poll::Pending
            }
        }
    }
}
