use internment::ArcIntern;
use serde::{Serialize, Serializer};
use std::{
  borrow::Cow,
  fmt::{Debug, Display},
  ops::Deref,
  sync::LazyLock,
};

#[repr(transparent)]
#[derive(Clone)]
pub struct ArcStr(ArcIntern<String>);

impl ArcStr {
  pub fn as_str(&self) -> &str {
    &self.0
  }
}

impl Deref for ArcStr {
  type Target = str;

  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

impl AsRef<str> for ArcStr {
  fn as_ref(&self) -> &str {
    &self.0
  }
}

impl<T> PartialEq<T> for ArcStr
where
  T: AsRef<str>,
{
  fn eq(&self, other: &T) -> bool {
    *self.0 == other.as_ref()
  }
}

impl Eq for ArcStr {}

impl From<ArcStr> for Cow<'_, str> {
  fn from(value: ArcStr) -> Self {
    Self::Owned(value.to_string())
  }
}

impl Serialize for ArcStr {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    serializer.serialize_str(&self.0)
  }
}

impl std::hash::Hash for ArcStr {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    self.0.hash(state);
  }
}

impl PartialOrd for ArcStr {
  fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
    Some(self.cmp(other))
  }
}

impl Ord for ArcStr {
  fn cmp(&self, other: &Self) -> std::cmp::Ordering {
    self.0.cmp(&other.0)
  }
}

impl Display for ArcStr {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    Display::fmt(&self.0, f)
  }
}

impl Debug for ArcStr {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    Debug::fmt(&self.0, f)
  }
}

impl From<&str> for ArcStr {
  fn from(value: &str) -> Self {
    Self(ArcIntern::from_ref(value))
  }
}

impl Default for ArcStr {
  fn default() -> Self {
    DEFAULT_ARCSTR.clone()
  }
}

static DEFAULT_ARCSTR: LazyLock<ArcStr> = LazyLock::new(|| ArcStr(ArcIntern::from_ref("")));

#[derive(Default)]
pub struct StringCache;

impl StringCache {
  pub fn new() -> Self {
    Self
  }

  pub fn get_or_insert(&self, s: &str) -> ArcStr {
    ArcStr(ArcIntern::from_ref(s))
  }

  // Probably this is not necessary.
  // I don't think we can find a way to turn a String into Arc<str> in-place.
  pub fn get_or_insert_owned(&self, s: String) -> ArcStr {
    ArcStr(ArcIntern::new(s))
  }
}
