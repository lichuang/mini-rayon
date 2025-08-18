use std::sync::Condvar;
use std::sync::Mutex;
use std::sync::atomic::Ordering;
use std::thread;

use crossbeam_utils::CachePadded;

use super::counter::AtomicCounters;
use super::counter::JobsEventCounter;
use super::latch::CoreLatch;
use crate::core::counter::THREADS_MAX;

pub struct Sleep {
  worker_sleep_states: Vec<CachePadded<WorkerSleepState>>,

  counters: AtomicCounters,
}

pub struct IdleState {
  worker_index: usize,

  rounds: u32,

  jobs_counter: JobsEventCounter,
}

#[derive(Default)]
struct WorkerSleepState {
  is_sleep: Mutex<bool>,

  condvar: Condvar,
}

const ROUNDS_UNTIL_SLEEPY: u32 = 32;
const ROUNDS_UNTIL_SLEEPING: u32 = ROUNDS_UNTIL_SLEEPY + 1;

impl Sleep {
  pub fn new(n_threads: usize) -> Sleep {
    assert!(n_threads <= THREADS_MAX);
    Sleep {
      worker_sleep_states: (0..n_threads).map(|_| Default::default()).collect(),
      counters: AtomicCounters::new(),
    }
  }

  pub fn start_looking(&self, worker_index: usize) -> IdleState {
    self.counters.add_inactive_thread();

    IdleState {
      worker_index,
      rounds: 0,
      jobs_counter: JobsEventCounter::DUMMY,
    }
  }

  pub fn work_found(&self) {
    let threads_to_wake = self.counters.sub_inactive_thread();
    self.wake_any_threads(threads_to_wake as u32);
  }

  pub(super) fn no_work_found(
    &self,
    idle_state: &mut IdleState,
    latch: &CoreLatch,
    has_injected_jobs: impl FnOnce() -> bool,
  ) {
    if idle_state.rounds < ROUNDS_UNTIL_SLEEPY {
      thread::yield_now();
      idle_state.rounds += 1;
    } else if idle_state.rounds == ROUNDS_UNTIL_SLEEPY {
      idle_state.jobs_counter = self.announce_sleepy();
      idle_state.rounds += 1;
      thread::yield_now();
    } else if idle_state.rounds < ROUNDS_UNTIL_SLEEPING {
      idle_state.rounds += 1;
      thread::yield_now();
    } else {
      self.sleep(idle_state, latch, has_injected_jobs);
    }
  }

  fn announce_sleepy(&self) -> JobsEventCounter {
    self
      .counters
      .increment_jobs_event_counter_if(JobsEventCounter::is_active)
      .jobs_counter()
  }

  fn sleep(
    &self,
    idle_state: &mut IdleState,
    latch: &CoreLatch,
    has_injected_jobs: impl FnOnce() -> bool,
  ) {
    let worker_index = idle_state.worker_index;

    if !latch.get_sleepy() {
      return;
    }

    let sleep_state = &self.worker_sleep_states[worker_index];
    let mut is_sleep = sleep_state.is_sleep.lock().unwrap();
    debug_assert!(!*is_sleep);

    // Our latch was signalled. We should wake back up fully as we
    // will have some stuff to do.
    if !latch.fall_asleep() {
      idle_state.wake_fully();
      return;
    }

    loop {
      let counters = self.counters.load(Ordering::SeqCst);

      // Check if the JEC has changed since we got sleepy.
      debug_assert!(idle_state.jobs_counter.is_sleepy());
      if counters.jobs_counter() != idle_state.jobs_counter {
        // JEC has changed, so a new job was posted, but for some reason
        // we didn't see it. We should return to just before the SLEEPY
        // state so we can do another search and (if we fail to find
        // work) go back to sleep.
        idle_state.wake_partly();
        latch.wake_up();
        return;
      }

      // Otherwise, let's move from IDLE to SLEEPING.
      if self.counters.try_add_sleeping_thread(counters) {
        break;
      }
    }

    // Successfully registered as asleep.

    // We have one last check for injected jobs to do. This protects against
    // deadlock in the very unlikely event that
    //
    // - an external job is being injected while we are sleepy
    // - that job triggers the rollover over the JEC such that we don't see it
    // - we are the last active worker thread
    std::sync::atomic::fence(Ordering::SeqCst);
    if has_injected_jobs() {
      // If we see an externally injected job, then we have to 'wake
      // ourselves up'. (Ordinarily, `sub_sleeping_thread` is invoked by
      // the one that wakes us.)
      self.counters.sub_sleeping_thread();
    } else {
      // If we don't see an injected job (the normal case), then flag
      // ourselves as asleep and wait till we are notified.
      //
      // (Note that `is_sleep` is held under a mutex and the mutex was
      // acquired *before* we incremented the "sleepy counter". This means
      // that whomever is coming to wake us will have to wait until we
      // release the mutex in the call to `wait`, so they will see this
      // boolean as true.)
      *is_sleep = true;
      while *is_sleep {
        is_sleep = sleep_state.condvar.wait(is_sleep).unwrap();
      }
    }

    // Update other state:
    idle_state.wake_fully();
    latch.wake_up();
  }

  pub fn new_injected_jobs(&self, num_jobs: u32, queue_was_empty: bool) {
    std::sync::atomic::fence(Ordering::SeqCst);

    self.new_jobs(num_jobs, queue_was_empty)
  }

  fn new_jobs(&self, num_jobs: u32, queue_was_empty: bool) {
    let counters = self
      .counters
      .increment_jobs_event_counter_if(JobsEventCounter::is_sleepy);
    let num_awake_but_idle = counters.awake_but_idle_threads();
    let num_sleepers = counters.sleeping_threads();

    if num_sleepers == 0 {
      return;
    }

    let num_awake_but_idle = num_awake_but_idle as u32;
    let num_sleepers = num_sleepers as u32;

    if !queue_was_empty {
      let num_to_wake = Ord::min(num_jobs, num_sleepers);
      self.wake_any_threads(num_to_wake);
    } else if num_awake_but_idle < num_jobs {
      let num_to_wake = Ord::min(num_jobs - num_awake_but_idle, num_sleepers);
      self.wake_any_threads(num_to_wake);
    }
  }

  fn wake_any_threads(&self, mut num_to_wake: u32) {
    if num_to_wake > 0 {
      for i in 0..self.worker_sleep_states.len() {
        if self.wake_specific_thread(i) {
          num_to_wake -= 1;
          if num_to_wake == 0 {
            return;
          }
        }
      }
    }
  }

  fn wake_specific_thread(&self, index: usize) -> bool {
    let sleep_state = &self.worker_sleep_states[index];

    let mut is_sleep = sleep_state.is_sleep.lock().unwrap();
    if *is_sleep {
      *is_sleep = false;
      sleep_state.condvar.notify_one();

      self.counters.sub_sleeping_thread();

      true
    } else {
      false
    }
  }
}

impl IdleState {
  fn wake_fully(&mut self) {
    self.rounds = 0;
    self.jobs_counter = JobsEventCounter::DUMMY;
  }

  fn wake_partly(&mut self) {
    self.rounds = ROUNDS_UNTIL_SLEEPY;
    self.jobs_counter = JobsEventCounter::DUMMY;
  }
}
