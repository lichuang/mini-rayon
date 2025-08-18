use std::ptr;
use std::sync::Arc;
use std::sync::Once;

use anyhow::Result;
use crossbeam_deque::Injector;
use crossbeam_deque::Stealer;
use crossbeam_deque::Worker;

use super::job::JobRef;
use super::latch::LockLatch;
use super::latch::OnceLatch;
use super::sleep::Sleep;
use super::worker::WorkerThread;
use crate::core::job::StackJob;
use crate::core::latch::LatchRef;

pub struct ThreadPoolBuilder {
  num_threads: usize,
}

impl Default for ThreadPoolBuilder {
  fn default() -> Self {
    ThreadPoolBuilder { num_threads: 2 }
  }
}

static mut THE_REGISTRY: Option<Arc<Registry>> = None;
static THE_REGISTRY_SET: Once = Once::new();

struct ThreadInfo {
  primed: LockLatch,

  stopped: LockLatch,

  terminate: OnceLatch,

  stealer: Stealer<JobRef>,
}

impl ThreadInfo {
  fn new(stealer: Stealer<JobRef>) -> ThreadInfo {
    ThreadInfo {
      primed: LockLatch::new(),
      stopped: LockLatch::new(),
      terminate: OnceLatch::new(),
      stealer,
    }
  }
}

pub struct Registry {
  thread_infos: Vec<ThreadInfo>,
  injected_jobs: Injector<JobRef>,
  sleep: Sleep,
}

impl Registry {
  pub fn new(builder: &ThreadPoolBuilder) -> Result<Arc<Registry>> {
    let n_threads = builder.num_threads;

    let (workers, stealers): (Vec<_>, Vec<_>) = (0..n_threads)
      .map(|_| {
        let worker = Worker::new_fifo();
        let stealer = worker.stealer();
        (worker, stealer)
      })
      .unzip();

    let registry = Arc::new(Registry {
      thread_infos: stealers.into_iter().map(ThreadInfo::new).collect(),
      injected_jobs: Injector::new(),
      sleep: Sleep::new(n_threads),
    });

    for (index, worker) in workers.into_iter().enumerate() {
      let worker = WorkerThread::new(worker, Arc::clone(&registry), index);

      worker.spawn();
    }
    Ok(registry)
  }

  fn inject(&self, injected_job: JobRef) {
    let queue_was_empty = self.injected_jobs.is_empty();
    self.injected_jobs.push(injected_job);
  }

  pub fn in_worker<OP, R>(&self, op: OP) -> R
  where
    OP: FnOnce(&WorkerThread, bool) -> R + Send,
    R: Send,
  {
    unsafe {
      let worker = WorkerThread::current();
      if worker.is_null() {
        self.in_worker_cold(op)
      } else {
        op(&*worker, false)
      }
    }
  }

  unsafe fn in_worker_cold<OP, R>(&self, op: OP) -> R
  where
    OP: FnOnce(&WorkerThread, bool) -> R + Send,
    R: Send,
  {
    thread_local!(static LOCK_LATCH: LockLatch = LockLatch::new());

    LOCK_LATCH.with(|l| {
      debug_assert!(WorkerThread::current().is_null());
      let job = StackJob::new(
        |injected| {
          let worker_thread = WorkerThread::current();
          assert!(injected && !worker_thread.is_null());
          op(&*worker_thread, injected)
        },
        LatchRef::new(l),
      );
      self.inject(job.as_job_ref());
      job.latch.wait_and_reset();

      job.into_result()
    })
  }
}

fn default_global_registry() -> Result<Arc<Registry>> {
  let result = Registry::new(&ThreadPoolBuilder::default());

  result
}

fn set_global_registry() -> Result<&'static Arc<Registry>> {
  let mut result = Err(anyhow::Error::msg("Global Worker Pool Already Initialized"));

  THE_REGISTRY_SET.call_once(|| {
    result = default_global_registry().map(|registry| unsafe {
      ptr::addr_of_mut!(THE_REGISTRY).write(Some(registry));
      (*ptr::addr_of!(THE_REGISTRY)).as_ref().unwrap_unchecked()
    })
  });

  result
}

fn global_registry() -> &'static Arc<Registry> {
  set_global_registry()
    .or_else(|err| {
      let registry = unsafe { &*ptr::addr_of!(THE_REGISTRY) };
      registry.as_ref().ok_or(err)
    })
    .expect("The global thread pool has not been initialized.")
}

pub fn in_worker<OP, R>(op: OP) -> R
where
  OP: FnOnce(&WorkerThread, bool) -> R + Send,
  R: Send,
{
  unsafe {
    let worker = WorkerThread::current();
    if worker.is_null() {
      global_registry().in_worker(op)
    } else {
      op(&*worker, false)
    }
  }
}
