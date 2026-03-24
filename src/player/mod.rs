pub mod mpv;
pub mod queue_manager;
pub mod remote;
pub mod state;
pub mod types;

use anyhow::Result;
use std::time::Duration;

pub use mpv::MpvPlayer;
pub use remote::RemoteSession;
pub use types::RepeatMode;

pub trait MediaPlayer {
    // Core playback
    async fn play(&mut self, url: &str, title: &str) -> Result<()>;
    async fn pause(&self) -> Result<()>;
    async fn resume(&self) -> Result<()>;
    async fn toggle_pause(&self) -> Result<()>;
    async fn stop(&mut self) -> Result<()>;
    async fn set_volume(&self, volume: u8) -> Result<()>;
    async fn seek(&self, offset_secs: f64) -> Result<()>;
    async fn get_position(&self) -> Result<Duration>;
    async fn get_duration(&self) -> Result<Duration>;
    async fn seek_to(&self, position_secs: f64) -> Result<()>;
    async fn set_speed(&self, speed: f64) -> Result<()>;
    async fn get_speed(&self) -> Result<f64>;
    async fn get_paused(&self) -> Result<bool>;
    async fn get_volume(&self) -> Result<u8>;

    // Reserved for future daemon/playlist use
    #[allow(dead_code)]
    fn is_playing(&self) -> bool;
    #[allow(dead_code)]
    async fn is_finished(&self) -> Result<bool>;
    #[allow(dead_code)]
    async fn load(&self, url: &str) -> Result<()>;
}
