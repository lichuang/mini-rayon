use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

pub(super) struct AtomicCounters {
  value: AtomicUsize,
}

#[derive(Copy, Clone)]
pub struct Counters {
  word: usize,
}

#[derive(Copy, Clone, Debug, PartialEq, PartialOrd)]
pub struct JobsEventCounter(usize);

impl JobsEventCounter {
  pub const DUMMY: JobsEventCounter = JobsEventCounter(usize::MAX);

  pub fn as_usize(self) -> usize {
    self.0
  }

  pub fn is_sleepy(self) -> bool {
    (self.as_usize() & 1) == 0
  }

  pub fn is_active(self) -> bool {
    !self.is_sleepy()
  }
}

#[cfg(target_pointer_width = "64")]
const THREADS_BITS: usize = 16;

#[cfg(target_pointer_width = "32")]
const THREADS_BITS: usize = 8;

#[allow(clippy::erasing_op)]
const SLEEPING_SHIFT: usize = 0 * THREADS_BITS;

#[allow(clippy::identity_op)]
const INACTIVE_SHIFT: usize = 1 * THREADS_BITS;

const JEC_SHIFT: usize = 2 * THREADS_BITS;

pub(crate) const THREADS_MAX: usize = (1 << THREADS_BITS) - 1;

const ONE_SLEEPING: usize = 1;

const ONE_INACTIVE: usize = 1 << INACTIVE_SHIFT;

const ONE_JEC: usize = 1 << JEC_SHIFT;

impl AtomicCounters {
  pub fn new() -> AtomicCounters {
    AtomicCounters {
      value: AtomicUsize::new(0),
    }
  }

  #[inline]
  pub fn load(&self, ordering: Ordering) -> Counters {
    Counters::new(self.value.load(ordering))
  }

  #[inline]
  fn try_exchange(&self, old_value: Counters, new_value: Counters, ordering: Ordering) -> bool {
    self
      .value
      .compare_exchange(old_value.word, new_value.word, ordering, Ordering::Relaxed)
      .is_ok()
  }

  #[inline]
  pub fn add_inactive_thread(&self) {
    self.value.fetch_add(ONE_INACTIVE, Ordering::SeqCst);
  }

  pub fn increment_jobs_event_counter_if(
    &self,
    increment_when: impl Fn(JobsEventCounter) -> bool,
  ) -> Counters {
    loop {
      let old_value = self.load(Ordering::SeqCst);
      if increment_when(old_value.jobs_counter()) {
        let new_value = old_value.increment_jobs_counter();
        if self.try_exchange(old_value, new_value, Ordering::SeqCst) {
          return new_value;
        }
      } else {
        return old_value;
      }
    }
  }

  pub fn sub_inactive_thread(&self) -> usize {
    let old_value = Counters::new(self.value.fetch_sub(ONE_INACTIVE, Ordering::SeqCst));

    // Current heuristic: whenever an inactive thread goes away, if
    // there are any sleeping threads, wake 'em up.
    let sleeping_threads = old_value.sleeping_threads();
    Ord::min(sleeping_threads, 2)
  }

  pub fn sub_sleeping_thread(&self) {
    let old_value = Counters::new(self.value.fetch_sub(ONE_SLEEPING, Ordering::SeqCst));
    debug_assert!(
      old_value.sleeping_threads() > 0,
      "sub_sleeping_thread: old_value {old_value:?} had no sleeping threads",
    );
    debug_assert!(
      old_value.sleeping_threads() <= old_value.inactive_threads(),
      "sub_sleeping_thread: old_value {:?} had {} sleeping threads and {} inactive threads",
      old_value,
      old_value.sleeping_threads(),
      old_value.inactive_threads(),
    );
  }

  pub fn try_add_sleeping_thread(&self, old_value: Counters) -> bool {
    debug_assert!(
      old_value.inactive_threads() > 0,
      "try_add_sleeping_thread: old_value {old_value:?} has no inactive threads",
    );
    debug_assert!(
      old_value.sleeping_threads() < THREADS_MAX,
      "try_add_sleeping_thread: old_value {old_value:?} has too many sleeping threads",
    );

    let mut new_value = old_value;
    new_value.word += ONE_SLEEPING;

    self.try_exchange(old_value, new_value, Ordering::SeqCst)
  }
}

#[inline]
fn select_thread(word: usize, shift: usize) -> usize {
  (word >> shift) & THREADS_MAX
}

#[inline]
fn select_jec(word: usize) -> usize {
  word >> JEC_SHIFT
}

impl Counters {
  #[inline]
  fn new(word: usize) -> Counters {
    Counters { word }
  }

  #[inline]
  fn increment_jobs_counter(self) -> Counters {
    // We can freely add to JEC because it occupies the most significant bits.
    // Thus it doesn't overflow into the other counters, just wraps itself.
    Counters {
      word: self.word.wrapping_add(ONE_JEC),
    }
  }

  #[inline]
  pub fn jobs_counter(self) -> JobsEventCounter {
    JobsEventCounter(select_jec(self.word))
  }

  pub fn inactive_threads(self) -> usize {
    select_thread(self.word, INACTIVE_SHIFT)
  }

  pub fn awake_but_idle_threads(self) -> usize {
    self.inactive_threads() - self.sleeping_threads()
  }

  pub fn sleeping_threads(self) -> usize {
    select_thread(self.word, SLEEPING_SHIFT)
  }
}

impl std::fmt::Debug for Counters {
  fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let word = format!("{:016x}", self.word);
    fmt
      .debug_struct("Counters")
      .field("word", &word)
      .field("jobs", &self.jobs_counter().0)
      .field("inactive", &self.inactive_threads())
      .field("sleeping", &self.sleeping_threads())
      .finish()
  }
}
