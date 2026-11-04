//! Transport-only RPC layer: NDJSON framing, v2 chunk reassembly, id correlation.
//!
//! Commands and frames stay `serde_json::Value`; wire types live in TypeScript
//! (`src/shared/rpc-types.ts`) and are not re-declared here.

pub mod client;
pub mod frame;
pub mod transport;

pub use client::{DEFAULT_TIMEOUT, RpcClient, timeout_for_command};
pub use frame::{
    CHUNK_PAYLOAD_BYTES, ChunkReassembler, MAX_CHUNK_COUNT, MAX_FRAME_BYTES,
    MAX_REASSEMBLED_BYTES, supports_protocol_v2,
};
pub use transport::{FrameHandler, handle_line, pump, write_frame};
