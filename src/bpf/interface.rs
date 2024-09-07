use enumflags2::bitflags;

#[bitflags]
#[repr(u32)]
#[derive(Copy, Clone, Debug, PartialEq)]
#[allow(non_camel_case_types)]
pub enum BpfEventFlags {
  // This flag is set if any other error occurs
  ERROR = 1,
  // This flag is set if we don't have enough loops to read all items
  TOO_MANY_ITEMS = 2,
  COMM_READ_FAILURE = 4,
  POSSIBLE_TRUNCATION = 8,
  PTR_READ_FAILURE = 16,
  NO_ROOM = 32,
  STR_READ_FAILURE = 64,
  // Failed to get information about fds
  FDS_PROBE_FAILURE = 128,
  // Failed to send event into ringbuf
  OUTPUT_FAILURE = 256,
  // Failed to read flags
  FLAGS_READ_FAILURE = 512,
  // A marker for dropped events. This flag is only set in userspace.
  USERSPACE_DROP_MARKER = 1024,
  // Operation stopped early because of errors
  BAIL_OUT = 2048,
  // bpf_loop failure
  LOOP_FAIL = 4096,
  // Failed to read whole path
  PATH_READ_ERR = 8192,
  // inode read failure
  INO_READ_ERR = 16384,
  // mount id read failure
  MNTID_READ_ERR = 32768,
  // filename read failure
  FILENAME_READ_ERR = 65536,
}
