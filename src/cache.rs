use std::{
  collections::HashSet,
  ops::{Deref, DerefMut},
};

use arcstr::ArcStr;

pub struct StringCache {
  cache: HashSet<ArcStr>,
}

impl StringCache {
  pub fn new() -> Self {
    Self {
      cache: HashSet::new(),
    }
  }

  pub fn get_or_insert_owned(&mut self, s: String) -> ArcStr {
    if let Some(s) = self.cache.get(s.as_str()) {
      s.clone()
    } else {
      let arc = ArcStr::from(s);
      self.cache.insert(arc.clone());
      arc
    }
  }

  pub fn get_or_insert(&mut self, s: &str) -> ArcStr {
    if let Some(s) = self.cache.get(s) {
      s.clone()
    } else {
      let arc = ArcStr::from(s);
      self.cache.insert(arc.clone());
      arc
    }
  }
}

impl Deref for StringCache {
  type Target = HashSet<ArcStr>;

  fn deref(&self) -> &Self::Target {
    &self.cache
  }
}

impl DerefMut for StringCache {
  fn deref_mut(&mut self) -> &mut Self::Target {
    &mut self.cache
  }
}
