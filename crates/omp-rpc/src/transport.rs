//! NDJSON transport: line framing, v2 chunk reassembly, frame dispatch.

use std::sync::Arc;

use serde_json::Value;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};

use crate::frame::ChunkReassembler;

pub type FrameHandler = Arc<dyn Fn(Value) + Send + Sync>;

/// Parse one already-read stdout line into zero or more frames.
///
/// Blank lines are skipped, malformed JSON clears any in-flight chunk sequence, and a normal
/// frame interrupts an incomplete sequence while still being delivered on its own.
pub fn handle_line(
    line: &str,
    reassembler: &mut ChunkReassembler,
    on_frame: &FrameHandler,
) -> Option<Vec<u8>> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    let Ok(parsed) = serde_json::from_str::<Value>(trimmed) else {
        reassembler.clear();
        return None;
    };
    let Some(obj) = parsed.as_object() else {
        reassembler.clear();
        return None;
    };

    if obj.get("type").and_then(Value::as_str) == Some("rpc_chunk") {
        let mut reassembled = None;
        match reassembler.feed(&parsed) {
            Ok(Some(bytes)) => {
                if let Ok(text) = std::str::from_utf8(&bytes) {
                    if let Ok(value) = serde_json::from_str::<Value>(text) {
                        if value.is_object() {
                            on_frame(value);
                        }
                    }
                }
                reassembled = Some(bytes);
            }
            Ok(None) => {}
            Err(_) => reassembler.clear(),
        }
        return reassembled;
    }

    if reassembler.has_pending() {
        reassembler.clear();
    }
    on_frame(parsed);
    None
}

/// Split one buffered chunk into frames the way `readline` does: `\n`, `\r\n` and a bare `\r`
/// all terminate a line.
fn handle_chunk(chunk: &str, reassembler: &mut ChunkReassembler, on_frame: &FrameHandler) {
    for segment in chunk.split(['\n', '\r']) {
        if segment.is_empty() {
            continue;
        }
        handle_line(segment, reassembler, on_frame);
    }
}

/// Read `reader` to EOF, dispatching every frame to `on_frame`.
///
/// A trailing fragment without a terminator is still delivered, matching `readline`'s close-time
/// flush — a sidecar that dies mid-write usually leaves exactly one complete frame missing its
/// newline.
pub async fn pump<R>(mut reader: R, on_frame: FrameHandler) -> std::io::Result<()>
where
    R: AsyncBufRead + Unpin,
{
    let mut reassembler = ChunkReassembler::new();
    let mut line = String::new();
    loop {
        line.clear();
        let read = reader.read_line(&mut line).await?;
        if read == 0 {
            if !line.is_empty() {
                handle_chunk(&line, &mut reassembler, &on_frame);
            }
            return Ok(());
        }
        handle_chunk(&line, &mut reassembler, &on_frame);
    }
}

/// Serialize a frame as one NDJSON line and flush it.
pub async fn write_frame<W>(writer: &mut W, frame: &Value) -> std::io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let mut bytes = serde_json::to_vec(frame)?;
    bytes.push(b'\n');
    writer.write_all(&bytes).await?;
    writer.flush().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD as BASE64;
    use parking_lot::Mutex;

    fn collector() -> (FrameHandler, Arc<Mutex<Vec<Value>>>) {
        let frames = Arc::new(Mutex::new(Vec::new()));
        let sink = frames.clone();
        (
            Arc::new(move |f: Value| sink.lock().push(f)),
            frames,
        )
    }

    #[test]
    fn skips_blank_and_malformed_lines() {
        let (handler, frames) = collector();
        let mut r = ChunkReassembler::new();
        handle_line("   ", &mut r, &handler);
        handle_line("{not json", &mut r, &handler);
        handle_line("[1,2,3]", &mut r, &handler);
        assert!(frames.lock().is_empty());
    }

    #[test]
    fn delivers_plain_frame() {
        let (handler, frames) = collector();
        let mut r = ChunkReassembler::new();
        handle_line(r#"{"type":"ready","protocolVersion":1}"#, &mut r, &handler);
        assert_eq!(frames.lock().len(), 1);
        assert_eq!(frames.lock()[0]["type"], "ready");
    }

    #[test]
    fn normal_frame_interrupts_incomplete_chunk_sequence() {
        let (handler, frames) = collector();
        let mut r = ChunkReassembler::new();
        let payload = vec![b'a'; 100];
        let line = serde_json::json!({
            "type": "rpc_chunk",
            "chunkId": "c1",
            "index": 0,
            "count": 2,
            "byteLength": crate::frame::MAX_FRAME_BYTES,
            "data": BASE64.encode(&payload),
        })
        .to_string();
        handle_line(&line, &mut r, &handler);
        assert!(r.has_pending());
        handle_line(r#"{"type":"session_info_update"}"#, &mut r, &handler);
        assert!(!r.has_pending());
        assert_eq!(frames.lock().len(), 1);
        assert_eq!(frames.lock()[0]["type"], "session_info_update");
    }

    #[tokio::test]
    async fn delivers_trailing_fragment_without_newline() {
        let (handler, frames) = collector();
        let body = br#"{"type":"agent_end"}"#;
        pump(&body[..], handler).await.unwrap();
        assert_eq!(frames.lock().len(), 1);
        assert_eq!(frames.lock()[0]["type"], "agent_end");
    }

    #[tokio::test]
    async fn splits_bare_carriage_returns() {
        let (handler, frames) = collector();
        let body = b"{\"type\":\"a\"}\r{\"type\":\"b\"}\r\n{\"type\":\"c\"}\n";
        pump(&body[..], handler).await.unwrap();
        let got: Vec<String> = frames
            .lock()
            .iter()
            .map(|f| f["type"].as_str().unwrap_or("").to_owned())
            .collect();
        assert_eq!(got, vec!["a", "b", "c"]);
    }

    #[test]
    fn reassembles_chunk_sequence_into_one_frame() {
        let (handler, frames) = collector();
        let mut r = ChunkReassembler::new();
        // The sequence must declare at least one max frame; pad the body to that size.
        let total = crate::frame::MAX_FRAME_BYTES as usize;
        let head = r#"{"type":"message_update","big":""#;
        let tail = r#""}"#;
        let body = format!("{head}{}{tail}", "x".repeat(total - head.len() - tail.len()));
        let bytes = body.as_bytes();
        assert!(bytes.len() as u64 >= crate::frame::MAX_FRAME_BYTES);
        let parts: Vec<&[u8]> = bytes.chunks(crate::frame::CHUNK_PAYLOAD_BYTES as usize).collect();
        for (i, part) in parts.iter().enumerate() {
            let line = serde_json::json!({
                "type": "rpc_chunk",
                "chunkId": "c9",
                "index": i,
                "count": parts.len(),
                "byteLength": bytes.len(),
                "data": BASE64.encode(part),
            })
            .to_string();
            handle_line(&line, &mut r, &handler);
        }
        assert_eq!(frames.lock().len(), 1);
        assert_eq!(frames.lock()[0]["type"], "message_update");
    }
}
