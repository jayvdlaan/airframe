//! Correlation-aware JSON writer.
//! Wraps an inner Write and injects a `correlation_id` field into JSON lines when enabled
//! and a correlation id is available via `crate::correlation::get()`.

pub struct CorrelationJsonWriter<W: std::io::Write> {
    inner: W,
    enabled: bool,
    buf: Vec<u8>,
}

impl<W: std::io::Write> CorrelationJsonWriter<W> {
    pub fn new(inner: W, enabled: bool) -> Self {
        Self {
            inner,
            enabled,
            buf: Vec::with_capacity(1024),
        }
    }

    fn process_line(&mut self, mut line: String) -> std::io::Result<()> {
        if self.enabled {
            if let Some(id) = crate::correlation::get() {
                if let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&line) {
                    if let serde_json::Value::Object(ref mut map) = v {
                        map.insert("correlation_id".to_string(), serde_json::Value::String(id));
                        line = serde_json::to_string(&v).unwrap_or(line);
                    }
                }
            }
        }
        self.inner.write_all(line.as_bytes())?;
        self.inner.write_all(b"\n")
    }
}

impl<W: std::io::Write> std::io::Write for CorrelationJsonWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buf.extend_from_slice(buf);
        // Process complete lines terminated by '\n'
        while let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
            // Drain up to and including the newline
            let mut line_bytes: Vec<u8> = self.buf.drain(..=pos).collect();
            // Remove the trailing newline for JSON parsing
            if let Some(b'\n') = line_bytes.last().copied() {
                line_bytes.pop();
            }
            let line = String::from_utf8_lossy(&line_bytes).to_string();
            self.process_line(line)?;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if !self.buf.is_empty() {
            // Best-effort: write the remaining partial line without injection (not a complete JSON object yet)
            self.inner.write_all(&self.buf)?;
            self.buf.clear();
        }
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[derive(Default)]
    struct MemWriter {
        data: Vec<u8>,
    }
    impl std::io::Write for MemWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.data.extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn injects_correlation_when_enabled_and_present() {
        // Initialize task-local by creating a tokio runtime and using correlation::scope
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut mw = MemWriter::default();
            let mut w = CorrelationJsonWriter::new(&mut mw, true);
            // Set correlation id in scope
            crate::correlation::scope("abc-123", async {
                let line = serde_json::json!({"message":"hi"}).to_string() + "\n";
                w.write_all(line.as_bytes()).unwrap();
                w.flush().unwrap();
            })
            .await;
            let s = String::from_utf8(mw.data.clone()).unwrap();
            let last = s.trim_end();
            let v: serde_json::Value = serde_json::from_str(last).unwrap();
            assert_eq!(v.get("message").and_then(|x| x.as_str()), Some("hi"));
            assert_eq!(
                v.get("correlation_id").and_then(|x| x.as_str()),
                Some("abc-123")
            );
        });
    }

    #[test]
    fn does_not_inject_when_disabled_or_missing_id() {
        // disabled
        let mut mw = MemWriter::default();
        let mut w = CorrelationJsonWriter::new(&mut mw, false);
        let line = serde_json::json!({"k":1}).to_string() + "\n";
        w.write_all(line.as_bytes()).unwrap();
        let s = String::from_utf8(mw.data.clone()).unwrap();
        let v: serde_json::Value = serde_json::from_str(s.trim_end()).unwrap();
        assert!(v.get("correlation_id").is_none());

        // enabled but no correlation set
        let mut mw2 = MemWriter::default();
        let mut w2 = CorrelationJsonWriter::new(&mut mw2, true);
        let line2 = serde_json::json!({"k":2}).to_string() + "\n";
        w2.write_all(line2.as_bytes()).unwrap();
        let s2 = String::from_utf8(mw2.data.clone()).unwrap();
        let v2: serde_json::Value = serde_json::from_str(s2.trim_end()).unwrap();
        assert!(v2.get("correlation_id").is_none());
    }

    #[test]
    fn partial_line_then_flush_writes_verbatim() {
        let mut mw = MemWriter::default();
        let s = "{\"a\":1}"; // no newline yet
        {
            let mut w = CorrelationJsonWriter::new(&mut mw, true);
            w.write_all(s.as_bytes()).unwrap();
            // nothing should be flushed yet because no newline
            w.flush().unwrap(); // flush should write the partial as-is
        }
        // After flush and drop, partial is written as-is (no newline added by flush)
        let got = String::from_utf8(mw.data.clone()).unwrap();
        assert_eq!(got, s);
        // If we now finish the line and write a newline, it should be processed
        {
            let mut w2 = CorrelationJsonWriter::new(&mut mw, true);
            let tail = "\n";
            w2.write_all(tail.as_bytes()).unwrap();
        }
        let out = String::from_utf8(mw.data.clone()).unwrap();
        assert!(out.ends_with("\n"));
    }
}
