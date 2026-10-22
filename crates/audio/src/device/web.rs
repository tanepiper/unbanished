//! The browser audio device: Web Audio buffers and source nodes for the channels.

use std::time::Duration;

use super::{pitch_ratio, Channel, PlayParams, Slots, Sound};

fn err(e: wasm_bindgen::JsValue) -> String {
    format!("{e:?}")
}

fn spawn(promise: Result<js_sys::Promise, wasm_bindgen::JsValue>) {
    if let Ok(p) = promise {
        wasm_bindgen_futures::spawn_local(async move {
            let _ = wasm_bindgen_futures::JsFuture::from(p).await;
        });
    }
}

struct Voice {
    sound: Sound,
    source: Option<web_sys::AudioBufferSourceNode>,
    gain: web_sys::GainNode,
    looping: bool,
    rate: f64,
    volume: f32,
    /// Sound time at `started_at` (or while paused).
    offset: f64,
    started_at: f64,
    paused: bool,
}

/// The multi-channel output (Web Audio): one buffer source and gain node per channel,
/// into a master gain.
pub struct Device {
    context: Option<web_sys::AudioContext>,
    master_gain: Option<web_sys::GainNode>,
    voices: Slots<Voice>,
    master: f32,
}

impl Default for Device {
    fn default() -> Self {
        Device {
            context: None,
            master_gain: None,
            voices: Slots::default(),
            master: 1.0,
        }
    }
}

impl Device {
    fn context(&mut self) -> Result<(web_sys::AudioContext, web_sys::GainNode), String> {
        if self.context.is_none() {
            let ctx = web_sys::AudioContext::new().map_err(err)?;
            let gain = ctx.create_gain().map_err(err)?;
            gain.gain().set_value(self.master);
            gain.connect_with_audio_node(&ctx.destination())
                .map_err(err)?;
            self.context = Some(ctx);
            self.master_gain = Some(gain);
        }
        Ok((
            self.context.clone().unwrap(),
            self.master_gain.clone().unwrap(),
        ))
    }

    fn buffer(ctx: &web_sys::AudioContext, sound: &Sound) -> Result<web_sys::AudioBuffer, String> {
        if let Some(b) = sound.web.borrow().as_ref() {
            return Ok(b.clone());
        }
        let pcm = sound.pcm();
        let channels = pcm.channels as usize;
        if channels == 0
            || pcm.sample_rate == 0
            || pcm.samples.is_empty()
            || !pcm.samples.len().is_multiple_of(channels)
        {
            return Err(
                "PCM must contain complete frames with a channel count and sample rate".into(),
            );
        }
        let frames = u32::try_from(pcm.samples.len() / channels)
            .map_err(|_| "PCM is too long for Web Audio")?;
        let buffer = ctx
            .create_buffer(channels as u32, frames, pcm.sample_rate as f32)
            .map_err(err)?;
        for ch in 0..channels {
            let plane: Vec<f32> = pcm
                .samples
                .iter()
                .skip(ch)
                .step_by(channels)
                .copied()
                .collect();
            buffer.copy_to_channel(&plane, ch as i32).map_err(err)?;
        }
        *sound.web.borrow_mut() = Some(buffer.clone());
        Ok(buffer)
    }

    fn now(&self) -> f64 {
        self.context.as_ref().map_or(0.0, |c| c.current_time())
    }

    fn start(ctx: &web_sys::AudioContext, v: &mut Voice) -> Result<(), String> {
        let buffer = Self::buffer(ctx, &v.sound)?;
        let source = ctx.create_buffer_source().map_err(err)?;
        source.set_buffer(Some(&buffer));
        source.set_loop(v.looping);
        source.playback_rate().set_value(v.rate as f32);
        source.connect_with_audio_node(&v.gain).map_err(err)?;
        v.started_at = ctx.current_time();
        source
            .start_with_when_and_grain_offset(0.0, v.offset)
            .map_err(err)?;
        v.source = Some(source);
        Ok(())
    }

    fn halt(v: &mut Voice) {
        if let Some(s) = v.source.take() {
            #[allow(deprecated)]
            let _ = s.stop();
            let _ = s.disconnect();
        }
    }

    /// Unwrapped sound time of a voice.
    fn time(now: f64, v: &Voice) -> f64 {
        if v.paused || v.source.is_none() {
            v.offset
        } else {
            v.offset + (now - v.started_at) * v.rate
        }
    }

    fn ended(now: f64, v: &Voice) -> bool {
        !v.looping && !v.paused && Self::time(now, v) >= v.sound.duration().as_secs_f64()
    }

    /// `Device::Play`: see the native backend.
    pub fn play(&mut self, sound: &Sound, params: PlayParams) -> Result<Option<Channel>, String> {
        if self.voices.len() >= super::CHANNELS {
            return Ok(None);
        }
        let (ctx, master) = self.context()?;
        let gain = ctx.create_gain().map_err(err)?;
        gain.gain().set_value(params.volume.max(0.0));
        gain.connect_with_audio_node(&master).map_err(err)?;
        let mut v = Voice {
            sound: sound.clone(),
            source: None,
            gain,
            looping: params.looping.unwrap_or(sound.looping),
            rate: pitch_ratio(params.pitch) as f64,
            volume: params.volume,
            offset: sound.wrap(params.offset).as_secs_f64(),
            started_at: 0.0,
            paused: false,
        };
        Self::start(&ctx, &mut v)?;
        spawn(ctx.resume());
        Ok(self.voices.insert(v))
    }

