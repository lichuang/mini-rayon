use std::cell::Cell;
use std::ptr;

use crossbeam_deque::Worker;

use super::job::JobRef;

pub struct ThreadBuilder {
  worker: Worker<JobRef>,
  index: usize,
}

thread_local! {
    static WORKER_THREAD_STATE: Cell<*const WorkerThread> = const { Cell::new(ptr::null()) };
}

pub struct WorkerThread {
  worker: Worker<JobRef>,
  index: usize,
}

impl From<ThreadBuilder> for WorkerThread {
  fn from(thread: ThreadBuilder) -> Self {
    Self {
      worker: thread.worker,
      index: thread.index,
    }
  }
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
