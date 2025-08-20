use std::cell::Cell;
use std::ptr;
use std::sync::Arc;

use crossbeam_deque::Steal;
use crossbeam_deque::Stealer;
use crossbeam_deque::Worker;

use super::job::JobRef;
use super::latch::AsCoreLatch;
use super::latch::CoreLatch;
use super::registry::Registry;

thread_local! {
    static WORKER_THREAD_STATE: Cell<*const WorkerThread> = const { Cell::new(ptr::null()) };
}

pub struct WorkerThread {
  worker: Worker<JobRef>,

  stealer: Stealer<JobRef>,

  index: usize,

  pub registry: Arc<Registry>,
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

  pub fn registry(&self) -> &Arc<Registry> {
    &self.registry
  }

  pub fn index(&self) -> usize {
    self.index
  }

  pub unsafe fn push(&self, job: JobRef) {
    let queue_was_empty = self.worker.is_empty();
    self.worker.push(job);
    self.registry.sleep.new_internal_jobs(1, queue_was_empty);
  }

  pub(super) fn take_local_job(&self) -> Option<JobRef> {
    let popped_job = self.worker.pop();

    if popped_job.is_some() {
      return popped_job;
    }

    loop {
      match self.stealer.steal() {
        Steal::Success(job) => return Some(job),
        Steal::Empty => return None,
        Steal::Retry => {}
      }
    }
  }

  pub(super) unsafe fn wait_until<L: AsCoreLatch + ?Sized>(&self, latch: &L) {
    let latch = latch.as_core_latch();
    if !latch.probe() {
      self.wait_until_cold(latch);
    }
  }

  unsafe fn wait_until_cold(&self, latch: &CoreLatch) {
    while !latch.probe() {
      if let Some(job) = self.take_local_job() {
        self.execute(job);
        continue;
      }

      let mut idle_state = self.registry.sleep.start_looking(self.index);
      while !latch.probe() {}
    }
  }

  fn find_work(&self) -> Option<JobRef> {
    self
      .take_local_job()
      .or_else(|| self.steal().or_else(|| self.registry.pop_injected_job()))
  }

  fn steal(&self) -> Option<JobRef> {
    let thread_infos = self.registry.thread_infos.as_slice();
    let num_threads = thread_infos.len();
    if num_threads <= 1 {
      return None;
    }

    loop {
      let mut retry = false;
      let job = (0..num_threads)
        .filter(|&i| i != self.index)
        .find_map(|victim_index| {
          let victim = &thread_infos[victim_index];
          match victim.stealer.steal() {
            Steal::Success(job) => Some(job),
            Steal::Empty => None,
            Steal::Retry => {
              retry = true;
              None
            }
          }
        });
      if job.is_some() || !retry {
        return job;
      }
    }
  }

  pub unsafe fn execute(&self, job: JobRef) {
    job.execute();
  }
}

unsafe fn main_loop(worker: WorkerThread) {
  unimplemented!()
}
