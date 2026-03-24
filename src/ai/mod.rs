pub mod chat;
pub mod transcript;

// Public re-exports for use by CLI commands and external integrations.
// Some are not yet wired into the main binary but are part of the stable API.
#[allow(unused_imports)]
pub use chat::{chat as ai_chat, ChatMessage, VideoContext};
#[allow(unused_imports)]
pub use transcript::{fetch_transcript, Transcript};
