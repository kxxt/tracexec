use std::{
  borrow::Cow,
  fmt::{
    Debug,
    Display,
  },
  hash::BuildHasher,
  ops::Deref,
  sync::{
    Arc,
    LazyLock,
    Weak,
  },
};

use hashbrown::{
  DefaultHashBuilder,
  HashTable,
  hash_table::Entry,
};
use parking_lot::Mutex;
use serde::{
  Serialize,
  Serializer,
};

#[repr(transparent)]
#[derive(Clone)]
pub struct ArcStr(Arc<InternedString>);

struct InternedString {
  text: Arc<String>,
  hash: u64,
}

type StringTable = HashTable<(Arc<String>, Weak<InternedString>)>;

struct StringInterner {
  hasher: DefaultHashBuilder,
  shards: [Mutex<StringTable>; 32],
}

static INTERNER: LazyLock<StringInterner> = LazyLock::new(|| StringInterner {
  hasher: DefaultHashBuilder::default(),
  shards: std::array::from_fn(|_| Mutex::new(HashTable::new())),
});

impl StringInterner {
  fn shard(&self, hash: u64) -> &Mutex<StringTable> {
    // Leave the low hash bits independent: HashTable uses them to choose buckets.
    &self.shards[(hash >> 32) as usize % self.shards.len()]
  }

  fn intern(&self, text: Cow<'_, str>) -> ArcStr {
    let hash = self.hasher.hash_one(text.as_ref());
    let mut strings = self.shard(hash).lock();
    let entry = strings.entry(
      hash,
      |(key, _)| key.as_str() == text.as_ref(),
      |(key, _)| self.hasher.hash_one(key.as_str()),
    );
    match entry {
      Entry::Occupied(mut entry) => {
        if let Some(string) = entry.get().1.upgrade() {
          return ArcStr(string);
        }
        // The last handle may be dropping while waiting for this shard's lock.
        // Reuse its text, but give the replacement its own lifetime and identity.
        let string = Arc::new(InternedString {
          text: entry.get().0.clone(),
          hash,
        });
        entry.get_mut().1 = Arc::downgrade(&string);
        ArcStr(string)
      }
      Entry::Vacant(entry) => {
        let text = Arc::new(text.into_owned());
        let string = Arc::new(InternedString {
          text: text.clone(),
          hash,
        });
        // Keep the lookup key separately from the weak handle so comparing keys
        // never drops a last strong handle while holding the shard's lock.
        entry.insert((text, Arc::downgrade(&string)));
        ArcStr(string)
      }
    }
  }
}

impl Drop for InternedString {
  fn drop(&mut self) {
    let mut strings = INTERNER.shard(self.hash).lock();
    // A concurrent lookup can replace an expired weak handle before this Drop
    // acquires the lock. Only remove our own entry, never that replacement.
    if let Ok(entry) = strings.find_entry(self.hash, |(_, weak)| std::ptr::eq(weak.as_ptr(), self))
    {
      entry.remove();
    }
  }
}

impl ArcStr {
  pub fn as_str(&self) -> &str {
    self.0.text.as_str()
  }
}

impl Deref for ArcStr {
  type Target = str;

  fn deref(&self) -> &Self::Target {
    self.as_str()
  }
}

impl AsRef<str> for ArcStr {
  fn as_ref(&self) -> &str {
    self.as_str()
  }
}

impl<T> PartialEq<T> for ArcStr
where
  T: AsRef<str>,
{
  fn eq(&self, other: &T) -> bool {
    self.as_str() == other.as_ref()
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
    serializer.serialize_str(self.as_str())
  }
}

impl std::hash::Hash for ArcStr {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    // Equal live strings share an allocation, including across constructors.
    Arc::as_ptr(&self.0).hash(state);
  }
}

impl PartialOrd for ArcStr {
  fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
    Some(self.cmp(other))
  }
}

impl Ord for ArcStr {
  fn cmp(&self, other: &Self) -> std::cmp::Ordering {
    self.as_str().cmp(other.as_str())
  }
}

impl Display for ArcStr {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    Display::fmt(self.as_str(), f)
  }
}

impl Debug for ArcStr {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    Debug::fmt(self.as_str(), f)
  }
}

impl From<&str> for ArcStr {
  fn from(value: &str) -> Self {
    INTERNER.intern(Cow::Borrowed(value))
  }
}

impl Default for ArcStr {
  fn default() -> Self {
    DEFAULT_ARCSTR.clone()
  }
}

static DEFAULT_ARCSTR: LazyLock<ArcStr> = LazyLock::new(|| ArcStr::from(""));

#[derive(Default)]
pub struct StringCache;

