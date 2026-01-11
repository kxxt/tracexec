use std::ops::{
  Add,
  Sub,
};

use nutype::nutype;

#[nutype(derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize))]
pub struct EventId(u64);

impl Add<u64> for EventId {
  type Output = Self;

  fn add(self, rhs: u64) -> Self::Output {
    Self::new(self.into_inner() + rhs)
  }
}

impl Sub<u64> for EventId {
  type Output = Self;

  fn sub(self, rhs: u64) -> Self::Output {
    Self::new(self.into_inner().saturating_sub(rhs))
  }
}

impl EventId {
  pub fn zero() -> Self {
    Self::new(0)
  }
}

#[cfg(test)]
mod tests {
  use super::EventId;

  #[test]
  fn test_zero() {
    let zero = EventId::zero();
    assert_eq!(zero.into_inner(), 0);
  }

  #[test]
  fn test_add() {
    let id = EventId::new(10);
    let new_id = id + 5;
    assert_eq!(new_id.into_inner(), 15);

    // Original should not change
    assert_eq!(id.into_inner(), 10);
  }

  #[test]
  fn test_sub() {
    let id = EventId::new(10);
    let new_id = id - 3;
    assert_eq!(new_id.into_inner(), 7);

    // Original should not change
    assert_eq!(id.into_inner(), 10);
  }

  #[test]
  fn test_sub_saturating() {
    let id = EventId::new(5);
    let new_id = id - 10; // Should saturate to 0
    assert_eq!(new_id.into_inner(), 0);
  }

  #[test]
  fn test_equality_and_ordering() {
    let a = EventId::new(1);
    let b = EventId::new(2);
    assert!(a < b);
    assert!(b > a);
    assert_eq!(a, EventId::new(1));
  }

  #[test]
  fn test_clone_copy() {
    let id = EventId::new(42);
    let c1 = id;
    let c2 = id.clone();
    assert_eq!(c1, c2);
  }
}
