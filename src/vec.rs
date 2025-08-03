use std::slice;

use crate::ParallelIterator;
use crate::bridge;
use crate::iter::IntoParallelIteratorator;
use crate::plumbing::Consumer;
use crate::plumbing::Producer;
use crate::plumbing::ProducerCallback;

impl<T> IntoParallelIteratorator for Vec<T> {
  type Item = T;
  type Iter = IntoIter<T>;

  fn par_iter(self) -> Self::Iter {
    IntoIter { vec: self }
  }
}

pub struct IntoIter<T> {
  vec: Vec<T>,
}

impl<T> ParallelIterator for IntoIter<T> {
  type Item = T;

  fn len(&self) -> usize {
    self.vec.len()
  }

  fn drive<C>(self, consumer: C) -> C::Result
  where C: Consumer<Self::Item> {
    bridge::bridge(self, consumer)
  }

  fn with_producer<CB: ProducerCallback<Self::Item>>(mut self, callback: CB) -> CB::Output {
    unsafe {
      let producer = DrainProducer::from_vec(&mut self.vec);
      callback.callback(producer)
    }
  }
}

pub struct DrainProducer<'data, T> {
  slice: &'data mut [T],
}

impl<T> DrainProducer<'_, T> {
  pub unsafe fn from_vec(vec: &mut Vec<T>) -> DrainProducer<'_, T> {
    let ptr = vec.as_mut_ptr();

    DrainProducer {
      slice: slice::from_raw_parts_mut(ptr, vec.len()),
    }
  }
}

impl<T> Producer for DrainProducer<'_, T> {
  type Item = T;
}
