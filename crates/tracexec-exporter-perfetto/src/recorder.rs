use bytes::BufMut;
use crate::proto::TracePacket;
use prost::Message;

pub struct PerfettoTraceRecorder<W: std::io::Write> {
  buf: Vec<u8>,
  writer: W,
}

impl<W: std::io::Write> PerfettoTraceRecorder<W> {
  pub fn new(w: W) -> Self {
    Self {
      writer: w,
      // Start with a 2MiB buffer for buffering one [`TracePacket`]
      buf: Vec::with_capacity(2 * 1024 * 1024),
    }
  }

  pub fn record(&mut self, trace: TracePacket) -> std::io::Result<()> {
    // Write Tag: 0b00001010
    //
    // The MSB(Sign bit) is zero since we only use one byte
    // The next 4 bits represent the field number, which is 1 according to trace.proto
    // The last 3 bits represent the wire type, which is LEN in our case. (LEN=2)
    #[allow(clippy::unusual_byte_groupings)]
    self.buf.put_u8(0b0_0001_010);

    // Write Length and Value
    if trace.encode_length_delimited(&mut self.buf).is_err() {
      // Flush the buffer to get free space
      self.flush()?;
      // Try again
      if let Err(e) = trace.encode_length_delimited(&mut self.buf) {
        self.buf.reserve(e.required_capacity() - e.remaining());
        // SAFETY: we reserved enough capacity
        trace.encode_length_delimited(&mut self.buf).unwrap();
      }
    };

    Ok(())
  }

  pub fn flush(&mut self) -> std::io::Result<()> {
    self.writer.write_all(&self.buf)?;
    self.buf.clear();
    self.writer.flush()
  }
}

impl<W: std::io::Write> Drop for PerfettoTraceRecorder<W> {
  fn drop(&mut self) {
    // The user is required to call flush.
    // We don't guarantee successful flush on drop.
    let _ = self.flush();
  }
}
