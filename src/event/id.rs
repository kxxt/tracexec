use std::ops::{Add, Sub};

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
