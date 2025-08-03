use crate::ParallelIterator;
use crate::plumbing::Consumer;

pub fn bridge<I, C>(pi: I, consumer: C) -> C::Result
where
  I: ParallelIterator,
  C: Consumer<I::Item>,
{
  let len = pi.len();
  unimplemented!()
}
