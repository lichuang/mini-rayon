mod containers;
mod core;
mod functions;
mod iter;
mod plumbing;

pub(crate) use functions::for_each;
pub use iter::ParallelIterator;
