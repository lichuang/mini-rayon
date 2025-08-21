use std::mem;
use std::ops::Range;
use std::ops::RangeBounds;
use std::ptr;
use std::slice;

use crate::ParallelIterator;
use crate::iter::IntoParallelIteratorator;
use crate::iter::ParallelDrainRange;
use crate::math::simplify_range;
use crate::plumbing::Consumer;
use crate::plumbing::Producer;
use crate::plumbing::ProducerCallback;
use crate::plumbing::bridge;

impl<T: Send> IntoParallelIteratorator for Vec<T> {
  type Item = T;
  type Iter = IntoIter<T>;

  fn par_iter(self) -> Self::Iter {
    IntoIter { vec: self }
  }
}

pub struct IntoIter<T: Send> {
  vec: Vec<T>,
}

impl<T: Send> ParallelIterator for IntoIter<T> {
  type Item = T;

  fn len(&self) -> usize {
    self.vec.len()
  }

  fn drive<C>(self, consumer: C) -> C::Result
  where C: Consumer<Self::Item> {
    bridge(self, consumer)
  }

  fn with_producer<CB: ProducerCallback<Self::Item>>(mut self, callback: CB) -> CB::Output
  where CB: ProducerCallback<Self::Item> {
    // self.vec.drain(..)
    unimplemented!()
  }
}

impl<'data, T: Send> ParallelDrainRange<usize> for &'data mut Vec<T> {
  type Iter = Drain<'data, T>;
  type Item = T;

  fn par_drain<R: RangeBounds<usize>>(self, range: R) -> Self::Iter {
    Drain {
      orig_len: self.len(),
      range: simplify_range(range, self.len()),
      vec: self,
    }
  }
}

pub struct Drain<'data, T: Send> {
  vec: &'data mut Vec<T>,
  range: Range<usize>,
  orig_len: usize,
}

impl<'data, T: Send> ParallelIterator for Drain<'data, T> {
  type Item = T;

  fn len(&self) -> usize {
    self.vec.len()
  }

  fn drive<C>(self, consumer: C) -> C::Result
  where C: Consumer<Self::Item> {
    bridge(self, consumer)
  }

  fn with_producer<CB>(self, callback: CB) -> CB::Output
  where CB: ProducerCallback<Self::Item> {
    unsafe {
      self.vec.set_len(self.range.start);

      let producer = DrainProducer::from_vec(self.vec, self.range.len());

      callback.callback(producer)
    }
  }
}

pub struct DrainProducer<'data, T: Send> {
  slice: &'data mut [T],
}

impl<T: Send> DrainProducer<'_, T> {
  pub(crate) unsafe fn new(slice: &mut [T]) -> DrainProducer<'_, T> {
    DrainProducer { slice }
  }

  pub unsafe fn from_vec(vec: &mut Vec<T>, len: usize) -> DrainProducer<'_, T> {
    let ptr = vec.as_mut_ptr();

    DrainProducer {
      slice: slice::from_raw_parts_mut(ptr, len),
    }
  }
}

impl<'data, T: Send> Producer for DrainProducer<'data, T> {
  type Item = T;
  type IntoIter = SliceDrain<'data, T>;

  fn into_iter(mut self) -> Self::IntoIter {
    let slice = mem::take(&mut self.slice);
    SliceDrain {
      iter: slice.iter_mut(),
    }
  }

  fn split_at(mut self, index: usize) -> (Self, Self) {
    // replace the slice so we don't drop it twice
    let slice = mem::take(&mut self.slice);
    let (left, right) = slice.split_at_mut(index);
    unsafe { (DrainProducer::new(left), DrainProducer::new(right)) }
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
