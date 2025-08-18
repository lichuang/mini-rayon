use std::marker::PhantomData;
use std::ops::Deref;
use std::sync::Condvar;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

pub trait Latch {
  unsafe fn set(this: *const Self);
}

pub trait AsCoreLatch {
  fn as_core_latch(&self) -> &CoreLatch;
}

/// Latch is not set, owning thread is awake
const UNSET: usize = 0;

/// Latch is not set, owning thread is going to sleep on this latch
/// (but has not yet fallen asleep).
const SLEEPY: usize = 1;

/// Latch is not set, owning thread is asleep on this latch and
/// must be awoken.
const SLEEPING: usize = 2;

/// Latch is set.
const SET: usize = 3;

pub struct CoreLatch {
  state: AtomicUsize,
}

pub struct LockLatch {
  m: Mutex<bool>,
  v: Condvar,
}

impl LockLatch {
  pub fn new() -> LockLatch {
    LockLatch {
      m: Mutex::new(false),
      v: Condvar::new(),
    }
  }

  pub(super) fn wait_and_reset(&self) {
    let mut guard = self.m.lock().unwrap();
    while !*guard {
      guard = self.v.wait(guard).unwrap();
    }
    *guard = false;
  }
}

impl CoreLatch {
  fn new() -> Self {
    Self {
      state: AtomicUsize::new(0),
    }
  }

  pub fn get_sleepy(&self) -> bool {
    self
      .state
      .compare_exchange(UNSET, SLEEPY, Ordering::SeqCst, Ordering::Relaxed)
      .is_ok()
  }

  pub fn fall_asleep(&self) -> bool {
    self
      .state
      .compare_exchange(SLEEPY, SLEEPING, Ordering::SeqCst, Ordering::Relaxed)
      .is_ok()
  }

  pub fn wake_up(&self) {
    if !self.probe() {
      let _ = self
        .state
        .compare_exchange(SLEEPING, UNSET, Ordering::SeqCst, Ordering::Relaxed);
    }
  }

  unsafe fn set(this: *const Self) -> bool {
    let old_state = (*this).state.swap(SET, Ordering::AcqRel);
    old_state == SLEEPING
  }

  pub fn probe(&self) -> bool {
    self.state.load(Ordering::Acquire) == SET
  }
}

impl Latch for LockLatch {
  unsafe fn set(this: *const Self) {
    unsafe {
      let mut guand = (*this).m.lock().unwrap();
      *guand = true;
      (*this).v.notify_all();
    }
  }
}

pub(super) struct OnceLatch {
  core_latch: CoreLatch,
}

impl OnceLatch {
  pub fn new() -> OnceLatch {
    Self {
      core_latch: CoreLatch::new(),
    }
  }
}

pub struct LatchRef<'a, L> {
  inner: *const L,
  marker: PhantomData<&'a L>,
}

unsafe impl<L: Sync> Sync for LatchRef<'_, L> {}

impl<L> LatchRef<'_, L> {
  pub(super) fn new(inner: &L) -> LatchRef<'_, L> {
    LatchRef {
      inner,
      marker: PhantomData,
    }
  }
}

// unsafe impl<L: Sync> Sync for LatchRef<'_, L> {}

impl<L> Deref for LatchRef<'_, L> {
  type Target = L;

  fn deref(&self) -> &L {
    // SAFETY: if we have &self, the inner latch is still alive
    unsafe { &*self.inner }
  }
}

impl<L: Latch> Latch for LatchRef<'_, L> {
  #[inline]
  unsafe fn set(this: *const Self) {
    L::set((*this).inner);
  }
}
