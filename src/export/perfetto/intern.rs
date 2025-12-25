use std::num::NonZeroUsize;

use perfetto_trace_proto::{
  DebugAnnotation, DebugAnnotationName, EventName, InternedString,
  debug_annotation::{NameField, Value},
};
use strum::{EnumIter, IntoEnumIterator, IntoStaticStr};


#[repr(u64)]
#[derive(Debug, Clone, Copy, IntoStaticStr, EnumIter)]
#[strum(serialize_all = "snake_case")]
pub enum DebugAnnotationInternId {
  Argv = 1,
  Filename,
  Cwd,
  SyscallRet,
  EndReason,
  ExitCode,
  ExitSignal,
  Cmdline,
}

impl DebugAnnotationInternId {
  pub fn interned_data() -> Vec<DebugAnnotationName> {
    Self::iter()
      .map(|v| {
        let name: &'static str = v.into();
        DebugAnnotationName {
          iid: Some(v as _),
          name: Some(name.to_string()),
        }
      })
      .collect()
  }

  pub fn with_string(self, value: String) -> DebugAnnotation {
    DebugAnnotation {
      value: Some(Value::StringValue(value)),
      name_field: Some(NameField::NameIid(self as _)),
      ..Default::default()
    }
  }

  pub fn with_interned_string(self, value: InternedId) -> DebugAnnotation {
    DebugAnnotation {
      value: Some(Value::StringValueIid(value)),
      name_field: Some(NameField::NameIid(self as _)),
      ..Default::default()
    }
  }

  pub fn with_array(self, value: Vec<DebugAnnotation>) -> DebugAnnotation {
    DebugAnnotation {
      array_values: value,
      name_field: Some(NameField::NameIid(self as _)),
      ..Default::default()
    }
  }

  pub fn with_int(self, value: i64) -> DebugAnnotation {
    DebugAnnotation {
      value: Some(Value::IntValue(value)),
      name_field: Some(NameField::NameIid(self as _)),
      ..Default::default()
    }
  }
}

type InternedId = u64;

/// A value that should be included in the intern table of the trace packet
pub struct InternedValue {
  pub iid: InternedId,
  pub value: String,
}

impl From<InternedValue> for InternedString {
  fn from(value: InternedValue) -> Self {
    Self {
      iid: Some(value.iid),
      str: Some(value.value.into_bytes()),
    }
  }
}

impl From<InternedValue> for EventName {
  fn from(value: InternedValue) -> Self {
    Self {
      iid: Some(value.iid),
      name: Some(value.value),
    }
  }
}

pub struct ValueInterner {
  /// The iid counter
  iid: InternedId,
  /// The cache
  cache: lru::LruCache<String, InternedId, hashbrown::DefaultHashBuilder>,
}

impl ValueInterner {
  pub fn new(max_cap: NonZeroUsize) -> Self {
    Self {
      iid: 1,
      cache: lru::LruCache::new(max_cap),
    }
  }

  /// Try to intern a string, if already interned, the iid is returned.
  /// Otherwise we intern it and return the value to be inserted into intern table
  pub fn intern(&mut self, msg: &str) -> Result<InternedId, InternedValue> {
    if let Some(v) = self.cache.get(msg) {
      Ok(*v)
    } else {
      let s = msg.to_owned();
      let iid = self.iid;
      self.iid += 1;
      // Unfortunately we must clone the string for inserting it into the intern table.
      self.cache.put(s.clone(), iid);
      Err(InternedValue { iid, value: s })
    }
  }

  pub fn intern_with(
    &mut self,
    msg: &str,
    table: &mut Vec<impl From<InternedValue>>,
  ) -> InternedId {
    match self.intern(msg) {
      Ok(iid) => iid,
      Err(value) => {
        let iid = value.iid;
        table.push(value.into());
        iid
      }
    }
  }
}

pub fn da_interned_string(iid: InternedId) -> DebugAnnotation {
  DebugAnnotation {
    value: Some(Value::StringValueIid(iid)),
    ..Default::default()
  }
}
