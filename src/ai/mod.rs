pub mod chat;
pub mod transcript;

// Public re-exports for use by CLI commands and external integrations.
#[allow(unused_imports)]
pub use chat::{chat as ai_chat, AiAction, ChatMessage, ChatResponse, VideoContext};
#[allow(unused_imports)]
pub use chat::execute_action;
#[allow(unused_imports)]
pub use transcript::{fetch_transcript, Transcript};
