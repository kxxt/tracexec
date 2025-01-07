use serde::{Serialize, Serializer};
use std::{
  borrow::Cow,
  fmt::{Debug, Display},
  ops::Deref,
  sync::{Arc, LazyLock, Weak},
};
use weak_table::WeakHashSet;

#[derive(Clone)]
#[repr(transparent)]
pub struct ArcStr(Arc<str>);

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
    &*self.0 == other.as_ref()
  }
}

impl Eq for ArcStr {}

impl From<Arc<str>> for ArcStr {
  fn from(value: Arc<str>) -> Self {
    Self(value)
  }
}

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
    Some(self.0.cmp(&other.0))
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
    Arc::<str>::from(value).into()
  }
}

impl Default for ArcStr {
  fn default() -> Self {
    DEFAULT_ARCSTR.clone()
  }
}

static DEFAULT_ARCSTR: LazyLock<ArcStr> = LazyLock::new(|| "".into());

pub struct StringCache {
  inner: WeakHashSet<Weak<str>>,
}

impl StringCache {
  pub fn new() -> Self {
    Self {
      inner: WeakHashSet::new(),
    }
  }

  pub fn get_or_insert(&mut self, s: &str) -> ArcStr {
    if let Some(s) = self.inner.get(s) {
      s.into()
    } else {
      let arc: Arc<str> = Arc::from(s);
      self.inner.insert(arc.clone());
      arc.into()
    }
  }

  // Probably this is not necessary.
  // I don't think we can find a way to turn a String into Arc<str> in-place.
  pub fn get_or_insert_owned(&mut self, s: String) -> ArcStr {
    if let Some(s) = self.inner.get(s.as_str()) {
      s.into()
    } else {
      let arc: Arc<str> = Arc::from(s);
      self.inner.insert(arc.clone());
      arc.into()
    }
  }
}
