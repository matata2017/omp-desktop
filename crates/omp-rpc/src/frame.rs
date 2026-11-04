//! NDJSON framing and v2 `rpc_chunk` reassembly, mirroring `src/main/rpc-bridge.ts`.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::Value;

pub const MAX_FRAME_BYTES: u64 = 1_048_576;
pub const MAX_REASSEMBLED_BYTES: u64 = 67_108_864;
pub const CHUNK_PAYLOAD_BYTES: u64 = 262_144;
pub const MAX_CHUNK_COUNT: u64 = MAX_REASSEMBLED_BYTES.div_ceil(CHUNK_PAYLOAD_BYTES);

/// Negotiate v2 only when both peers agree on the framing limits.
pub fn supports_protocol_v2(ready: &Value) -> bool {
    let supported = ready.get("supportedProtocolVersions").and_then(Value::as_array);
    let has_v2 = supported
        .map(|versions| versions.iter().any(|v| v.as_u64() == Some(2)))
        .unwrap_or(false);
    has_v2
        && ready.get("maxFrameBytes").and_then(Value::as_u64) == Some(MAX_FRAME_BYTES)
        && ready.get("maxReassembledFrameBytes").and_then(Value::as_u64)
            == Some(MAX_REASSEMBLED_BYTES)
}

struct Pending {
    chunk_id: String,
    count: u64,
    byte_length: u64,
    next_index: u64,
    received_bytes: u64,
    chunks: Vec<u8>,
}

/// Reassembles one v2 chunk sequence.
///
/// The server writes a sequence contiguously; rejecting interleaving keeps retained memory
/// bounded to one logical frame. Any error clears the pending sequence.
#[derive(Default)]
pub struct ChunkReassembler {
    pending: Option<Pending>,
}

fn invalid(data: &str) -> Result<Vec<u8>, String> {
    // Strict: canonical standard alphabet with correct padding, and a byte-exact round trip.
    let trimmed = data.len();
    let padding_ok = trimmed % 4 == 0
        && data.bytes().all(|b| {
            b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'='
        })
        && !data[..data.len().saturating_sub(2)].contains('=');
    if !padding_ok {
        return Err("Invalid RPC chunk data".to_owned());
    }
    let bytes = BASE64
        .decode(data)
        .map_err(|_| "Invalid RPC chunk data".to_owned())?;
    if BASE64.encode(&bytes) != data {
        return Err("Invalid RPC chunk data".to_owned());
    }
    Ok(bytes)
}

impl ChunkReassembler {
    pub fn new() -> Self {
        Self { pending: None }
    }

    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub fn clear(&mut self) {
        self.pending = None;
    }

    /// Feed one `rpc_chunk` frame. Returns the reassembled bytes once the sequence completes.
    pub fn feed(&mut self, frame: &Value) -> Result<Option<Vec<u8>>, String> {
        let result = self.feed_inner(frame);
        if result.is_err() {
            self.pending = None;
        }
        result
    }

