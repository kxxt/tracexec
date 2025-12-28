use std::{borrow::Cow, sync::LazyLock};

use chrono::{DateTime, Local};
use nutype::nutype;

#[nutype(
  validate(with = validate_strftime, error = Cow<'static,str>),
  derive(Debug, Clone, Serialize, Deserialize, Deref, FromStr)
)]
pub struct TimestampFormat(String);

fn validate_strftime(fmt: &str) -> Result<(), Cow<'static, str>> {
  if fmt.contains("\n") {
    return Err("inline timestamp format string should not contain newline(s)".into());
  }
  Ok(())
}

pub type Timestamp = DateTime<Local>;

pub fn ts_from_boot_ns(boot_ns: u64) -> Timestamp {
  DateTime::from_timestamp_nanos((*BOOT_TIME + boot_ns) as i64).into()
}

static BOOT_TIME: LazyLock<u64> = LazyLock::new(|| {
  let content = std::fs::read_to_string("/proc/stat").expect("Failed to read /proc/stat");
  for line in content.lines() {
    if line.starts_with("btime") {
      return line
        .split(' ')
        .nth(1)
        .unwrap()
        .parse::<u64>()
        .expect("Failed to parse btime in /proc/stat")
        * 1_000_000_000;
    }
  }
  panic!("btime is not available in /proc/stat. Am I running on Linux?")
});
