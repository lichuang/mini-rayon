use std::marker::PhantomData;

use super::noop::NoopReducer;
use crate::iter::ParallelIterator;
use crate::plumbing::Consumer;
use crate::plumbing::Folder;

pub fn for_each<Iter, F, T>(pi: Iter, op: &F)
where
  Iter: ParallelIterator<Item = T>,
  F: Fn(T) + Sync,
  T: Send,
{
  let consumer = ForEachConsumer {
    op,
    _marker: PhantomData,
  };
  pi.drive(consumer)
}

struct ForEachConsumer<'f, F, T> {
  op: &'f F,
  _marker: PhantomData<T>,
}

impl<'f, F, T> ForEachConsumer<'f, F, T>
where F: Fn(T) + Sync
{
  fn split_off_left(&self) -> Self {
    ForEachConsumer {
      op: self.op,
      _marker: PhantomData,
    }
  }
}

impl<'f, F, T> Consumer<T> for ForEachConsumer<'f, F, T>
where
  F: Fn(T) + Sync,
  T: Send,
{
  type Result = ();
  type Folder = ForEachConsumer<'f, F, T>;
  type Reducer = NoopReducer;

  fn split_at(self, _index: usize) -> (Self, Self, NoopReducer) {
    (self.split_off_left(), self, NoopReducer)
  }

  fn full(&self) -> bool {
    false
  }

  fn into_folder(self) -> Self {
    self
  }
}

impl<'f, F, T> Folder<T> for ForEachConsumer<'f, F, T>
where
  F: Fn(T) + Sync,
  T: Send,
{
  type Result = ();

  fn consume(self, item: T) -> Self {
    (self.op)(item);
    self
  }

  fn consume_iter<I>(self, iter: I) -> Self
  where I: IntoIterator<Item = T> {
    iter.into_iter().for_each(self.op);
    self
  }

  fn complete(self) {}

  fn full(&self) -> bool {
    false
  }
}
