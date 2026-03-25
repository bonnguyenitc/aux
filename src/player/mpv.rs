use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::process::{Child, Command};
use tokio::time::sleep;

use super::MediaPlayer;
use crate::error::AuxError;

const SOCKET_PATH: &str = "/tmp/aux-mpv.sock";

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

    /// Create a player that connects to an existing mpv socket (for remote/detached commands)
    pub fn connect_existing() -> Result<Self> {
        let socket_path = PathBuf::from(SOCKET_PATH);
        if !socket_path.exists() {
            return Err(AuxError::NoActiveSession.into());
        }
        Ok(Self {
            process: None,
            socket_path,
            playing: true,
        })
    }

    /// Returns the IPC socket path for debugging and future external IPC use.
    #[allow(dead_code)]
    pub fn socket_path_str() -> &'static str {
        SOCKET_PATH
    }

    pub async fn check_available() -> Result<()> {
        let output = Command::new("mpv")
            .arg("--version")
            .output()
            .await
            .map_err(|_| AuxError::MpvNotFound)?;
        if !output.status.success() {
            return Err(AuxError::MpvNotFound.into());
        }
        Ok(())
    }

    fn cleanup_socket(&self) {
        let _ = std::fs::remove_file(&self.socket_path);
    }

    /// Send a JSON IPC command to mpv via Unix socket and return the response.
    /// Retries up to 3 times with backoff on connection failure.
    /// Times out after 2 seconds to prevent hangs.
    async fn send_ipc_command(&self, command: &serde_json::Value) -> Result<String> {
        let mut cmd = serde_json::to_vec(command)?;
        cmd.push(b'\n');

        let mut last_err = None;
        for attempt in 0..3u32 {
            if attempt > 0 {
                sleep(Duration::from_millis(50 * (attempt as u64))).await;
            }

            let connect_result = tokio::time::timeout(
                Duration::from_secs(2),
                UnixStream::connect(&self.socket_path),
            )
            .await;

            let mut stream = match connect_result {
                Ok(Ok(s)) => s,
                Ok(Err(e)) => {
                    last_err = Some(e.into());
                    continue;
                }
                Err(_) => {
                    last_err = Some(anyhow::anyhow!("connection timeout"));
                    continue;
                }
            };

            // Write with timeout
            let write_result = tokio::time::timeout(
                Duration::from_secs(2),
                stream.write_all(&cmd),
            )
            .await;

            match write_result {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    last_err = Some(e.into());
                    continue;
                }
                Err(_) => {
                    last_err = Some(anyhow::anyhow!("write timeout"));
                    continue;
                }
            }

            // Read response with timeout
            let mut reader = BufReader::new(stream);
            let mut response = String::new();
            let read_result = tokio::time::timeout(
                Duration::from_secs(2),
                reader.read_line(&mut response),
            )
            .await;

            match read_result {
                Ok(Ok(_)) => return Ok(response),
                Ok(Err(e)) => {
                    last_err = Some(e.into());
                    continue;
                }
                Err(_) => {
                    last_err = Some(anyhow::anyhow!("read timeout"));
                    continue;
                }
            }
        }

        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("mpv IPC failed after 3 attempts")))
    }

    async fn mpv_command(&self, args: &[&str]) -> Result<String> {
        let command = serde_json::json!({ "command": args });
        self.send_ipc_command(&command).await
    }

    async fn get_property(&self, name: &str) -> Result<serde_json::Value> {
        let command = serde_json::json!({ "command": ["get_property", name] });
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

    async fn set_property(&self, name: &str, value: &serde_json::Value) -> Result<()> {
        let command = serde_json::json!({ "command": ["set_property", name, value] });
        self.send_ipc_command(&command).await?;
        Ok(())
    }
}

impl MediaPlayer for MpvPlayer {
    async fn play(&mut self, url: &str, _title: &str) -> Result<()> {
        self.stop().await.ok();
        self.cleanup_socket();

        let child = Command::new("mpv")
            .args([
                "--no-video",
                &format!("--input-ipc-server={}", self.socket_path.display()),
                "--input-media-keys=yes",
                "--af=loudnorm",
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

        // Wait for socket to become available
        for _ in 0..30 {
            if self.socket_path.exists() {
                return Ok(());
            }
            sleep(Duration::from_millis(100)).await;
        }
        Ok(())
    }

    async fn pause(&self) -> Result<()> {
        self.set_property("pause", &serde_json::json!(true)).await
    }

    async fn resume(&self) -> Result<()> {
        self.set_property("pause", &serde_json::json!(false)).await
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
        let vol = volume.min(100);
        self.set_property("volume", &serde_json::json!(vol)).await
    }

    async fn seek(&self, offset_secs: f64) -> Result<()> {
        let offset = offset_secs.to_string();
        self.mpv_command(&["seek", &offset, "relative"]).await?;
        Ok(())
    }

    async fn seek_to(&self, position_secs: f64) -> Result<()> {
        let pos = position_secs.to_string();
        self.mpv_command(&["seek", &pos, "absolute"]).await?;
        Ok(())
    }

    async fn set_speed(&self, speed: f64) -> Result<()> {
        let speed = speed.clamp(0.25, 4.0);
        self.set_property("speed", &serde_json::json!(speed)).await
    }

    async fn get_speed(&self) -> Result<f64> {
        let val = self.get_property("speed").await?;
        Ok(val.as_f64().unwrap_or(1.0))
    }

    async fn get_paused(&self) -> Result<bool> {
        let val = self.get_property("pause").await?;
        Ok(val.as_bool().unwrap_or(false))
    }

    async fn get_volume(&self) -> Result<u8> {
        let val = self.get_property("volume").await?;
        Ok((val.as_f64().unwrap_or(80.0) as u8).min(100))
    }

    async fn get_position(&self) -> Result<Duration> {
        let val = self.get_property("time-pos").await?;
        Ok(Duration::from_secs_f64(val.as_f64().unwrap_or(0.0)))
    }

    async fn get_duration(&self) -> Result<Duration> {
        let val = self.get_property("duration").await?;
        Ok(Duration::from_secs_f64(val.as_f64().unwrap_or(0.0)))
    }

    async fn is_finished(&self) -> Result<bool> {
        let eof = self
            .get_property("eof-reached")
            .await
            .map(|v| v.as_bool().unwrap_or(false))
            .unwrap_or(false);
        let idle = self
            .get_property("idle-active")
            .await
            .map(|v| v.as_bool().unwrap_or(false))
            .unwrap_or(false);
        Ok(eof || idle)
    }

    async fn load(&self, url: &str) -> Result<()> {
        self.mpv_command(&["loadfile", url, "replace"]).await?;
        Ok(())
    }

    fn is_playing(&self) -> bool {
        self.playing
    }
}

impl MpvPlayer {
    /// Set mpv loop-file property: true = "inf" (loop forever), false = "no"
    pub async fn set_loop_file(&self, enabled: bool) -> Result<()> {
        let val = if enabled { "inf" } else { "no" };
        self.set_property("loop-file", &serde_json::json!(val)).await
    }

    /// Set audio filter chain (equalizer). Empty string to clear.
    pub async fn set_audio_filter(&self, filter: &str) -> Result<()> {
        if filter.is_empty() {
            // Remove all audio filters, keep only loudnorm
            self.set_property("af", &serde_json::json!("loudnorm")).await
        } else {
            // Set loudnorm + the equalizer filter
            let combined = format!("loudnorm,{}", filter);
            self.set_property("af", &serde_json::json!(combined)).await
        }
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
