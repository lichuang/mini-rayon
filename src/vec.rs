use std::mem;
use std::ptr;
use std::slice;

use crate::ParallelIterator;
use crate::iter::IntoParallelIteratorator;
use crate::plumbing::Consumer;
use crate::plumbing::Producer;
use crate::plumbing::ProducerCallback;
use crate::plumbing::bridge;

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
    bridge(self, consumer)
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

impl<'data, T> Producer for DrainProducer<'data, T> {
  type Item = T;
  type IntoIter = SliceDrain<'data, T>;

  fn into_iter(mut self) -> Self::IntoIter {
    let slice = mem::take(&mut self.slice);
    SliceDrain {
      iter: slice.iter_mut(),
    }
  }
}

// like std::vec::Drain, without updating a source Vec
pub struct SliceDrain<'data, T> {
  iter: slice::IterMut<'data, T>,
}

impl<'data, T: 'data> Iterator for SliceDrain<'data, T> {
  type Item = T;

  fn next(&mut self) -> Option<T> {
    let ptr: *const T = self.iter.next()?;
    Some(unsafe { ptr::read(ptr) })
  }

  fn size_hint(&self) -> (usize, Option<usize>) {
    self.iter.size_hint()
  }

  fn count(self) -> usize {
    self.iter.len()
  }
}
