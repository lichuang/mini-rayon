use std::ops::RangeBounds;

use crate::for_each;
use crate::plumbing::Consumer;
use crate::plumbing::ProducerCallback;

pub trait IntoParallelIteratorator {
  type Item;
  type Iter: ParallelIterator<Item = Self::Item>;

  fn par_iter(self) -> Self::Iter;
}

pub trait ParallelIterator: Sized + Send {
  type Item: Send;

  fn for_each<Op>(self, op: Op)
  where Op: Fn(Self::Item) + Sync + Send {
    for_each(self, &op);
  }

  fn len(&self) -> usize;

  fn drive<C>(self, c: C) -> C::Result
  where C: Consumer<Self::Item>;

  fn with_producer<CB: ProducerCallback<Self::Item>>(self, callback: CB) -> CB::Output;
}

pub trait ParallelDrainRange<Idx = usize> {
  type Iter: ParallelIterator<Item = Self::Item>;

  type Item: Send;

  fn par_drain<R: RangeBounds<Idx>>(self, range: R) -> Self::Iter;
}