impl StringCache {
  pub fn new() -> Self {
    Self
  }

  pub fn get_or_insert(&self, s: &str) -> ArcStr {
    INTERNER.intern(Cow::Borrowed(s))
  }

  pub fn get_or_insert_owned(&self, s: String) -> ArcStr {
    INTERNER.intern(Cow::Owned(s))
  }
}

#[cfg(test)]
mod tests {
  use std::{
    hash::{
      BuildHasher,
      DefaultHasher,
      Hash,
      Hasher,
    },
    sync::{
      Arc,
      Barrier,
      Weak,
    },
    thread,
  };

  use super::{
    ArcStr,
    INTERNER,
    StringCache,
  };

  fn hash(value: &ArcStr) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
  }

  fn is_interned(text: &str) -> bool {
    let hash = INTERNER.hasher.hash_one(text);
    INTERNER
      .shard(hash)
      .lock()
      .find(hash, |(key, _)| key.as_str() == text)
      .is_some()
  }

  #[test]
  fn constructors_share_strings_and_preserve_value_semantics() -> serde_json::Result<()> {
    let owned = String::from("cache constructor test: λ\n");
    let allocation = owned.as_ptr();
    let first = StringCache::new().get_or_insert_owned(owned);
    let second = StringCache::new().get_or_insert(first.as_str());
    let third = ArcStr::from(first.as_str());

    assert_eq!(
      first.as_ptr(),
      allocation,
      "owned misses must reuse the string buffer"
    );
    assert!(Arc::ptr_eq(&first.0, &second.0));
    assert!(Arc::ptr_eq(&first.0, &third.0));
    assert_eq!(hash(&first), hash(&third));
    assert_eq!(first, "cache constructor test: λ\n");
    assert_eq!(first.to_string(), first.as_str());
    assert_eq!(format!("{first:?}"), format!("{:?}", first.as_str()));
    assert_eq!(
      serde_json::to_string(&first)?,
      serde_json::to_string(first.as_str())?
    );
    let lower = ArcStr::from("aaa");
    let upper = ArcStr::from("zzz");
    assert!(lower < upper);
    assert!(Arc::ptr_eq(&ArcStr::default().0, &ArcStr::from("").0));
    Ok(())
  }

  #[test]
  fn last_handle_reclaims_the_string_and_cache_entry() {
    let text = "cache last-handle reclamation test";
    let first = ArcStr::from(text);
    let second = first.clone();
    let allocation = Arc::downgrade(&first.0.text);
    drop(first);
    assert!(is_interned(text));
    assert!(allocation.upgrade().is_some());

    drop(second);
    assert!(!is_interned(text));
    assert!(allocation.upgrade().is_none());

    let replacement = ArcStr::from(text);
    assert_eq!(replacement, text);
    drop(replacement);
    assert!(!is_interned(text));
  }

  #[test]
  fn expired_entry_replacement_survives_the_old_strings_cleanup() {
    let text = "cache expired-entry replacement test";
    let old = ArcStr::from(text);
    {
      let mut strings = INTERNER.shard(old.0.hash).lock();
      let entry = strings.find_mut(old.0.hash, |(key, _)| key.as_str() == text);
      let Some((_, weak)) = entry else {
        panic!("the live string must have a cache entry");
      };
      // Model an expired weak handle whose destructor has not acquired the
      // shard lock yet. Keep the old object to run that cleanup deterministically.
      *weak = Weak::new();
    }
    let replacement = ArcStr::from(text);
    assert!(!Arc::ptr_eq(&old.0, &replacement.0));
    assert!(Arc::ptr_eq(&old.0.text, &replacement.0.text));
    drop(old);
    let hit = ArcStr::from(text);
    assert!(Arc::ptr_eq(&replacement.0, &hit.0));
    drop(hit);
    drop(replacement);
    assert!(!is_interned(text));
  }

  #[test]
  fn concurrent_lookup_and_last_drop_preserve_interning() {
    let text = "cache concurrent lookup and last-drop test";
    let barrier = Barrier::new(8);
    thread::scope(|scope| {
      for _ in 0..8 {
        let barrier = &barrier;
        scope.spawn(move || {
          let cache = StringCache::new();
          barrier.wait();
          for _ in 0..2000 {
            let first = cache.get_or_insert(text);
            let second = cache.get_or_insert_owned(text.to_owned());
            let third = ArcStr::from(text);
            assert!(Arc::ptr_eq(&first.0, &second.0));
            assert!(Arc::ptr_eq(&first.0, &third.0));
            assert_eq!(hash(&first), hash(&third));
          }
        });
      }
    });
    assert!(!is_interned(text));
  }
}
