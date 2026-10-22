//! The native audio device: rodio sources that play the channels.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::{pitch_ratio, Channel, PlayParams, Slots, Sound};

/// A rodio source over a shared buffer. Position and looping are shared with the channel,
/// so they can be read and changed while it plays.
pub(crate) struct SharedSource {
    pub(crate) sound: Sound,
    /// Next sample index (interleaved).
    pub(crate) cursor: Arc<AtomicUsize>,
    pub(crate) looping: Arc<AtomicBool>,
}

impl Iterator for SharedSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let samples = &self.sound.pcm.samples;
        let mut i = self.cursor.load(Ordering::Relaxed);
        if i >= samples.len() {
            if !self.looping.load(Ordering::Relaxed) || samples.is_empty() {
                return None;
            }
            i = 0;
        }
        self.cursor.store(i + 1, Ordering::Relaxed);
        Some(samples[i])
    }
}

impl rodio::Source for SharedSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> rodio::ChannelCount {
        self.sound.pcm.channels.max(1)
    }

    fn sample_rate(&self) -> rodio::SampleRate {
        self.sound.pcm.sample_rate.max(1)
    }

    fn total_duration(&self) -> Option<Duration> {
        (!self.looping.load(Ordering::Relaxed)).then(|| self.sound.duration())
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), rodio::source::SeekError> {
        let channels = self.sound.pcm.channels.max(1) as usize;
        let frame = (pos.as_secs_f64() * self.sound.pcm.sample_rate as f64) as usize;
        let i = (frame.min(self.sound.frames())) * channels;
        self.cursor.store(i, Ordering::Relaxed);
        Ok(())
    }
}

struct Voice {
    sink: rodio::Sink,
    sound: Sound,
    cursor: Arc<AtomicUsize>,
    looping: Arc<AtomicBool>,
    volume: f32,
}

/// The multi-channel output (rodio).
pub struct Device {
    stream: Option<rodio::OutputStream>,
    voices: Slots<Voice>,
    master: f32,
}

impl Default for Device {
    fn default() -> Self {
        Device {
            stream: None,
            voices: Slots::default(),
            master: 1.0,
        }
    }
}

impl Device {
    fn mixer(&mut self) -> Result<&rodio::mixer::Mixer, String> {
        if self.stream.is_none() {
            let mut stream =
                rodio::OutputStreamBuilder::open_default_stream().map_err(|e| e.to_string())?;
            stream.log_on_drop(false);
            self.stream = Some(stream);
        }
        Ok(self.stream.as_ref().unwrap().mixer())
    }

    /// `Device::Play`: start `sound` on the first free channel. `Ok(None)` when all 64 are
    /// busy (the original returns null); `Err` when there is no audio output.
    pub fn play(&mut self, sound: &Sound, params: PlayParams) -> Result<Option<Channel>, String> {
        if self.voices.len() >= super::CHANNELS {
            return Ok(None);
        }
        let sink = rodio::Sink::connect_new(self.mixer()?);
        let looping = Arc::new(AtomicBool::new(params.looping.unwrap_or(sound.looping)));
        let channels = sound.pcm.channels.max(1) as usize;
        let start = sound.wrap(params.offset);
        let frame = (start.as_secs_f64() * sound.pcm.sample_rate as f64) as usize;
        let cursor = Arc::new(AtomicUsize::new(frame.min(sound.frames()) * channels));
        sink.set_volume((params.volume * self.master).max(0.0));
        sink.set_speed(pitch_ratio(params.pitch));
        sink.append(SharedSource {
            sound: sound.clone(),
            cursor: cursor.clone(),
            looping: looping.clone(),
        });
        Ok(self.voices.insert(Voice {
            sink,
            sound: sound.clone(),
            cursor,
            looping,
            volume: params.volume,
        }))
    }

    /// `Device::Stop`.
    pub fn stop(&mut self, c: Channel) {
        if let Some(v) = self.voices.remove(c) {
            v.sink.stop();
        }
    }

    /// `Scene::StopAll`.
    pub fn stop_all(&mut self) {
        let all: Vec<_> = self.voices.iter().map(|(c, _)| c).collect();
        for c in all {
            self.stop(c);
        }
    }

    /// `Device::Update`: free the channels that finished, returning them (the end
    /// callbacks).
    pub fn update(&mut self) -> Vec<Channel> {
        let ended: Vec<_> = self
            .voices
            .iter()
            .filter(|(_, v)| v.sink.empty())
            .map(|(c, _)| c)
            .collect();
        for &c in &ended {
            self.voices.remove(c);
        }
        ended
    }

    /// Whether the channel is still allocated and has not finished.
    pub fn is_playing(&self, c: Channel) -> bool {
        self.voices
            .get(c)
            .is_some_and(|v| !v.sink.empty() && !v.sink.is_paused())
    }

    /// Whether the channel is allocated and has not finished, paused or not.
    pub fn is_active(&self, c: Channel) -> bool {
        self.voices.get(c).is_some_and(|v| !v.sink.empty())
    }

    /// The number of channels in use.
    pub fn active(&self) -> usize {
        self.voices.len()
    }

    /// Pause a channel. The other channels go on.
    pub fn pause(&mut self, c: Channel) {
        if let Some(v) = self.voices.get(c) {
            v.sink.pause();
        }
    }

    /// Resume a paused channel.
    pub fn resume(&mut self, c: Channel) {
        if let Some(v) = self.voices.get(c) {
            v.sink.play();
        }
    }

    /// Whether the channel is paused.
    pub fn is_paused(&self, c: Channel) -> bool {
        self.voices.get(c).is_some_and(|v| v.sink.is_paused())
    }

    /// Position in the sound (wrapped for looping channels).
    pub fn position(&self, c: Channel) -> Duration {
        let Some(v) = self.voices.get(c) else {
            return Duration::ZERO;
        };
        let channels = v.sound.pcm.channels.max(1) as usize;
        let frame = v.cursor.load(Ordering::Relaxed) / channels;
        Duration::from_secs_f64(frame as f64 / v.sound.pcm.sample_rate.max(1) as f64)
            .min(v.sound.duration())
    }

    /// Move a channel to a position in its sound.
    pub fn seek(&mut self, c: Channel, position: Duration) -> Result<(), String> {
        if let Some(v) = self.voices.get(c) {
            v.sink
                .try_seek(position.min(v.sound.duration()))
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// `Device::SetChannelVolume`.
    pub fn set_volume(&mut self, c: Channel, volume: f32) {
        let master = self.master;
        if let Some(v) = self.voices.get_mut(c) {
            v.volume = volume.max(0.0);
            v.sink.set_volume(v.volume * master);
        }
    }

    /// Change a channel's pitch, in semitones.
    pub fn set_pitch(&mut self, c: Channel, semitones: f32) {
        if let Some(v) = self.voices.get(c) {
            v.sink.set_speed(pitch_ratio(semitones));
        }
    }

    /// Change whether a channel loops.
    pub fn set_looping(&mut self, c: Channel, looping: bool) {
        if let Some(v) = self.voices.get(c) {
            v.looping.store(looping, Ordering::Relaxed);
        }
    }

    /// Scales every channel.
    pub fn set_master_volume(&mut self, volume: f32) {
        self.master = volume.max(0.0);
        for (_, v) in self.voices.iter() {
            v.sink.set_volume(v.volume * self.master);
        }
    }

    /// Browsers start audio suspended until a user gesture; nothing to do natively.
    pub fn unlock(&mut self) {}
}
