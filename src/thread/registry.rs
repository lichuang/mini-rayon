use std::ptr;
use std::sync::Arc;
use std::sync::Once;

use anyhow::Result;

use super::worker::WorkerThread;

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

pub struct Registry {}

impl Registry {
  pub fn new(builder: &ThreadPoolBuilder) -> Result<Arc<Registry>> {
    Ok(Arc::new(Registry {}))
  }

  pub fn in_worker<OP, R>(&self, op: OP) -> R
  where
    OP: FnOnce(&WorkerThread, bool) -> R,
    R: Send,
  {
    unimplemented!()
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
  OP: FnOnce(&WorkerThread, bool) -> R,
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
