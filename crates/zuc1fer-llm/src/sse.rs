//! Robust line buffering for streamed HTTP bodies.
//!
//! Network chunks from `reqwest::Response::bytes_stream` are arbitrary byte
//! buffers: a single SSE `data:` line can be split across two chunks, and a
//! multi-byte UTF-8 character can straddle a chunk boundary. `LineBuffer`
//! accumulates raw bytes and only yields complete lines, decoding each line
//! once all of its bytes have arrived. This prevents the silent token / tool
//! call loss that occurs when decoding and line-splitting each chunk in
//! isolation.

#[derive(Default)]
pub struct LineBuffer {
    buf: Vec<u8>,
}

impl LineBuffer {
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    /// Append a chunk and return every complete line it produced (without the
    /// trailing `\n`/`\r`). Incomplete trailing bytes are retained for the
    /// next call.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        self.buf.extend_from_slice(chunk);
        let mut lines = Vec::new();
        while let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.buf.drain(..=pos).collect();
            let decoded = String::from_utf8_lossy(&line);
            lines.push(decoded.trim_end_matches(['\n', '\r']).to_string());
        }
        lines
    }

    /// Return any remaining buffered bytes as a final line. Call once after the
    /// stream ends to flush a body that did not terminate with a newline.
    pub fn flush(&mut self) -> Option<String> {
        if self.buf.is_empty() {
            return None;
        }
        let decoded = String::from_utf8_lossy(&self.buf).trim().to_string();
        self.buf.clear();
        if decoded.is_empty() {
            None
        } else {
            Some(decoded)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_lines_within_a_chunk() {
        let mut lb = LineBuffer::new();
        let lines = lb.push(b"data: a\ndata: b\n");
        assert_eq!(lines, vec!["data: a".to_string(), "data: b".to_string()]);
    }

    #[test]
    fn reassembles_line_split_across_chunks() {
        let mut lb = LineBuffer::new();
        assert!(lb.push(b"data: {\"hel").is_empty());
        let lines = lb.push(b"lo\":1}\n");
        assert_eq!(lines, vec!["data: {\"hello\":1}".to_string()]);
    }

    #[test]
    fn reassembles_utf8_split_across_chunks() {
        let mut lb = LineBuffer::new();
        let smiley = "😀".as_bytes();
        lb.push(&smiley[..2]);
        let lines = lb.push(&[&smiley[2..], b"\n"].concat());
        assert_eq!(lines, vec!["😀".to_string()]);
    }

    #[test]
    fn flush_returns_trailing_partial_line() {
        let mut lb = LineBuffer::new();
        assert!(lb.push(b"data: tail").is_empty());
        assert_eq!(lb.flush(), Some("data: tail".to_string()));
        assert_eq!(lb.flush(), None);
    }
}
