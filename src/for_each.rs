use crate::ParallelIterator;

pub fn for_each<I, Op, T>(pi: I, op: Op)
where
  I: ParallelIterator<Item = T>,
  Op: Fn(T),
{
  let consumer = ForEachConsumer { op };
}

struct ForEachConsumer<F> {
  op: F,
}
