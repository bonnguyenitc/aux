pub mod mpv;

use anyhow::Result;
use std::time::Duration;

pub use mpv::MpvPlayer;

pub trait MediaPlayer {
    fn play(
        &mut self,
        url: &str,
        title: &str,
    ) -> impl std::future::Future<Output = Result<()>> + Send;

    fn pause(&self) -> impl std::future::Future<Output = Result<()>> + Send;
    fn resume(&self) -> impl std::future::Future<Output = Result<()>> + Send;
    fn toggle_pause(&self) -> impl std::future::Future<Output = Result<()>> + Send;
    fn stop(&mut self) -> impl std::future::Future<Output = Result<()>> + Send;
    fn set_volume(&self, volume: u8) -> impl std::future::Future<Output = Result<()>> + Send;
    fn seek(&self, offset_secs: f64) -> impl std::future::Future<Output = Result<()>> + Send;
    fn get_position(&self) -> impl std::future::Future<Output = Result<Duration>> + Send;
    fn get_duration(&self) -> impl std::future::Future<Output = Result<Duration>> + Send;
    fn is_playing(&self) -> bool;
}
