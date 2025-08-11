use crate::for_each;
use crate::plumbing::Consumer;
use crate::plumbing::ProducerCallback;

pub trait IntoParallelIteratorator {
  type Item;
  type Iter: ParallelIterator<Item = Self::Item>;

  fn par_iter(self) -> Self::Iter;
}

pub trait ParallelIterator: Sized {
  type Item;

  fn for_each<Op>(self, op: Op)
  where Op: Fn(Self::Item) {
    for_each(self, op);
  }

  fn len(&self) -> usize;

  fn drive<C>(self, c: C) -> C::Result
  where C: Consumer<Self::Item>;

  fn with_producer<CB: ProducerCallback<Self::Item>>(self, callback: CB) -> CB::Output;
}
