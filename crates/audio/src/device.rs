//! A pool of concurrent voices, after the original `Audio::Device` (Runtime DLL): 64 channels
//! (`0x28` bytes each, flag bit 0 = in use, bit 1 = ended). `Device::Play` (`0x1800e0ad0`)
//! takes the first free channel and returns null when all are busy; `PlayPlatform`
//! (`0x1800e0ff0`) starts one XAudio2 source voice at `offset % duration`, with the given
//! volume and a frequency ratio of `1.05946314^pitch` (pitch in semitones), looping when the
//! sound buffer's loop flag is set. `Device::Update` (`0x1800e0d70`) frees ended channels and
//! calls their end callbacks; `Device::Stop` (`0x1800e0a50`) stops, frees and calls back.
//!
//! The port plays every channel at once through rodio's mixer natively and through one Web Audio
//! graph (a gain node per channel into a master gain) in the browser. The end callback becomes
//! the list of channels `update` returns.

use std::sync::Arc;
use std::time::Duration;

use crate::Pcm;

/// `Audio::Device` channel count (loops in `FindFreeChannel`, `Play`, `Update`).
pub const CHANNELS: usize = 64;

/// `DAT_180164810`: one semitone (the twelfth root of two).
const SEMITONE: f32 = 1.059_463_1;

/// XAudio2 frequency ratio for a pitch in semitones (`FUN_180129df0(1.05946314, pitch)`).
pub fn pitch_ratio(semitones: f32) -> f32 {
    SEMITONE.powf(semitones)
}

/// A decoded sound buffer (`Audio::SoundBuffer`) that any number of channels can play at once;
/// the samples are shared, not copied, per channel.
#[derive(Clone)]
pub struct Sound {
    pcm: Arc<Pcm>,
    /// The buffer's loop flag (`SoundBuffer +0x20`).
    pub looping: bool,
    #[cfg(target_arch = "wasm32")]
    web: std::rc::Rc<std::cell::RefCell<Option<web_sys::AudioBuffer>>>,
}

impl Sound {
    pub fn new(pcm: Pcm, looping: bool) -> Sound {
        Sound {
            pcm: Arc::new(pcm),
            looping,
            #[cfg(target_arch = "wasm32")]
            web: Default::default(),
        }
    }

    /// Load and decode a sound by resource reference (see [`crate::load_sound`]).
    pub fn load(
        archive: &banished_formats::pkg::Archive,
        reference: &str,
    ) -> Result<Sound, String> {
        let (pcm, looping) = crate::load_sound(archive, reference)?;
        Ok(Sound::new(pcm, looping))
    }

    pub fn pcm(&self) -> &Pcm {
        &self.pcm
    }

    pub fn frames(&self) -> usize {
        self.pcm.samples.len() / self.pcm.channels.max(1) as usize
    }

    pub fn duration(&self) -> Duration {
        Duration::from_secs_f64(self.frames() as f64 / self.pcm.sample_rate.max(1) as f64)
    }

    /// `PlayPlatform`'s start point: the requested time wrapped into the sound.
    pub fn wrap(&self, offset: Duration) -> Duration {
        let d = self.duration().as_secs_f64();
        if d <= 0.0 {
            return Duration::ZERO;
        }
        Duration::from_secs_f64(offset.as_secs_f64() % d)
    }
}

/// A playing channel. Handles carry a generation, so one kept after its channel was freed and
/// reused refers to nothing (calls on it do nothing).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Channel {
    index: u8,
    generation: u32,
}

impl Channel {
    pub fn index(self) -> usize {
        self.index as usize
    }

    #[cfg(test)]
    pub(crate) fn test(n: u32) -> Channel {
        Channel {
            index: (n % 64) as u8,
            generation: n,
        }
    }
}

/// Channel bookkeeping without any output: first-free allocation and generations.
#[derive(Debug)]
pub struct Slots<T> {
    slots: Vec<Option<(u32, T)>>,
    generation: u32,
}

impl<T> Default for Slots<T> {
    fn default() -> Self {
        Slots {
            slots: (0..CHANNELS).map(|_| None).collect(),
            generation: 0,
        }
    }
}

impl<T> Slots<T> {
    /// `FindFreeChannel`: the lowest free index, or `None` when all 64 are in use.
    pub fn insert(&mut self, value: T) -> Option<Channel> {
        let index = self.slots.iter().position(Option::is_none)?;
        self.generation = self.generation.wrapping_add(1);
        self.slots[index] = Some((self.generation, value));
        Some(Channel {
            index: index as u8,
            generation: self.generation,
        })
    }

    pub fn get(&self, c: Channel) -> Option<&T> {
        match self.slots.get(c.index())? {
            Some((g, v)) if *g == c.generation => Some(v),
            _ => None,
        }
    }

    pub fn get_mut(&mut self, c: Channel) -> Option<&mut T> {
        match self.slots.get_mut(c.index())? {
            Some((g, v)) if *g == c.generation => Some(v),
            _ => None,
        }
    }

    pub fn remove(&mut self, c: Channel) -> Option<T> {
        self.get(c)?;
        self.slots[c.index()].take().map(|(_, v)| v)
    }

    pub fn iter(&self) -> impl Iterator<Item = (Channel, &T)> {
        self.slots.iter().enumerate().filter_map(|(i, s)| {
            s.as_ref().map(|(g, v)| {
                (
                    Channel {
                        index: i as u8,
                        generation: *g,
                    },
                    v,
                )
            })
        })
    }

