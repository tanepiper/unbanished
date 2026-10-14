//! Sound playback. Game audio is xWMA (WMA v2), decoded in-process by `banished-wma`, then played
//! with rodio natively or Web Audio in the browser. No exported files are needed.

use banished_formats::types::SoundBuffer;

#[derive(Clone)]
pub struct Pcm {
    pub channels: u16,
    pub sample_rate: u32,
    /// Interleaved samples in [-1, 1).
    pub samples: Vec<f32>,
}

/// Decode a `SoundBuffer`; `stream` supplies the packets of streamed music (its `.str` entry).
pub fn decode(sound: &SoundBuffer, stream: Option<&[u8]>) -> Result<Pcm, String> {
    let f = &sound.format;
    let params = banished_wma::Params::xwma(
        f.channels,
        f.sample_rate,
        f.avg_bytes_per_sec,
        f.block_align,
    );
    let packets = stream.unwrap_or(&sound.data);
    let pcm = banished_wma::decode(&params, packets).map_err(|e| format!("decode: {e:?}"))?;
    Ok(Pcm {
        channels: f.channels,
        sample_rate: f.sample_rate,
        samples: pcm.into_iter().map(|s| s as f32 / 32768.0).collect(),
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub struct Player {
    stream: Option<rodio::OutputStream>,
    sink: Option<rodio::Sink>,
    pcm: Option<Pcm>,
    duration: std::time::Duration,
    looping: bool,
    volume: f32,
}

#[cfg(not(target_arch = "wasm32"))]
impl Default for Player {
    fn default() -> Self {
        Self {
            stream: None,
            sink: None,
            pcm: None,
            duration: std::time::Duration::ZERO,
            looping: false,
            volume: 1.0,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Player {
    pub fn play(&mut self, pcm: Pcm, looping: bool) -> Result<(), String> {
        self.stop();
        self.duration = std::time::Duration::from_secs_f64(
            pcm.samples.len() as f64 / pcm.channels.max(1) as f64 / pcm.sample_rate.max(1) as f64,
        );
        self.looping = looping;
        self.pcm = Some(pcm);
        self.start_source(std::time::Duration::ZERO)
    }

    fn start_source(&mut self, position: std::time::Duration) -> Result<(), String> {
        if self.stream.is_none() {
            let mut stream =
                rodio::OutputStreamBuilder::open_default_stream().map_err(|e| e.to_string())?;
            stream.log_on_drop(false);
            self.stream = Some(stream);
        }
        let sink = rodio::Sink::connect_new(self.stream.as_ref().unwrap().mixer());
        sink.set_volume(self.volume.max(0.0));
        let pcm = self
            .pcm
            .as_ref()
            .ok_or_else(|| "no decoded audio is loaded".to_owned())?;
        let buffer =
            rodio::buffer::SamplesBuffer::new(pcm.channels, pcm.sample_rate, pcm.samples.clone());
        if self.looping {
            use rodio::Source;
            sink.append(buffer.repeat_infinite());
        } else {
            sink.append(buffer);
        }
        if !position.is_zero() {
            sink.try_seek(position).map_err(|e| e.to_string())?;
        }
        self.sink = Some(sink);
        Ok(())
    }

    pub fn pause(&self) {
        if let Some(sink) = &self.sink {
            sink.pause();
        }
    }

    pub fn resume(&self) {
        if let Some(sink) = &self.sink {
            sink.play();
        }
    }

    pub fn is_paused(&self) -> bool {
        self.sink.as_ref().is_some_and(rodio::Sink::is_paused)
    }

    pub fn is_playing(&self) -> bool {
        self.sink
            .as_ref()
            .is_some_and(|sink| !sink.is_paused() && !sink.empty())
    }

    pub fn position(&self) -> std::time::Duration {
        let Some(sink) = &self.sink else {
            return std::time::Duration::ZERO;
        };
        let position = sink.get_pos();
        if self.looping && !self.duration.is_zero() {
            std::time::Duration::from_secs_f64(position.as_secs_f64() % self.duration.as_secs_f64())
        } else {
            position.min(self.duration)
        }
    }

    pub fn seek(&self, position: std::time::Duration) -> Result<(), String> {
        if let Some(sink) = &self.sink {
            sink.try_seek(position.min(self.duration))
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        if let Some(sink) = &self.sink {
            sink.set_volume(self.volume);
        }
    }

    pub fn set_looping(&mut self, looping: bool) -> Result<(), String> {
        if self.looping == looping {
            return Ok(());
        }
        let position = self.position();
        let Some(sink) = self.sink.take() else {
            self.looping = looping;
            return Ok(());
        };
        let paused = sink.is_paused();
        sink.stop();
        self.looping = looping;
        self.start_source(position)?;
        if paused {
            self.pause();
        }
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(s) = self.sink.take() {
            s.stop();
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub struct Player {
    context: Option<web_sys::AudioContext>,
    source: Option<web_sys::AudioBufferSourceNode>,
    buffer: Option<web_sys::AudioBuffer>,
    gain: Option<web_sys::GainNode>,
    duration: f64,
    looping: bool,
    volume: f32,
    paused: bool,
    offset: f64,
    started_at: f64,
}

#[cfg(target_arch = "wasm32")]
impl Default for Player {
    fn default() -> Self {
        Self {
            context: None,
            source: None,
            buffer: None,
            gain: None,
            duration: 0.0,
            looping: false,
            volume: 1.0,
            paused: false,
            offset: 0.0,
            started_at: 0.0,
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Player {
    pub fn play(&mut self, pcm: Pcm, looping: bool) -> Result<(), String> {
        let err = |e: wasm_bindgen::JsValue| format!("{e:?}");
        let channels = pcm.channels as usize;
        if channels == 0
            || pcm.sample_rate == 0
            || pcm.samples.is_empty()
            || pcm.samples.len() % channels != 0
        {
            return Err(
                "PCM must contain complete frames with a channel count and sample rate".into(),
            );
        }
        let frames = u32::try_from(pcm.samples.len() / channels)
            .map_err(|_| "PCM is too long for Web Audio")?;
        self.stop();
        if self.context.is_none() {
            self.context = Some(web_sys::AudioContext::new().map_err(err)?);
        }
        let ctx = self.context.as_ref().unwrap().clone();
        let buffer = ctx
            .create_buffer(channels as u32, frames, pcm.sample_rate as f32)
            .map_err(err)?;
        for ch in 0..channels {
            let mut plane: Vec<f32> = pcm
                .samples
                .iter()
                .skip(ch)
                .step_by(channels)
                .copied()
                .collect();
            buffer.copy_to_channel(&mut plane, ch as i32).map_err(err)?;
        }
        self.duration = buffer.duration();
        self.buffer = Some(buffer);
        self.looping = looping;
        self.offset = 0.0;
        self.paused = false;
        if self.gain.is_none() {
            let gain = ctx.create_gain().map_err(err)?;
            gain.gain().set_value(self.volume);
            gain.connect_with_audio_node(&ctx.destination())
                .map_err(err)?;
            self.gain = Some(gain);
        }
        self.start_source(0.0)?;
        let resume = ctx.resume().map_err(err)?;
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = wasm_bindgen_futures::JsFuture::from(resume).await {
                log::error!("Could not resume Web Audio: {e:?}");
            }
        });
        Ok(())
    }

    fn start_source(&mut self, offset: f64) -> Result<(), String> {
        let err = |e: wasm_bindgen::JsValue| format!("{e:?}");
        let ctx = self
            .context
            .as_ref()
            .ok_or_else(|| "audio context is unavailable".to_owned())?;
        let buffer = self
            .buffer
            .as_ref()
            .ok_or_else(|| "no decoded audio is loaded".to_owned())?;
        let source = ctx.create_buffer_source().map_err(err)?;
        source.set_buffer(Some(buffer));
        source.set_loop(self.looping);
        let output = self
            .gain
            .as_ref()
            .ok_or_else(|| "audio output is unavailable".to_owned())?;
        source.connect_with_audio_node(output).map_err(err)?;
        self.started_at = ctx.current_time();
        self.offset = offset.clamp(0.0, self.duration);
        source
            .start_with_when_and_grain_offset(0.0, self.offset)
            .map_err(err)?;
        self.source = Some(source);
        Ok(())
    }

    pub fn pause(&mut self) {
        if self.source.is_some() && !self.paused {
            self.offset = self.position().as_secs_f64();
            self.paused = true;
            if let Some(context) = &self.context {
                if let Ok(promise) = context.suspend() {
                    wasm_bindgen_futures::spawn_local(async move {
                        let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
                    });
                }
            }
        }
    }

    pub fn resume(&mut self) {
        if self.source.is_some() && self.paused {
            self.started_at = self.context.as_ref().map_or(0.0, |ctx| ctx.current_time());
            self.paused = false;
            if let Some(context) = &self.context {
                if let Ok(promise) = context.resume() {
                    wasm_bindgen_futures::spawn_local(async move {
                        let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
                    });
                }
            }
        }
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn is_playing(&self) -> bool {
        self.source.is_some() && !self.paused && self.position().as_secs_f64() < self.duration
    }

    pub fn position(&self) -> std::time::Duration {
        let seconds = if self.paused || self.source.is_none() {
            self.offset
        } else {
            self.offset + self.context.as_ref().map_or(0.0, |ctx| ctx.current_time())
                - self.started_at
        };
        let seconds = if self.looping && self.duration > 0.0 {
            seconds.rem_euclid(self.duration)
        } else {
            seconds.clamp(0.0, self.duration)
        };
        std::time::Duration::from_secs_f64(seconds.max(0.0))
    }

    pub fn seek(&mut self, position: std::time::Duration) -> Result<(), String> {
        let offset = position.as_secs_f64().clamp(0.0, self.duration);
        if self.source.is_some() {
            if let Some(source) = self.source.take() {
                #[allow(deprecated)]
                let _ = source.stop();
                let _ = source.disconnect();
            }
            self.start_source(offset)?;
            if self.paused {
                if let Some(context) = &self.context {
                    if let Ok(promise) = context.suspend() {
                        wasm_bindgen_futures::spawn_local(async move {
                            let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
                        });
                    }
                }
            }
        } else {
            self.offset = offset;
        }
        Ok(())
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        if let Some(gain) = &self.gain {
            gain.gain().set_value(self.volume);
        }
    }

    pub fn set_looping(&mut self, looping: bool) -> Result<(), String> {
        self.looping = looping;
        if let Some(source) = &self.source {
            source.set_loop(looping);
        }
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(s) = self.source.take() {
            #[allow(deprecated)]
            let _ = s.stop();
            let _ = s.disconnect();
        }
        self.paused = false;
        self.offset = 0.0;
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for Player {
    fn drop(&mut self) {
        self.stop();
        if let Some(context) = self.context.take() {
            let _ = context.close();
        }
    }
}