    pub fn stop(&mut self, c: Channel) {
        if let Some(mut v) = self.voices.remove(c) {
            Self::halt(&mut v);
            let _ = v.gain.disconnect();
        }
    }

    pub fn stop_all(&mut self) {
        let all: Vec<_> = self.voices.iter().map(|(c, _)| c).collect();
        for c in all {
            self.stop(c);
        }
    }

    pub fn update(&mut self) -> Vec<Channel> {
        let now = self.now();
        let ended: Vec<_> = self
            .voices
            .iter()
            .filter(|(_, v)| Self::ended(now, v))
            .map(|(c, _)| c)
            .collect();
        for &c in &ended {
            self.stop(c);
        }
        ended
    }

    pub fn is_playing(&self, c: Channel) -> bool {
        let now = self.now();
        self.voices
            .get(c)
            .is_some_and(|v| !v.paused && !Self::ended(now, v))
    }

    /// Whether the channel is allocated and has not finished, paused or not.
    pub fn is_active(&self, c: Channel) -> bool {
        let now = self.now();
        self.voices.get(c).is_some_and(|v| !Self::ended(now, v))
    }

    /// The number of channels in use.
    pub fn active(&self) -> usize {
        self.voices.len()
    }

    /// Pause a channel. The other channels go on.
    pub fn pause(&mut self, c: Channel) {
        let now = self.now();
        if let Some(v) = self.voices.get_mut(c) {
            if !v.paused {
                v.offset = Self::time(now, v);
                if v.looping {
                    v.offset = v
                        .offset
                        .rem_euclid(v.sound.duration().as_secs_f64().max(1e-9));
                }
                Self::halt(v);
                v.paused = true;
            }
        }
    }

    /// Resume a paused channel.
    pub fn resume(&mut self, c: Channel) {
        let Some(ctx) = self.context.clone() else {
            return;
        };
        if let Some(v) = self.voices.get_mut(c) {
            if v.paused {
                v.paused = false;
                if let Err(e) = Self::start(&ctx, v) {
                    log::error!("resume: {e}");
                }
            }
        }
    }

    /// Whether the channel is paused.
    pub fn is_paused(&self, c: Channel) -> bool {
        self.voices.get(c).is_some_and(|v| v.paused)
    }

    pub fn position(&self, c: Channel) -> Duration {
        let Some(v) = self.voices.get(c) else {
            return Duration::ZERO;
        };
        let d = v.sound.duration().as_secs_f64();
        let t = Self::time(self.now(), v);
        let t = if v.looping && d > 0.0 {
            t.rem_euclid(d)
        } else {
            t.clamp(0.0, d)
        };
        Duration::from_secs_f64(t.max(0.0))
    }

    /// Move a channel to a position in its sound.
    pub fn seek(&mut self, c: Channel, position: Duration) -> Result<(), String> {
        let Some(ctx) = self.context.clone() else {
            return Ok(());
        };
        if let Some(v) = self.voices.get_mut(c) {
            v.offset = position
                .as_secs_f64()
                .clamp(0.0, v.sound.duration().as_secs_f64());
            if !v.paused {
                Self::halt(v);
                Self::start(&ctx, v)?;
            }
        }
        Ok(())
    }

    pub fn set_volume(&mut self, c: Channel, volume: f32) {
        if let Some(v) = self.voices.get_mut(c) {
            v.volume = volume.max(0.0);
            v.gain.gain().set_value(v.volume);
        }
    }

    /// Change a channel's pitch, in semitones.
    pub fn set_pitch(&mut self, c: Channel, semitones: f32) {
        let now = self.now();
        if let Some(v) = self.voices.get_mut(c) {
            // Re-base the clock so the position stays continuous.
            v.offset = Self::time(now, v);
            v.started_at = now;
            v.rate = pitch_ratio(semitones) as f64;
            if let Some(s) = &v.source {
                s.playback_rate().set_value(v.rate as f32);
            }
        }
    }

    /// Change whether a channel loops.
    pub fn set_looping(&mut self, c: Channel, looping: bool) {
        if let Some(v) = self.voices.get_mut(c) {
            v.looping = looping;
            if let Some(s) = &v.source {
                s.set_loop(looping);
            }
        }
    }

    pub fn set_master_volume(&mut self, volume: f32) {
        self.master = volume.max(0.0);
        if let Some(g) = &self.master_gain {
            g.gain().set_value(self.master);
        }
    }

    /// Resume a context the browser created suspended (autoplay policy). Call from a user
    /// gesture handler such as a click or key press.
    pub fn unlock(&mut self) {
        if let Some(context) = &self.context {
            if context.state() == web_sys::AudioContextState::Suspended {
                log::info!("resuming Web Audio after a user gesture");
                spawn(context.resume());
            }
        }
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        self.stop_all();
        if let Some(context) = self.context.take() {
            let _ = context.close();
        }
    }
}
