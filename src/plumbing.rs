use std::iter::Product;

use crate::ParallelIterator;

pub trait ProducerCallback<T> {
  type Output;

  fn callback<P>(self, producer: P) -> Self::Output
  where P: Producer<Item = T>;
}

pub trait Producer {
  type Item;
  type IntoIter: Iterator<Item = Self::Item>;

  fn into_iter(self) -> Self::IntoIter;
}

pub trait Consumer<Item> {
  type Result;

  fn full(&self) -> bool;
}

pub fn bridge<I, C>(pi: I, consumer: C) -> C::Result
where
  I: ParallelIterator,
  C: Consumer<I::Item>,
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
  unimplemented!()
}
