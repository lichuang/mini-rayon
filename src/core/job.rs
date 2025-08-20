use std::any::Any;
use std::cell::UnsafeCell;
use std::mem;

use super::latch::Latch;
use super::unwind;

pub enum JobResult<T> {
  None,
  Ok(T),
  Panic(Box<dyn Any + Send>),
}

pub(super) trait Job {
  unsafe fn execute(this: *const ());
}

pub struct JobRef {
  pointer: *const (),
  execute_fn: unsafe fn(*const ()),
}

impl JobRef {
  pub unsafe fn new<T>(data: *const T) -> JobRef
  where T: Job {
    JobRef {
      pointer: data as *const (),
      execute_fn: <T as Job>::execute,
    }
  }

  pub fn id(&self) -> impl Eq {
    (self.pointer, self.execute_fn)
  }

  pub unsafe fn execute(&self) {
    (self.execute_fn)(self.pointer)
  }
}

unsafe impl Send for JobRef {}
unsafe impl Sync for JobRef {}

pub struct StackJob<L, F, R>
where
  L: Latch + Sync,
  F: FnOnce(bool) -> R + Send,
  R: Send,
{
  pub latch: L,
  func: UnsafeCell<Option<F>>,
  result: UnsafeCell<JobResult<R>>,
}

impl<L, F, R> StackJob<L, F, R>
where
  L: Latch + Sync,
  F: FnOnce(bool) -> R + Send,
  R: Send,
{
  pub fn new(func: F, latch: L) -> StackJob<L, F, R> {
    StackJob {
      latch,
      func: UnsafeCell::new(Some(func)),
      result: UnsafeCell::new(JobResult::None),
    }
  }

  pub unsafe fn as_job_ref(&self) -> JobRef {
    JobRef::new(self)
  }

  pub(super) unsafe fn run_inline(self, stolen: bool) -> R {
    self.func.into_inner().unwrap()(stolen)
  }

  pub unsafe fn into_result(self) -> R {
    self.result.into_inner().into_return_value()
  }
}

impl<L, F, R> Job for StackJob<L, F, R>
where
  L: Latch + Sync,
  F: FnOnce(bool) -> R + Send,
  R: Send,
{
  unsafe fn execute(this: *const ()) {
    let this = &*(this as *const Self);
    let abort = unwind::AbortIfPanic;
    let func = (*this.func.get()).take().unwrap();
    (*this.result.get()) = JobResult::call(func);
    Latch::set(&this.latch);
    mem::forget(abort);
  }
}

impl<T> JobResult<T> {
  fn call(func: impl FnOnce(bool) -> T) -> Self {
    match unwind::halt_unwinding(|| func(true)) {
      Ok(x) => JobResult::Ok(x),
      Err(x) => JobResult::Panic(x),
    }
  }

  /// Convert the `JobResult` for a job that has finished (and hence
  /// its JobResult is populated) into its return value.
  ///
  /// NB. This will panic if the job panicked.
  pub(super) fn into_return_value(self) -> T {
    match self {
      JobResult::None => unreachable!(),
      JobResult::Ok(x) => x,
      JobResult::Panic(x) => unwind::resume_unwinding(x),
    }
  }
}
