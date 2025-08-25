use std::ops::RangeBounds;

use crate::for_each;
use crate::plumbing::Consumer;
use crate::plumbing::ProducerCallback;

pub trait IntoParallelIteratorator {
  type Item;
  type Iter: ParallelIterator<Item = Self::Item>;

  fn into_par_iter(self) -> Self::Iter;
}

pub trait ParallelIterator: Sized + Send {
  type Item: Send;

  fn for_each<Op>(self, op: Op)
  where Op: Fn(Self::Item) + Sync + Send {
    for_each(self, &op);
  }

  fn len(&self) -> usize;

  /// Internal method used to define the behavior of this parallel
  /// iterator. You should not need to call this directly.
  ///
  /// This method causes the iterator `self` to start producing
  /// items and to feed them to the consumer `consumer` one by one.
  /// It may split the consumer before doing so to create the
  /// opportunity to produce in parallel. If a split does happen, it
  /// will inform the consumer of the index where the split should
  /// occur.
  fn drive<C>(self, consumer: C) -> C::Result
  where C: Consumer<Self::Item>;

  fn with_producer<CB: ProducerCallback<Self::Item>>(self, callback: CB) -> CB::Output;
}

/// `ParallelDrainRange` creates a parallel iterator that moves a range of items
/// from a collection while retaining the original capacity.
pub trait ParallelDrainRange<Idx = usize> {
  type Iter: ParallelIterator<Item = Self::Item>;

  type Item: Send;

  fn par_drain<R: RangeBounds<Idx>>(self, range: R) -> Self::Iter;
}