    pub fn len(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// How a channel starts (`Device::Play`'s time and floats).
#[derive(Debug, Clone, Copy)]
pub struct PlayParams {
    /// Start time, wrapped into the sound.
    pub offset: Duration,
    pub volume: f32,
    /// Semitones.
    pub pitch: f32,
    /// Defaults to the sound's own loop flag.
    pub looping: Option<bool>,
}

impl Default for PlayParams {
    fn default() -> Self {
        PlayParams {
            offset: Duration::ZERO,
            volume: 1.0,
            pitch: 0.0,
            looping: None,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::Device;
#[cfg(target_arch = "wasm32")]
pub use web::Device;

#[cfg(not(target_arch = "wasm32"))]
mod native {
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
        pub fn play(
            &mut self,
            sound: &Sound,
            params: PlayParams,
        ) -> Result<Option<Channel>, String> {
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

        pub fn is_active(&self, c: Channel) -> bool {
            self.voices.get(c).is_some_and(|v| !v.sink.empty())
        }

        pub fn active(&self) -> usize {
            self.voices.len()
        }

        pub fn pause(&mut self, c: Channel) {
            if let Some(v) = self.voices.get(c) {
                v.sink.pause();
            }
        }

        pub fn resume(&mut self, c: Channel) {
            if let Some(v) = self.voices.get(c) {
                v.sink.play();
            }
        }

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

        pub fn set_pitch(&mut self, c: Channel, semitones: f32) {
            if let Some(v) = self.voices.get(c) {
                v.sink.set_speed(pitch_ratio(semitones));
            }
        }

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
}

#[cfg(target_arch = "wasm32")]
mod web {
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

        fn buffer(
            ctx: &web_sys::AudioContext,
            sound: &Sound,
        ) -> Result<web_sys::AudioBuffer, String> {
            if let Some(b) = sound.web.borrow().as_ref() {
                return Ok(b.clone());
            }
            let pcm = sound.pcm();
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
        pub fn play(
            &mut self,
            sound: &Sound,
            params: PlayParams,
        ) -> Result<Option<Channel>, String> {
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

        pub fn is_active(&self, c: Channel) -> bool {
            let now = self.now();
            self.voices.get(c).is_some_and(|v| !Self::ended(now, v))
        }

        pub fn active(&self) -> usize {
            self.voices.len()
        }

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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(frames: usize, channels: u16) -> Sound {
        Sound::new(
            Pcm {
                channels,
                sample_rate: 100,
                samples: (0..frames * channels as usize).map(|i| i as f32).collect(),
            },
            false,
        )
    }

    #[test]
    fn first_free_channel_and_stale_handles() {
        let mut s = Slots::default();
        let a = s.insert('a').unwrap();
        let b = s.insert('b').unwrap();
        assert_eq!((a.index(), b.index()), (0, 1));
        assert_eq!(s.remove(a), Some('a'));
        // The freed channel is reused first; the old handle no longer reaches it.
        let c = s.insert('c').unwrap();
        assert_eq!(c.index(), 0);
        assert_eq!(s.get(a), None);
        assert_eq!(s.get(c), Some(&'c'));
        for _ in 2..CHANNELS {
            assert!(s.insert('x').is_some());
        }
        assert_eq!(s.len(), CHANNELS);
        assert!(
            s.insert('y').is_none(),
            "64 channels, as Device::FindFreeChannel"
        );
    }

    #[test]
    fn wrap_and_pitch() {
        let t = tone(250, 2);
        assert_eq!(t.duration(), Duration::from_millis(2500));
        assert_eq!(
            t.wrap(Duration::from_millis(3000)),
            Duration::from_millis(500)
        );
        assert!((pitch_ratio(12.0) - 2.0).abs() < 1e-4);
        assert_eq!(pitch_ratio(0.0), 1.0);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn shared_source_loops_seeks_and_shares_samples() {
        use rodio::Source;
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        let t = tone(3, 2);
        let cursor = Arc::new(AtomicUsize::new(0));
        let looping = Arc::new(AtomicBool::new(true));
        let mut src = native::SharedSource {
            sound: t.clone(),
            cursor: cursor.clone(),
            looping: looping.clone(),
        };
        let first: Vec<f32> = (&mut src).take(8).collect();
        assert_eq!(first, vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 0.0, 1.0]);
        src.try_seek(Duration::from_millis(20)).unwrap();
        assert_eq!(cursor.load(Ordering::Relaxed), 4);
        // Many channels can hold one buffer.
        assert_eq!(Arc::strong_count(&t.pcm), 2);
        looping.store(false, Ordering::Relaxed);
        assert_eq!(src.collect::<Vec<_>>(), vec![4.0, 5.0]);
    }

    /// Needs an audio output: `cargo test -p banished-audio -- --ignored`.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    #[ignore]
    fn channels_play_together_and_free_on_end() {
        let mut device = Device::default();
        device.set_master_volume(0.0);
        let long = tone(100, 1);
        let short = tone(5, 1);
        let music = device
            .play(
                &long,
                PlayParams {
                    looping: Some(true),
                    ..Default::default()
                },
            )
            .unwrap()
            .unwrap();
        let effect = device.play(&short, PlayParams::default()).unwrap().unwrap();
        assert_ne!(music.index(), effect.index());
        assert!(device.is_playing(music) && device.is_active(effect));
        std::thread::sleep(Duration::from_millis(300));
        // The short effect ended and is freed; the looping music keeps its channel.
        assert_eq!(device.update(), vec![effect]);
        assert!(device.is_playing(music));
        assert_eq!(device.active(), 1);
        device.stop(music);
        assert_eq!(device.active(), 0);
    }
}
