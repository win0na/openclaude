pub mod http;
pub mod openai;
pub mod protocol;
pub mod service;
pub mod stdio;

pub use http::create_router;
pub use openai::{
    ChatChoice, ChatChunk, ChatChunkChoice, ChatContent, ChatContentPart, ChatDelta,
    ChatFunctionCall, ChatFunctionCallDelta, ChatFunctionChoice, ChatImageUrl, ChatMessage,
    ChatRequest, ChatResponse, ChatRole, ChatTool, ChatToolCall, ChatToolCallDelta, ChatToolType,
    ChatUsage, format_sse, format_sse_done,
};
pub use protocol::{
    ServerCommand, ServerEnvelope, ServerMetadata, ServerModel, ServerRequest, ServerResponse,
};
pub use service::ClydeService;
pub use stdio::serve_stdio;
