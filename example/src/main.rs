use mini_rayon::prelude::IntoParallelIteratorator;
use mini_rayon::prelude::ParallelIterator;

fn main() {
  let vec = vec![1, 2, 3, 4, 5];
  vec.par_iter().for_each(|item| println!("item: {:?}", item));
}
