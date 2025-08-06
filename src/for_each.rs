use crate::ParallelIterator;
use crate::plumbing::Consumer;

pub fn for_each<I, Op, T>(pi: I, op: Op)
where
  I: ParallelIterator<Item = T>,
  Op: Fn(T),
{
  let consumer = ForEachConsumer { op };
  pi.drive(consumer)
}

struct ForEachConsumer<F> {
  op: F,
}

impl<F, T> Consumer<T> for ForEachConsumer<F>
where F: Fn(T)
{
  type Result = ();

  fn full(&self) -> bool {
    false
  }
}
