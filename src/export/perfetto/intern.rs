use perfetto_trace_proto::{
  DebugAnnotation, DebugAnnotationName,
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
