use std::any::Any;
use std::marker::PhantomData;

use super::unwind;
use super::worker::WorkerThread;
use crate::core::SpinLatch;
use crate::core::StackJob;
use crate::core::halt_unwinding;
use crate::core::in_worker;

pub struct FnContext {
  migrated: bool,

  /// disable `Send` and `Sync`, just for a little future-proofing.
  _marker: PhantomData<*mut ()>,
}

impl FnContext {
  #[inline]
  pub fn new(migrated: bool) -> Self {
    FnContext {
      migrated,
      _marker: PhantomData,
    }
  }

  /// Returns `true` if the closure was called from a different thread
  /// than it was provided from.
  #[inline]
  pub fn migrated(&self) -> bool {
    self.migrated
  }
}

pub fn join_context<A, B, RA, RB>(op_a: A, op_b: B) -> (RA, RB)
where
  A: FnOnce(FnContext) -> RA + Send,
  B: FnOnce(FnContext) -> RB + Send,
  RA: Send,
  RB: Send,
{
  #[inline]
  fn call_a<R>(f: impl FnOnce(FnContext) -> R, injected: bool) -> impl FnOnce() -> R {
    move || f(FnContext::new(injected))
  }

  #[inline]
  fn call_b<R>(f: impl FnOnce(FnContext) -> R) -> impl FnOnce(bool) -> R {
    move |migrated| f(FnContext::new(migrated))
  }

  in_worker(|worker_thread, injected| unsafe {
    let job_b = StackJob::new(call_b(op_b), SpinLatch::new(worker_thread));
    let job_b_ref = job_b.as_job_ref();
    // let job_b_id = job_b_ref.id();
    worker_thread.push(job_b_ref.clone());

    let status_a = halt_unwinding(call_a(op_a, injected));
    let result_a = match status_a {
      Ok(v) => v,
      Err(err) => join_recover_from_panic(worker_thread, &job_b.latch, err),
    };

    while !job_b.latch.probe() {
      if let Some(job) = worker_thread.take_local_job() {
        if job_b_ref == job {
          // Found it! Let's run it.
          //
          // Note that this could panic, but it's ok if we unwind here.
          let result_b = job_b.run_inline(injected);
          return (result_a, result_b);
        } else {
          worker_thread.execute(job);
        }
      } else {
        // Local deque is empty. Time to steal from other
        // threads.
        worker_thread.wait_until(&job_b.latch);
        debug_assert!(job_b.latch.probe());
        break;
      }
    }

    (result_a, job_b.into_result())
  })
}

#[cold] // cold path
unsafe fn join_recover_from_panic(
  worker_thread: &WorkerThread,
  job_b_latch: &SpinLatch<'_>,
  err: Box<dyn Any + Send>,
) -> ! {
  worker_thread.wait_until(job_b_latch);
  unwind::resume_unwinding(err)
}
