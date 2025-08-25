use crate::core::current_num_threads;
use crate::core::join_context;
use crate::iter::ParallelIterator;

pub trait ProducerCallback<T> {
  type Output;

  fn callback<P>(self, producer: P) -> Self::Output
  where P: Producer<Item = T>;
}

pub trait Producer: Sized + Send {
  type Item;
  type IntoIter: Iterator<Item = Self::Item>;

  fn into_iter(self) -> Self::IntoIter;

  fn split_at(self, index: usize) -> (Self, Self);

  fn min_len(&self) -> usize {
    1
  }

  fn max_len(&self) -> usize {
    usize::MAX
  }

  fn fold_with<F>(self, folder: F) -> F
  where F: Folder<Self::Item> {
    folder.consume_iter(self.into_iter())
  }
}

pub trait Consumer<Item>: Sized + Send {
  type Result: Send;

  type Reducer: Reducer<Self::Result>;

  type Folder: Folder<Item, Result = Self::Result>;

  fn full(&self) -> bool;

  fn split_at(self, index: usize) -> (Self, Self, Self::Reducer);

  fn into_folder(self) -> Self::Folder;
}

pub trait Folder<Item>: Sized {
  type Result;

  fn consume(self, item: Item) -> Self;

  fn consume_iter<I>(mut self, iter: I) -> Self
  where I: IntoIterator<Item = Item> {
    for item in iter {
      self = self.consume(item);
      if self.full() {
        break;
      }
    }
    self
  }

  fn complete(self) -> Self::Result;

  fn full(&self) -> bool;
}

pub trait Reducer<Result> {
  /// Reduce two final results into one; this is executed after a
  /// split.
  fn reduce(self, left: Result, right: Result) -> Result;
}

#[derive(Clone, Copy)]
struct Splitter {
  splits: usize,
}

impl Splitter {
  #[inline]
  fn new() -> Splitter {
    Splitter {
      splits: current_num_threads(),
    }
  }

  #[inline]
  fn try_split(&mut self, stolen: bool) -> bool {
    let Splitter { splits } = *self;

    if stolen {
      // This job was stolen!  Reset the number of desired splits to the
      // thread count, if that's more than we had remaining anyway.
      self.splits = Ord::max(current_num_threads(), self.splits / 2);
      true
    } else if splits > 0 {
      // We have splits remaining, make it so.
      self.splits /= 2;
      true
    } else {
      // Not stolen, and no more splits -- we're done!
      false
    }
  }
}

#[derive(Clone, Copy)]
struct LengthSplitter {
  inner: Splitter,

  min: usize,
}

impl LengthSplitter {
  #[inline]
  fn new(min: usize, max: usize, len: usize) -> LengthSplitter {
    let mut splitter = LengthSplitter {
      inner: Splitter::new(),
      min: Ord::max(min, 1),
    };

    let min_splits = len / Ord::max(max, 1);

    if min_splits > splitter.inner.splits {
      splitter.inner.splits = min_splits;
    }

    splitter
  }

  #[inline]
  fn try_split(&mut self, len: usize, stolen: bool) -> bool {
    len / 2 >= self.min && self.inner.try_split(stolen)
  }
}

pub fn bridge<Iter, C>(pi: Iter, consumer: C) -> C::Result
where
  Iter: ParallelIterator,
  C: Consumer<Iter::Item>,
{
  let len = pi.len();
  return pi.with_producer(Callback { len, consumer });

  struct Callback<C> {
    len: usize,
    consumer: C,
  }

  impl<C, I> ProducerCallback<I> for Callback<C>
  where C: Consumer<I>
  {
    type Output = C::Result;

    fn callback<P>(self, producer: P) -> Self::Output
    where P: Producer<Item = I> {
      bridge_producer_consumer(self.len, producer, self.consumer)
    }
  }
}

pub fn bridge_producer_consumer<P, C>(len: usize, producer: P, consumer: C) -> C::Result
where
  P: Producer,
  C: Consumer<P::Item>,
{
  let splitter = LengthSplitter::new(producer.min_len(), producer.max_len(), len);
  return helper(len, false, splitter, producer, consumer);

  fn helper<P, C>(
    len: usize,
    migrated: bool,
    mut splitter: LengthSplitter,
    producer: P,
    consumer: C,
  ) -> C::Result
  where
    P: Producer,
    C: Consumer<P::Item>,
  {
    if consumer.full() {
      consumer.into_folder().complete()
    } else if splitter.try_split(len, migrated) {
      let mid = len / 2;
      let (left_producer, right_producer) = producer.split_at(mid);
      let (left_consumer, right_consumer, reducer) = consumer.split_at(mid);
      let (left_result, right_result) = join_context(
        |context| {
          helper(
            mid,
            context.migrated(),
            splitter,
            left_producer,
            left_consumer,
          )
        },
        |context| {
          helper(
            len - mid,
            context.migrated(),
            splitter,
            right_producer,
            right_consumer,
          )
        },
      );
      reducer.reduce(left_result, right_result)
    } else {
      producer.fold_with(consumer.into_folder()).complete()
    }
  }
}
