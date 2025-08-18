use std::cell::Cell;
use std::ptr;
use std::sync::Arc;

use crossbeam_deque::Stealer;
use crossbeam_deque::Worker;

use super::job::JobRef;
use super::registry::Registry;

thread_local! {
    static WORKER_THREAD_STATE: Cell<*const WorkerThread> = const { Cell::new(ptr::null()) };
}

pub struct WorkerThread {
  worker: Worker<JobRef>,

  stealer: Stealer<JobRef>,

  index: usize,

  registry: Arc<Registry>,
}

impl Drop for WorkerThread {
  fn drop(&mut self) {
    // Undo `set_current`
    WORKER_THREAD_STATE.with(|t| {
      assert!(t.get().eq(&(self as *const _)));
      t.set(ptr::null());
    });
  }
}

impl WorkerThread {
  pub fn new(worker: Worker<JobRef>, registry: Arc<Registry>, index: usize) -> Self {
    Self {
      stealer: worker.stealer(),
      worker,
      registry: registry,
      index: index,
    }
  }

  pub fn spawn(self) {
    unsafe {
      main_loop(self);
    }
  }

  /// Gets the `WorkerThread` index for the current thread; returns
  /// NULL if this is not a worker thread. This pointer is valid
  /// anywhere on the current thread.
  #[inline]
  pub(super) fn current() -> *const WorkerThread {
    WORKER_THREAD_STATE.with(Cell::get)
  }

  unsafe fn set_current(thread: *const WorkerThread) {
    WORKER_THREAD_STATE.with(|t| {
      assert!(t.get().is_null());
      t.set(thread);
    });
  }
}

unsafe fn main_loop(worker: WorkerThread) {}
