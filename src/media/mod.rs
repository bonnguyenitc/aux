pub mod source;
pub mod types;
pub mod ytdlp;

use anyhow::Result;
pub use source::Source;
pub use types::{MediaInfo, StreamUrl};
pub use ytdlp::is_direct_url;
pub use ytdlp::YtDlp;

pub trait MediaBackend {
    fn search(
        &self,
        query: &str,
        limit: usize,
        source: &Source,
    ) -> impl std::future::Future<Output = Result<Vec<MediaInfo>>> + Send;

    fn get_stream_url(
        &self,
        video_url: &str,
    ) -> impl std::future::Future<Output = Result<StreamUrl>> + Send;
}
