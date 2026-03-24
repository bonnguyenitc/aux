use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use std::time::Duration;
use tokio::process::{Child, Command};
use tokio::time::sleep;

use crate::error::DuetError;
use super::MediaPlayer;

const SOCKET_PATH: &str = "/tmp/duet-mpv.sock";

pub struct MpvPlayer {
    process: Option<Child>,
    socket_path: PathBuf,
    playing: bool,
}

impl MpvPlayer {
    pub fn new() -> Self {
        Self {
            process: None,
            socket_path: PathBuf::from(SOCKET_PATH),
            playing: false,
        }
    }

    /// Check if mpv is available in PATH
    pub async fn check_available() -> Result<()> {
        let output = Command::new("mpv")
            .arg("--version")
            .output()
            .await
            .map_err(|_| DuetError::MpvNotFound)?;

        if !output.status.success() {
            return Err(DuetError::MpvNotFound.into());
        }

        Ok(())
    }

    /// Clean up socket file if it exists from a previous run
    fn cleanup_socket(&self) {
        let _ = std::fs::remove_file(&self.socket_path);
    }

    /// Send a JSON IPC command to mpv via the Unix socket
    async fn send_ipc_command(&self, command: &serde_json::Value) -> Result<String> {
        let cmd_str = serde_json::to_string(command)?;

        let output = Command::new("sh")
            .args([
                "-c",
                &format!(
                    "echo '{}' | nc -U -w1 {}",
                    cmd_str,
                    self.socket_path.display()
                ),
            ])
            .output()
            .await
            .context("Failed to send IPC command to mpv")?;

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Send mpv JSON IPC command
    async fn mpv_command(&self, args: &[&str]) -> Result<String> {
        let command = serde_json::json!({
            "command": args
        });
        self.send_ipc_command(&command).await
    }

    /// Get a property value from mpv
    async fn get_property(&self, name: &str) -> Result<serde_json::Value> {
        let command = serde_json::json!({
            "command": ["get_property", name]
        });
        let response = self.send_ipc_command(&command).await?;

        for line in response.lines() {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(line) {
                if let Some(data) = parsed.get("data") {
                    return Ok(data.clone());
                }
            }
        }

        bail!("Could not get property: {}", name);
    }

    /// Get current volume level
    pub async fn get_property_volume(&self) -> Result<u8> {
        let value = self.get_property("volume").await?;
        let vol = value.as_f64().unwrap_or(80.0) as u8;
        Ok(vol.min(100))
    }
}

impl MediaPlayer for MpvPlayer {
    async fn play(&mut self, url: &str, _title: &str) -> Result<()> {
        // Stop any existing playback
        self.stop().await.ok();
        self.cleanup_socket();

        let child = Command::new("mpv")
            .args([
                "--no-video",
                &format!("--input-ipc-server={}", self.socket_path.display()),
                "--really-quiet",
                url,
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .context("Failed to start mpv")?;

        self.process = Some(child);
        self.playing = true;

        // Wait for socket to be ready
        for _ in 0..20 {
            if self.socket_path.exists() {
                return Ok(());
            }
            sleep(Duration::from_millis(100)).await;
        }

        Ok(())
    }

    async fn pause(&self) -> Result<()> {
        self.mpv_command(&["set_property", "pause", "yes"]).await?;
        Ok(())
    }

    async fn resume(&self) -> Result<()> {
        self.mpv_command(&["set_property", "pause", "no"]).await?;
        Ok(())
    }

    async fn toggle_pause(&self) -> Result<()> {
        self.mpv_command(&["cycle", "pause"]).await?;
        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        if let Some(mut child) = self.process.take() {
            child.kill().await.ok();
        }
        self.playing = false;
        self.cleanup_socket();
        Ok(())
    }

    async fn set_volume(&self, volume: u8) -> Result<()> {
        let vol = volume.min(100).to_string();
        self.mpv_command(&["set_property", "volume", &vol]).await?;
        Ok(())
    }

    async fn seek(&self, offset_secs: f64) -> Result<()> {
        let offset = offset_secs.to_string();
        self.mpv_command(&["seek", &offset, "relative"]).await?;
        Ok(())
    }

    async fn get_position(&self) -> Result<Duration> {
        let value = self.get_property("time-pos").await?;
        let secs = value.as_f64().unwrap_or(0.0);
        Ok(Duration::from_secs_f64(secs))
    }

    async fn get_duration(&self) -> Result<Duration> {
        let value = self.get_property("duration").await?;
        let secs = value.as_f64().unwrap_or(0.0);
        Ok(Duration::from_secs_f64(secs))
    }

    fn is_playing(&self) -> bool {
        self.playing
    }
}

impl Drop for MpvPlayer {
    fn drop(&mut self) {
        self.cleanup_socket();
        if let Some(mut child) = self.process.take() {
            let _ = child.start_kill();
        }
    }
}
