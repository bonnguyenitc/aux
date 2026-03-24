pub mod types;
pub mod ytdlp;

use anyhow::Result;
pub use types::{VideoInfo, StreamUrl};
pub use ytdlp::YtDlp;

pub trait YouTubeBackend {
    fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> impl std::future::Future<Output = Result<Vec<VideoInfo>>> + Send;

    fn get_stream_url(
        &self,
        video_url: &str,
    ) -> impl std::future::Future<Output = Result<StreamUrl>> + Send;
}
