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

#[cfg(test)]
mod tests {
  use super::*;
  use chrono::{DateTime, Local};
  use std::str::FromStr;

  /* ---------------- TimestampFormat ---------------- */

  #[test]
  fn timestamp_format_accepts_valid_strftime() {
    let fmt = TimestampFormat::from_str("%Y-%m-%d %H:%M:%S");
    assert!(fmt.is_ok());
  }

  #[test]
  fn timestamp_format_rejects_newline() {
    let fmt = TimestampFormat::from_str("%Y-%m-%d\n%H:%M:%S");
    assert!(fmt.is_err());

    let err = fmt.unwrap_err();
    assert!(
      err.contains("should not contain newline"),
      "unexpected error message: {err}"
    );
  }

  #[test]
  fn timestamp_format_deref_works() {
    let fmt = TimestampFormat::from_str("%s").unwrap();
    assert_eq!(&*fmt, "%s");
  }

  /* ---------------- BOOT_TIME ---------------- */

  #[test]
  fn boot_time_is_non_zero() {
    assert!(*BOOT_TIME > 0);
  }

  #[test]
  fn boot_time_is_reasonable_unix_time() {
    // boot time should be after year 2000
    const YEAR_2000_NS: u64 = 946684800_u64 * 1_000_000_000;
    assert!(
      *BOOT_TIME > YEAR_2000_NS,
      "BOOT_TIME too small: {}",
      *BOOT_TIME
    );
  }

  /* ---------------- ts_from_boot_ns ---------------- */

  #[test]
  fn ts_from_boot_ns_zero_matches_boot_time() {
    let ts = ts_from_boot_ns(0);
    let expected: DateTime<Local> = DateTime::from_timestamp_nanos(*BOOT_TIME as i64).into();

    assert_eq!(ts, expected);
  }

  #[test]
  fn ts_from_boot_ns_is_monotonic() {
    let t1 = ts_from_boot_ns(1_000);
    let t2 = ts_from_boot_ns(2_000);

    assert!(t2 > t1);
  }

  #[test]
  fn ts_from_boot_ns_large_offset() {
    let one_sec = 1_000_000_000;
    let ts = ts_from_boot_ns(one_sec);

    let base: DateTime<Local> = DateTime::from_timestamp_nanos(*BOOT_TIME as i64).into();

    assert_eq!(ts.timestamp(), base.timestamp() + 1);
  }

  /* ---------------- serde (nutype derive) ---------------- */

  #[test]
  fn timestamp_format_serde_roundtrip() {
    let fmt = TimestampFormat::from_str("%H:%M:%S").unwrap();

    let json = serde_json::to_string(&fmt).unwrap();
    let de: TimestampFormat = serde_json::from_str(&json).unwrap();

    assert_eq!(&*fmt, &*de);
  }
}
