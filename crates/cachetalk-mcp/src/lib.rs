pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod semantic;
pub mod server;
pub mod tools;

pub use semantic::{
    bit_error_rate, MessageAssembler, MessageEncoder, SemanticError, CHUNK_FLAG_FIRST,
    CHUNK_HEADER_LEN,
};
pub use server::{tool_definitions, McpServer, PROTOCOL_VERSION};
pub use tools::{
    CacheOccupancyTransport, Channel, ChannelConfig, ChannelError, CovertMode, CovertTransport,
    LoopbackTransport, ReceiveReport, SendReport, TransportError,
};
