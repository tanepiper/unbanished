//! Sound playback. Game audio is xWMA (WMA v2), decoded in-process by `banished-wma`, then played
//! with rodio natively or Web Audio in the browser. No exported files are needed.

use banished_formats::types::SoundBuffer;

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
#[derive(Default)]
pub struct Player {
    stream: Option<rodio::OutputStream>,
    sink: Option<rodio::Sink>,
}

#[cfg(not(target_arch = "wasm32"))]
impl Player {
    pub fn play(&mut self, pcm: Pcm, looping: bool) -> Result<(), String> {
        self.stop();
        if self.stream.is_none() {
            let mut stream =
                rodio::OutputStreamBuilder::open_default_stream().map_err(|e| e.to_string())?;
            stream.log_on_drop(false);
            self.stream = Some(stream);
        }
        let sink = rodio::Sink::connect_new(self.stream.as_ref().unwrap().mixer());
        let buffer = rodio::buffer::SamplesBuffer::new(pcm.channels, pcm.sample_rate, pcm.samples);
        if looping {
            use rodio::Source;
            sink.append(buffer.repeat_infinite());
        } else {
            sink.append(buffer);
        }
        self.sink = Some(sink);
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(s) = self.sink.take() {
            s.stop();
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(Default)]
pub struct Player {
    context: Option<web_sys::AudioContext>,
    source: Option<web_sys::AudioBufferSourceNode>,
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
        let ctx = self.context.as_ref().unwrap();
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
        let source = ctx.create_buffer_source().map_err(err)?;
        source.set_buffer(Some(&buffer));
        source.set_loop(looping);
        source
            .connect_with_audio_node(&ctx.destination())
            .map_err(err)?;
        let resume = ctx.resume().map_err(err)?;
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = wasm_bindgen_futures::JsFuture::from(resume).await {
                log::error!("Could not resume Web Audio: {e:?}");
            }
        });
        source.start().map_err(err)?;
        self.source = Some(source);
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(s) = self.source.take() {
            #[allow(deprecated)]
            let _ = s.stop();
            let _ = s.disconnect();
        }
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
