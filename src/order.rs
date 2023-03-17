#[derive(Debug, Copy, Clone)]
pub enum Order {
  EQ, LT, GT
}

pub fn total<T: Ord + Copy + Clone>(left: T, right: T) -> Order {
  if left == right { Order::EQ } else {
  if left < right { Order::LT } else {
  Order::GT }}
}