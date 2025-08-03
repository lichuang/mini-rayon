pub trait ProducerCallback<T> {
  type Output;

  fn callback<P>(self, producer: P) -> Self::Output
  where P: Producer<Item = T>;
}

pub trait Producer {
  type Item;
}

pub trait Consumer<Item> {
  type Result;
}
