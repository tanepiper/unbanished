//! The viewer's single-sound preview (play, pause, seek, loop, volume) on one channel of the
//! shared `banished_audio::Device`, the same mixer the game uses.

use std::time::Duration;

use banished_audio::{Channel, Device, Pcm, PlayParams, Sound};

pub struct Player {
    device: Device,
    channel: Option<Channel>,
    volume: f32,
}

impl Default for Player {
    fn default() -> Self {
        Player {
            device: Device::default(),
            channel: None,
            volume: 1.0,
        }
    }
}

impl Player {
    fn channel(&self) -> Option<Channel> {
        self.channel.filter(|&c| self.device.is_active(c))
    }

    pub fn play(&mut self, pcm: Pcm, looping: bool) -> Result<(), String> {
        self.stop();
        let sound = Sound::new(pcm, looping);
        let params = PlayParams {
            volume: self.volume,
            looping: Some(looping),
            ..PlayParams::default()
        };
        self.channel = self.device.play(&sound, params)?;
        Ok(())
    }

    pub fn pause(&mut self) {
        if let Some(c) = self.channel() {
            self.device.pause(c);
        }
    }

    pub fn resume(&mut self) {
        if let Some(c) = self.channel() {
            self.device.resume(c);
        }
    }

    pub fn is_paused(&self) -> bool {
        self.channel().is_some_and(|c| self.device.is_paused(c))
    }

    pub fn is_playing(&self) -> bool {
        self.channel().is_some_and(|c| self.device.is_playing(c))
    }

    pub fn position(&self) -> Duration {
        self.channel
            .map_or(Duration::ZERO, |c| self.device.position(c))
    }

    pub fn seek(&mut self, position: Duration) -> Result<(), String> {
        match self.channel() {
            Some(c) => self.device.seek(c, position),
            None => Ok(()),
        }
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        if let Some(c) = self.channel() {
            self.device.set_volume(c, self.volume);
        }
    }

    pub fn set_looping(&mut self, looping: bool) -> Result<(), String> {
        if let Some(c) = self.channel() {
            self.device.set_looping(c, looping);
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub fn unlock(&mut self) {
        self.device.unlock();
    }

    pub fn stop(&mut self) {
        if let Some(c) = self.channel.take() {
            self.device.stop(c);
        }
        self.device.update();
    }
}
