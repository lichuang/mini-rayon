mod containers;
mod functions;
mod iter;
mod plumbing;
mod thread;

pub(crate) use functions::for_each;
pub use iter::ParallelIterator;
