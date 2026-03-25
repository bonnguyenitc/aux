use anyhow::Result;
use colored::Colorize;

use super::mpv::MpvPlayer;
use super::state::StateFile;
use super::types::{NowPlayingInfo, RepeatMode};
use super::MediaPlayer;
use crate::error::AuxError;

pub struct RemoteSession {
    pub player: MpvPlayer,
    pub state: StateFile,
}

impl RemoteSession {
    /// Connect to an existing aux session, or return NoActiveSession error.
    pub fn connect() -> Result<Self> {
        if !StateFile::exists() {
            return Err(AuxError::NoActiveSession.into());
        }
        let state = StateFile::read()?;
        let player = MpvPlayer::connect_existing()?;
        Ok(Self { player, state })
    }

    /// Get current now-playing info from mpv properties + state file metadata.
    pub async fn now(&self) -> Result<NowPlayingInfo> {
        let position_secs = self
            .player
            .get_position()
            .await
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        let duration_secs = self
            .player
            .get_duration()
            .await
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        let volume = self.player.get_volume().await.unwrap_or(80);
        let speed = self
            .player
            .get_speed()
            .await
            .unwrap_or(self.state.speed);
        let paused = self.player.get_paused().await.unwrap_or(false);

        Ok(NowPlayingInfo {
            video: self.state.video.clone(),
            position_secs,
            duration_secs,
            volume,
            speed,
            paused,
            repeat: self.state.repeat,
            shuffle: self.state.shuffle,
            sleep_deadline: self.state.sleep_deadline,
            eq_preset: self.state.eq_preset.clone().unwrap_or_else(|| "flat".to_string()),
        })
    }

    pub async fn pause(&self) -> Result<()> {
        self.player
            .pause()
            .await
            .map_err(|_| anyhow::anyhow!("Failed to pause. Is mpv still running?"))
    }

    pub async fn resume(&self) -> Result<()> {
        self.player
            .resume()
            .await
            .map_err(|_| anyhow::anyhow!("Failed to resume. Is mpv still running?"))
    }

    pub async fn stop(self) -> Result<()> {
        StateFile::remove()?;
        println!("  {} Stopped.", "■".red());
        Ok(())
    }

    pub async fn set_volume(&self, volume: u8) -> Result<()> {
        self.player
            .set_volume(volume.min(100))
            .await
            .map_err(|_| anyhow::anyhow!("Failed to set volume"))
    }

    pub async fn set_speed(&self, speed: f64) -> Result<()> {
        let speed = speed.clamp(0.25, 4.0);
        self.player.set_speed(speed).await?;
        let mut state = self.state.clone();
        state.speed = speed;
        state.write()?;
        Ok(())
    }

    pub async fn set_repeat(&self, repeat: RepeatMode) -> Result<()> {
        let mut state = self.state.clone();
        state.repeat = repeat;
        state.write()?;
        Ok(())
    }

    pub async fn toggle_shuffle(&self) -> Result<bool> {
        let mut state = self.state.clone();
        state.shuffle = !state.shuffle;
        state.write()?;
        Ok(state.shuffle)
    }
}