    fn feed_inner(&mut self, frame: &Value) -> Result<Option<Vec<u8>>, String> {
        let chunk_id = frame.get("chunkId").and_then(Value::as_str);
        let index = frame.get("index").and_then(Value::as_u64);
        let count = frame.get("count").and_then(Value::as_u64);
        let byte_length = frame.get("byteLength").and_then(Value::as_u64);

        let (Some(chunk_id), Some(index), Some(count), Some(byte_length)) =
            (chunk_id, index, count, byte_length)
        else {
            return Err("Invalid RPC chunk metadata".to_owned());
        };
        if chunk_id.is_empty()
            || chunk_id.len() > 128
            || count < 2
            || count > MAX_CHUNK_COUNT
            || byte_length < MAX_FRAME_BYTES
            || byte_length > MAX_REASSEMBLED_BYTES
        {
            return Err("Invalid RPC chunk metadata".to_owned());
        }

        let data = frame
            .get("data")
            .and_then(Value::as_str)
            .ok_or_else(|| "Invalid RPC chunk data".to_owned())?;
        let bytes = invalid(data)?;
        if bytes.len() as u64 > CHUNK_PAYLOAD_BYTES {
            return Err("RPC chunk payload exceeds the transport limit".to_owned());
        }

        if self.pending.is_none() {
            if index != 0 {
                return Err("RPC chunk sequence must start at index 0".to_owned());
            }
            self.pending = Some(Pending {
                chunk_id: chunk_id.to_owned(),
                count,
                byte_length,
                next_index: 0,
                received_bytes: 0,
                // Grow from the first chunk's size: a huge declared length with a tiny
                // payload must not pre-allocate the 64 MiB ceiling.
                chunks: Vec::with_capacity(bytes.len().min(CHUNK_PAYLOAD_BYTES as usize)),
            });
        }

        let pending = self.pending.as_mut().expect("just ensured");
        if pending.chunk_id != chunk_id
            || pending.count != count
            || pending.byte_length != byte_length
            || pending.next_index != index
        {
            return Err("RPC chunk sequence mismatch".to_owned());
        }

        pending.received_bytes += bytes.len() as u64;
        pending.next_index += 1;
        pending.chunks.extend_from_slice(&bytes);
        if pending.received_bytes > pending.byte_length {
            return Err("RPC chunk sequence exceeds declared length".to_owned());
        }
        if pending.next_index < pending.count {
            return Ok(None);
        }
        if pending.received_bytes != pending.byte_length {
            return Err("RPC chunk sequence length mismatch".to_owned());
        }
        let done = self.pending.take().expect("checked above");
        Ok(Some(done.chunks))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(chunk_id: &str, index: u64, count: u64, byte_length: u64, data: &[u8]) -> Value {
        serde_json::json!({
            "type": "rpc_chunk",
            "chunkId": chunk_id,
            "index": index,
            "count": count,
            "byteLength": byte_length,
            "data": BASE64.encode(data),
        })
    }

    #[test]
    fn reassembles_four_chunks() {
        // A chunk sequence only exists for frames >= MAX_FRAME_BYTES; payload pieces are capped
        // at CHUNK_PAYLOAD_BYTES, so 1 MiB needs exactly four.
        let payload = vec![b'x'; MAX_FRAME_BYTES as usize];
        let parts: Vec<&[u8]> = payload.chunks(CHUNK_PAYLOAD_BYTES as usize).collect();
        assert_eq!(parts.len(), 4);
        let total = payload.len() as u64;
        let mut r = ChunkReassembler::new();
        let mut out = None;
        for (i, part) in parts.iter().enumerate() {
            out = r.feed(&chunk("c1", i as u64, parts.len() as u64, total, part)).unwrap();
        }
        assert_eq!(out.unwrap(), payload);
        assert!(!r.has_pending());
    }

    #[test]
    fn rejects_out_of_order() {
        let mut r = ChunkReassembler::new();
        let e = r
            .feed(&chunk("c1", 1, 2, MAX_FRAME_BYTES + 1, b"abc"))
            .unwrap_err();
        assert_eq!(e, "RPC chunk sequence must start at index 0");
        assert!(!r.has_pending());
    }

    #[test]
    fn rejects_length_mismatch() {
        let mut r = ChunkReassembler::new();
        let total = MAX_FRAME_BYTES + 8;
        assert!(r.feed(&chunk("c1", 0, 2, total, b"abc")).unwrap().is_none());
        let e = r.feed(&chunk("c1", 1, 2, total, b"de")).unwrap_err();
        assert_eq!(e, "RPC chunk sequence length mismatch");
    }

    #[test]
    fn rejects_interleaved_sequence() {
        let mut r = ChunkReassembler::new();
        let total = MAX_FRAME_BYTES + 8;
        r.feed(&chunk("c1", 0, 2, total, b"abc")).unwrap();
        let e = r.feed(&chunk("c2", 1, 2, total, b"abc")).unwrap_err();
        assert_eq!(e, "RPC chunk sequence mismatch");
    }

    #[test]
    fn rejects_count_below_two() {
        let mut r = ChunkReassembler::new();
        let e = r.feed(&chunk("c1", 0, 1, MAX_FRAME_BYTES, b"abc")).unwrap_err();
        assert_eq!(e, "Invalid RPC chunk metadata");
    }

    #[test]
    fn rejects_declared_length_below_max_frame() {
        let mut r = ChunkReassembler::new();
        let e = r
            .feed(&chunk("c1", 0, 2, MAX_FRAME_BYTES - 1, b"abc"))
            .unwrap_err();
        assert_eq!(e, "Invalid RPC chunk metadata");
    }

    #[test]
    fn rejects_chunk_id_out_of_range() {
        let mut r = ChunkReassembler::new();
        let long = "x".repeat(129);
        assert_eq!(
            r.feed(&chunk(&long, 0, 2, MAX_FRAME_BYTES, b"abc")).unwrap_err(),
            "Invalid RPC chunk metadata"
        );
        assert_eq!(
            r.feed(&chunk("", 0, 2, MAX_FRAME_BYTES, b"abc")).unwrap_err(),
            "Invalid RPC chunk metadata"
        );
    }

    #[test]
    fn rejects_payload_over_transport_limit() {
        let mut r = ChunkReassembler::new();
        let oversized = vec![b'z'; CHUNK_PAYLOAD_BYTES as usize + 1];
        let e = r
            .feed(&chunk("c1", 0, 4, MAX_FRAME_BYTES, &oversized))
            .unwrap_err();
        assert_eq!(e, "RPC chunk payload exceeds the transport limit");
    }

    #[test]
    fn rejects_sequence_longer_than_declared() {
        let mut r = ChunkReassembler::new();
        let piece = vec![b'q'; CHUNK_PAYLOAD_BYTES as usize];
        let mut last = Ok(None);
        for i in 0..5u64 {
            last = r.feed(&chunk("c1", i, 5, MAX_FRAME_BYTES, &piece));
            if last.is_err() {
                break;
            }
        }
        assert_eq!(
            last.unwrap_err(),
            "RPC chunk sequence exceeds declared length"
        );
    }

    #[test]
    fn rejects_declared_length_too_large() {
        let mut r = ChunkReassembler::new();
        let e = r
            .feed(&chunk("c1", 0, 2, MAX_REASSEMBLED_BYTES + 1, b"abc"))
            .unwrap_err();
        assert_eq!(e, "Invalid RPC chunk metadata");
    }

    #[test]
    fn rejects_non_canonical_base64() {
        let mut r = ChunkReassembler::new();
        let mut frame = chunk("c1", 0, 2, MAX_FRAME_BYTES, b"abc");
        frame["data"] = Value::String("ab=c".to_owned());
        assert_eq!(r.feed(&frame).unwrap_err(), "Invalid RPC chunk data");

        let mut frame = chunk("c1", 0, 2, MAX_FRAME_BYTES, b"abc");
        frame["data"] = Value::String("a".to_owned());
        assert_eq!(r.feed(&frame).unwrap_err(), "Invalid RPC chunk data");
    }

    #[test]
    fn normal_frame_clears_pending() {
        let mut r = ChunkReassembler::new();
        let total = MAX_FRAME_BYTES + 8;
        r.feed(&chunk("c1", 0, 2, total, b"abc")).unwrap();
        assert!(r.has_pending());
        r.clear();
        assert!(!r.has_pending());
    }

    #[test]
    fn negotiates_protocol_v2() {
        let ready = serde_json::json!({
            "type": "ready",
            "supportedProtocolVersions": [1, 2],
            "maxFrameBytes": MAX_FRAME_BYTES,
            "maxReassembledFrameBytes": MAX_REASSEMBLED_BYTES,
        });
        assert!(supports_protocol_v2(&ready));
        let mut smaller = ready.clone();
        smaller["maxFrameBytes"] = serde_json::json!(1024);
        assert!(!supports_protocol_v2(&smaller));
    }
}
