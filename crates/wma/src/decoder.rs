//! WMA v2 superframe, frame and block decoding.

use crate::bits::BitReader;
use crate::mdct::Imdct;
use crate::tables;
use crate::vlc::Vlc;
use crate::{Error, Params, Result};

const BLOCK_MIN_BITS: u32 = 7;
const NOISE_TAB_SIZE: usize = 8192;
const MAX_RESERVOIR_BYTES: usize = 1 << 15;

/// Per block size layout of the spectrum.
struct Layout {
    /// Exponent band widths in coefficients; they sum to the block length.
    exponent_bands: Vec<usize>,
    /// One past the last coded coefficient.
    coefs_end: usize,
    /// First coefficient of the noise-substituted high band region.
    high_band_start: usize,
    /// Widths of the exponent bands clipped to `high_band_start..coefs_end`.
    high_bands: Vec<usize>,
}

#[derive(Default)]
struct Channel {
    coded: bool,
    /// Exponents at the resolution of the block size they were decoded for (`exp_bsize`).
    exponents: Vec<f32>,
    exp_bsize: usize,
    exp_ready: bool,
    max_exponent: f32,
    /// Quantised coefficients, then the dequantised spectrum.
    levels: Vec<f32>,
    spectrum: Vec<f32>,
    high_band_coded: Vec<bool>,
    high_band_values: Vec<i32>,
    /// Two frames of overlap-add output; the first is complete after each frame.
    frame_out: Vec<f32>,
}

/// A WMA v2 decoder for one stream. Give it the packets in order with
/// [`decode_packet`](Self::decode_packet), then call [`flush`](Self::flush) once at the end.
pub struct Decoder {
    channels: usize,
    sample_rate: u32,
    frame_len_bits: u32,
    frame_len: usize,
    nb_block_sizes: usize,
    use_exp_vlc: bool,
    use_bit_reservoir: bool,
    use_variable_block_len: bool,
    use_noise_coding: bool,
    byte_offset_bits: u32,
    noise_mult: f32,
    noise_table: Vec<f32>,
    noise_index: usize,
    layouts: Vec<Layout>,
    coef_vlc: [Vlc; 2],
    coef_runlevel: [&'static [(u16, u16)]; 2],
    exp_vlc: Vlc,
    hgain_vlc: Vlc,
    imdct: Vec<Imdct>,
    windows: Vec<Vec<f32>>,
    imdct_out: Vec<f32>,
    ch: Vec<Channel>,

    reset_block_lengths: bool,
    prev_block_len_bits: u32,
    block_len_bits: u32,
    next_block_len_bits: u32,
    block_pos: usize,
    ms_stereo: bool,

    /// Bytes of a frame that continues into the next packet, and the bit it starts at.
    reservoir: Vec<u8>,
    reservoir_bit: u32,
    /// Samples per channel still to drop for the codec delay.
    skip: usize,
}

impl Decoder {
    /// A decoder for a stream with these parameters. It fails when the format tag is not WMA v2
    /// or the parameters are not supported.
    pub fn new(p: &Params) -> Result<Self> {
        if p.tag != crate::WMAV2 {
            return Err(Error::Unsupported(
                "only WMA v2 (format tag 0x161) is supported",
            ));
        }
        if !(1..=2).contains(&p.channels) {
            return Err(Error::Unsupported("only mono and stereo are supported"));
        }
        if p.sample_rate == 0 || p.sample_rate > 50_000 || p.block_align == 0 {
            return Err(Error::Invalid("bad sample rate or block align"));
        }
        let use_exp_vlc = p.flags & 1 != 0;
        if !use_exp_vlc {
            return Err(Error::Unsupported("LSP-coded exponents"));
        }
        let channels = usize::from(p.channels);
        let sample_rate = p.sample_rate;
        let bit_rate = p.avg_bytes_per_sec as f32 * 8.0;

        let frame_len_bits = if sample_rate <= 16_000 {
            9
        } else if sample_rate <= 22_050 {
            10
        } else {
            11
        };
        let frame_len = 1usize << frame_len_bits;

        let use_variable_block_len = p.flags & 4 != 0;
        let nb_block_sizes = if use_variable_block_len {
            let mut nb = u32::from((p.flags >> 3) & 3) + 1;
            if bit_rate / channels as f32 >= 32_000.0 {
                nb += 2;
            }
            nb.min(frame_len_bits - BLOCK_MIN_BITS) as usize + 1
        } else {
            1
        };

        // Thresholds are defined against a normalised sample rate.
        let rate_class = [44_100, 22_050, 16_000, 11_025, 8_000]
            .into_iter()
            .find(|&r| sample_rate >= r)
            .unwrap_or(sample_rate);
        let bps = bit_rate / (channels as f32 * sample_rate as f32);
        let bps1 = if channels == 2 { bps * 1.6 } else { bps };
        let byte_offset_bits = ((bps * frame_len as f32 / 8.0 + 0.5) as u32).max(1).ilog2() + 2;

        let mut use_noise_coding = true;
        let mut high_freq = sample_rate as f32 * 0.5;
        match rate_class {
            44_100 => {
                if bps1 >= 0.61 {
                    use_noise_coding = false;
                } else {
                    high_freq *= 0.4;
                }
            }
            22_050 => {
                if bps1 >= 1.16 {
                    use_noise_coding = false;
                } else if bps1 >= 0.72 {
                    high_freq *= 0.7;
                } else {
                    high_freq *= 0.6;
                }
            }
            16_000 => high_freq *= if bps > 0.5 { 0.5 } else { 0.3 },
            11_025 => high_freq *= 0.7,
            8_000 => {
                if bps <= 0.625 {
                    high_freq *= 0.5;
                } else if bps > 0.75 {
                    use_noise_coding = false;
                } else {
                    high_freq *= 0.65;
                }
            }
            _ => {
                high_freq *= if bps >= 0.8 {
                    0.75
                } else if bps >= 0.6 {
                    0.6
                } else {
                    0.5
                }
            }
        }

        let layouts = (0..nb_block_sizes)
            .map(|k| layout(sample_rate, frame_len_bits, k as u32, high_freq))
            .collect();

        let noise_mult = 0.02f32; // exponent-VLC streams
        let noise_table = if use_noise_coding {
            let norm = (1.0 / (1u64 << 31) as f32) * 3f32.sqrt() * noise_mult;
            let mut seed = 1u32;
            (0..NOISE_TAB_SIZE)
                .map(|_| {
                    seed = seed.wrapping_mul(314_159).wrapping_add(1);
                    seed as i32 as f32 * norm
                })
                .collect()
        } else {
            Vec::new()
        };

        let class = if sample_rate >= 32_000 && bps1 < 0.72 {
            0
        } else if sample_rate >= 32_000 && bps1 < 1.16 {
            1
        } else {
            2
        };
        let (codes, runlevel) = COEF_TABLES[class];

        let windows = (0..nb_block_sizes)
            .map(|k| {
                let n = frame_len >> k;
                (0..n)
                    .map(|i| {
                        ((i as f64 + 0.5) * std::f64::consts::PI / (2.0 * n as f64)).sin() as f32
                    })
                    .collect()
            })
            .collect();
        let ch = (0..channels)
            .map(|_| Channel {
                exponents: vec![0.0; frame_len],
                levels: vec![0.0; frame_len],
                spectrum: vec![0.0; frame_len],
                high_band_coded: vec![false; 32],
                high_band_values: vec![0; 32],
                frame_out: vec![0.0; frame_len * 2],
                ..Channel::default()
            })
            .collect();

        Ok(Decoder {
            channels,
            sample_rate,
            frame_len_bits,
            frame_len,
            nb_block_sizes,
            use_exp_vlc,
            use_bit_reservoir: p.flags & 2 != 0,
            use_variable_block_len,
            use_noise_coding,
            byte_offset_bits,
            noise_mult,
            noise_table,
            noise_index: 0,
            layouts,
            coef_vlc: [Vlc::new(codes[0]), Vlc::new(codes[1])],
            coef_runlevel: runlevel,
            exp_vlc: Vlc::new(&tables::EXP_CODES),
            hgain_vlc: Vlc::new(&tables::HGAIN_CODES),
            imdct: (0..nb_block_sizes)
                .map(|k| Imdct::new(frame_len >> k))
                .collect(),
            windows,
            imdct_out: vec![0.0; frame_len * 2],
            ch,
            reset_block_lengths: true,
            prev_block_len_bits: frame_len_bits,
            block_len_bits: frame_len_bits,
            next_block_len_bits: frame_len_bits,
            block_pos: 0,
            ms_stereo: false,
            reservoir: Vec::new(),
            reservoir_bit: 0,
            skip: 2 * frame_len,
        })
    }

    /// The number of channels.
    pub fn channels(&self) -> usize {
        self.channels
    }

    /// The sample rate, in samples per second.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Codec delay in samples per channel: the first two frames only prime the overlap and are
    /// not output.
    pub fn delay(&self) -> usize {
        2 * self.frame_len
    }

    /// Samples per channel produced by each frame.
    pub fn frame_len(&self) -> usize {
        self.frame_len
    }

    /// Decode one packet (`block_align` bytes), appending interleaved samples in `[-1, 1)` to
    /// `out`. On error nothing is appended and the bit reservoir is dropped, so the next packet
    /// starts cleanly.
    pub fn decode_packet(&mut self, packet: &[u8], out: &mut Vec<f32>) -> Result<()> {
        let (start, skip) = (out.len(), self.skip);
        let r = self.superframe(packet, out);
        if r.is_err() {
            out.truncate(start);
            self.skip = skip;
            self.reservoir.clear();
        }
        r
    }

    /// Emit the final overlap tail (one frame) after the last packet and reset.
    pub fn flush(&mut self, out: &mut Vec<f32>) {
        self.emit_frame(out);
        self.reset();
    }

    /// Forget all stream state, e.g. before seeking.
    pub fn reset(&mut self) {
        for c in &mut self.ch {
            c.frame_out.fill(0.0);
            c.exp_ready = false;
        }
        self.reservoir.clear();
        self.reset_block_lengths = true;
        self.noise_index = 0;
        self.skip = self.delay();
    }

    fn superframe(&mut self, packet: &[u8], out: &mut Vec<f32>) -> Result<()> {
        let mut br = BitReader::new(packet);
        if !self.use_bit_reservoir {
            self.reset_block_lengths = true;
            return self.frame(&mut br, out);
        }
        br.skip(4); // superframe index
                    // Counts frames ending in this packet, including one continued from the previous.
        let mut nb_frames = br.bits(4) as i32 - i32::from(self.reservoir.is_empty());
        if nb_frames < 0 || (nb_frames == 0 && packet.len() <= 2) {
            return Err(Error::Invalid("superframe without a frame end"));
        }
        if nb_frames == 0 {
            // No frame ends here: the whole packet continues the reservoir frame.
            if self.reservoir.len() + packet.len() > MAX_RESERVOIR_BYTES {
                return Err(Error::Invalid("bit reservoir overflow"));
            }
            self.reservoir.extend_from_slice(&packet[1..]);
            return Ok(());
        }
        let bit_offset = br.bits(self.byte_offset_bits + 3) as usize;
        let header_bits = br.pos();

        if !self.reservoir.is_empty() {
            let mut buf = std::mem::take(&mut self.reservoir);
            if buf.len() + bit_offset.div_ceil(8) > MAX_RESERVOIR_BYTES {
                return Err(Error::Invalid("bit reservoir overflow"));
            }
            let mut left = bit_offset as u32;
            while left >= 8 {
                buf.push(br.bits(8) as u8);
                left -= 8;
            }
            if left > 0 {
                buf.push((br.bits(left) << (8 - left)) as u8);
            }
            let mut fr = BitReader::new(&buf);
            fr.skip(self.reservoir_bit);
            self.frame(&mut fr, out)?;
            nb_frames -= 1;
        }

        let pos = header_bits + bit_offset;
        if pos > br.len_bits() {
            return Err(Error::Invalid("frame offset past end of packet"));
        }
        br.seek(pos);
        self.reset_block_lengths = true;
        for _ in 0..nb_frames {
            self.frame(&mut br, out)?;
        }
        let tail = br.pos();
        if tail > br.len_bits() || packet.len() - tail / 8 > MAX_RESERVOIR_BYTES {
            return Err(Error::Invalid("frame overran packet"));
        }
        self.reservoir = packet[tail / 8..].to_vec();
        self.reservoir_bit = (tail % 8) as u32;
        Ok(())
    }

    fn frame(&mut self, br: &mut BitReader, out: &mut Vec<f32>) -> Result<()> {
        self.block_pos = 0;
        while !self.block(br)? {}
        self.emit_frame(out);
        let f = self.frame_len;
        for c in &mut self.ch {
            c.frame_out.copy_within(f..2 * f, 0);
        }
        Ok(())
    }

    fn emit_frame(&mut self, out: &mut Vec<f32>) {
        const SCALE: f32 = 1.0 / 32768.0;
        let first = self.skip.min(self.frame_len);
        self.skip -= first;
        out.reserve((self.frame_len - first) * self.channels);
        for i in first..self.frame_len {
            for c in &self.ch {
                out.push(c.frame_out[i] * SCALE);
            }
        }
    }

    /// Decode one block; true when it completes the frame.
    fn block(&mut self, br: &mut BitReader) -> Result<bool> {
        let fb = self.frame_len_bits;
        if self.use_variable_block_len {
            let n = (self.nb_block_sizes as u32 - 1).ilog2() + 1;
            let size_bits = |br: &mut BitReader| {
                let v = br.bits(n) as usize;
                if v >= self.nb_block_sizes {
                    return Err(Error::Invalid("block size index out of range"));
                }
                Ok(fb - v as u32)
            };
            if self.reset_block_lengths {
                self.reset_block_lengths = false;
                self.prev_block_len_bits = size_bits(br)?;
                self.block_len_bits = size_bits(br)?;
            } else {
                self.prev_block_len_bits = self.block_len_bits;
                self.block_len_bits = self.next_block_len_bits;
            }
            self.next_block_len_bits = size_bits(br)?;
        } else {
            self.prev_block_len_bits = fb;
            self.block_len_bits = fb;
            self.next_block_len_bits = fb;
        }
        let bsize = (fb - self.block_len_bits) as usize;
        let block_len = 1usize << self.block_len_bits;
        if self.block_pos + block_len > self.frame_len {
            return Err(Error::Invalid("block runs past frame end"));
        }

        if self.channels == 2 {
            self.ms_stereo = br.bit();
        }
        let mut any_coded = false;
        for c in &mut self.ch {
            c.coded = br.bit();
            any_coded |= c.coded;
        }
        if any_coded {
            self.decode_spectrum(br, bsize, block_len)?;
        }

        let half = block_len / 2;
        let index = self.frame_len / 2 + self.block_pos - half;
        for ci in 0..self.channels {
            if self.ch[ci].coded {
                self.imdct[bsize].run(&self.ch[ci].spectrum[..block_len], &mut self.imdct_out);
            } else if !(self.ms_stereo && ci == 1) {
                // An uncoded side channel of an M/S pair reuses the mid output: L = R = M.
                self.imdct_out.fill(0.0);
            }
            self.window(ci, index);
        }

        self.block_pos += block_len;
        Ok(self.block_pos >= self.frame_len)
    }

    fn decode_spectrum(
        &mut self,
        br: &mut BitReader,
        bsize: usize,
        block_len: usize,
    ) -> Result<()> {
        let mut total_gain = 1i32;
        loop {
            if br.pos() + 7 > br.len_bits() {
                return Err(Error::Invalid("truncated gain"));
            }
            let a = br.bits(7) as i32;
            total_gain += a;
            if a != 127 {
                break;
            }
        }
        let coef_nb_bits = match total_gain {
            ..15 => 13,
            15..32 => 12,
            32..40 => 11,
            40..45 => 10,
            _ => 9,
        };

        let layout = &self.layouts[bsize];
        let mut nb_coefs = [layout.coefs_end; 2];

        if self.use_noise_coding {
            let n = layout.high_bands.len();
            for (ci, c) in self.ch.iter_mut().enumerate() {
                if c.coded {
                    for i in 0..n {
                        c.high_band_coded[i] = br.bit();
                        if c.high_band_coded[i] {
                            nb_coefs[ci] -= layout.high_bands[i];
                        }
                    }
                }
            }
            for c in self.ch.iter_mut().filter(|c| c.coded) {
                let mut val: Option<i32> = None;
                for i in 0..n {
                    if c.high_band_coded[i] {
                        let v = match val {
                            None => br.bits(7) as i32 - 19,
                            Some(v) => v + i32::from(self.hgain_vlc.read(br).unwrap_or(0)) - 18,
                        };
                        val = Some(v);
                        c.high_band_values[i] = v;
                    }
                }
            }
        }

        // Short blocks may reuse the previous block's exponents.
        if self.block_len_bits == self.frame_len_bits || br.bit() {
            for ci in 0..self.channels {
                if self.ch[ci].coded {
                    self.decode_exponents(br, ci, bsize, block_len)?;
                }
            }
        }
        if self.ch.iter().any(|c| c.coded && !c.exp_ready) {
            return Err(Error::Invalid("exponents used before being sent"));
        }

        for ci in 0..self.channels {
            if !self.ch[ci].coded {
                continue;
            }
            let t = usize::from(ci == 1 && self.ms_stereo);
            let levels = &mut self.ch[ci].levels[..block_len];
            levels.fill(0.0);
            run_level(
                br,
                &self.coef_vlc[t],
                self.coef_runlevel[t],
                levels,
                nb_coefs[ci],
                self.frame_len_bits,
                coef_nb_bits,
            );
        }

        let mdct_norm = 1.0 / (block_len / 2) as f32;
        let gain = 10f32.powf(total_gain as f32 * 0.05);
        for ci in 0..self.channels {
            if self.ch[ci].coded {
                let mult = gain / self.ch[ci].max_exponent * mdct_norm;
                if self.use_noise_coding {
                    self.dequantise_with_noise(ci, bsize, block_len, mult, mdct_norm);
                } else {
                    let c = &mut self.ch[ci];
                    let esize = c.exp_bsize;
                    let n = nb_coefs[ci];
                    for i in 0..n {
                        c.spectrum[i] = c.levels[i] * c.exponents[(i << bsize) >> esize] * mult;
                    }
                    c.spectrum[n..block_len].fill(0.0);
                }
            }
        }

        if self.ms_stereo && self.ch[1].coded {
            if !self.ch[0].coded {
                self.ch[0].spectrum[..block_len].fill(0.0);
                self.ch[0].coded = true;
            }
            let (a, b) = self.ch.split_at_mut(1);
            for (m, s) in a[0].spectrum[..block_len]
                .iter_mut()
                .zip(&mut b[0].spectrum[..block_len])
            {
                let (l, r) = (*m + *s, *m - *s);
                *m = l;
                *s = r;
            }
        }
        Ok(())
    }

    fn decode_exponents(
        &mut self,
        br: &mut BitReader,
        ci: usize,
        bsize: usize,
        block_len: usize,
    ) -> Result<()> {
        debug_assert!(self.use_exp_vlc);
        let c = &mut self.ch[ci];
        let mut last = 36i32;
        let mut max = 0f32;
        let mut q = 0;
        for &width in &self.layouts[bsize].exponent_bands {
            if q >= block_len {
                break;
            }
            let code = self
                .exp_vlc
                .read(br)
                .ok_or(Error::Invalid("bad exponent code"))?;
            last += i32::from(code) - 60;
            if !(-60..=195).contains(&last) {
                return Err(Error::Invalid("exponent out of range"));
            }
            let v = 10f64.powf(f64::from(last) / 16.0) as f32;
            max = max.max(v);
            c.exponents[q..q + width].fill(v);
            q += width;
        }
        c.max_exponent = max;
        c.exp_bsize = bsize;
        c.exp_ready = true;
        Ok(())
    }

    /// Dequantise with noise substitution for uncoded high bands and dither elsewhere.
    fn dequantise_with_noise(
        &mut self,
        ci: usize,
        bsize: usize,
        block_len: usize,
        mult: f32,
        mdct_norm: f32,
    ) {
        let layout = &self.layouts[bsize];
        let c = &mut self.ch[ci];
        let esize = c.exp_bsize;
        let exp_at = |pos: usize| (pos << bsize) >> esize;
        let mut noise = || {
            let v = self.noise_table[self.noise_index];
            self.noise_index = (self.noise_index + 1) & (NOISE_TAB_SIZE - 1);
            v
        };

        // Mean exponent power of each noise-coded band.
        let n_high = layout.high_bands.len();
        let mut power = [0f32; 32];
        let mut last_high = 0;
        let mut e = exp_at(layout.high_band_start);
        for j in 0..n_high {
            let n = layout.high_bands[j];
            if c.high_band_coded[j] {
                let sum: f32 = (0..n).map(|i| c.exponents[e + exp_at(i)].powi(2)).sum();
                power[j] = sum / n as f32;
                last_high = j;
            }
            e += exp_at(n);
        }

        let mut out = 0;
        let mut levels = 0;
        let mut e = 0;
        for j in -1..n_high as i32 {
            let n = if j < 0 {
                layout.high_band_start
            } else {
                layout.high_bands[j as usize]
            };
            if j >= 0 && c.high_band_coded[j as usize] {
                let j = j as usize;
                let m = (power[j] / power[last_high]).sqrt()
                    * 10f32.powf(c.high_band_values[j] as f32 * 0.05)
                    / (c.max_exponent * self.noise_mult)
                    * mdct_norm;
                for i in 0..n {
                    c.spectrum[out] = noise() * c.exponents[e + exp_at(i)] * m;
                    out += 1;
                }
            } else {
                for i in 0..n {
                    c.spectrum[out] =
                        (c.levels[levels] + noise()) * c.exponents[e + exp_at(i)] * mult;
                    out += 1;
                    levels += 1;
                }
            }
            e += exp_at(n);
        }
        let m = mult * c.exponents[e - exp_at(1).max(1)];
        while out < block_len {
            c.spectrum[out] = noise() * m;
            out += 1;
        }
    }

    /// Window the IMDCT output into the channel's overlap buffer at `index`.
    fn window(&mut self, ci: usize, index: usize) {
        let fb = self.frame_len_bits;
        let block_len = 1usize << self.block_len_bits;
        let input = &self.imdct_out;
        let out = &mut self.ch[ci].frame_out[index..index + 2 * block_len];

        if self.block_len_bits <= self.prev_block_len_bits {
            let w = &self.windows[(fb - self.block_len_bits) as usize];
            for i in 0..block_len {
                out[i] += input[i] * w[i];
            }
        } else {
            let len = 1usize << self.prev_block_len_bits;
            let n = (block_len - len) / 2;
            let w = &self.windows[(fb - self.prev_block_len_bits) as usize];
            for i in 0..len {
                out[n + i] += input[n + i] * w[i];
            }
            out[n + len..block_len].copy_from_slice(&input[n + len..block_len]);
        }

        let (out, input) = (&mut out[block_len..], &input[block_len..]);
        if self.block_len_bits <= self.next_block_len_bits {
            let w = &self.windows[(fb - self.block_len_bits) as usize];
            for i in 0..block_len {
                out[i] = input[i] * w[block_len - 1 - i];
            }
        } else {
            let len = 1usize << self.next_block_len_bits;
            let n = (block_len - len) / 2;
            let w = &self.windows[(fb - self.next_block_len_bits) as usize];
            out[..n].copy_from_slice(&input[..n]);
            for i in 0..len {
                out[n + i] = input[n + i] * w[len - 1 - i];
            }
            out[n + len..block_len].fill(0.0);
        }
    }
}

type CoefTables = ([&'static [(u8, u32)]; 2], [&'static [(u16, u16)]; 2]);
const COEF_TABLES: [CoefTables; 3] = [
    (
        [&tables::COEF0_CODES, &tables::COEF1_CODES],
        [&tables::COEF0_RUNLEVEL, &tables::COEF1_RUNLEVEL],
    ),
    (
        [&tables::COEF2_CODES, &tables::COEF3_CODES],
        [&tables::COEF2_RUNLEVEL, &tables::COEF3_RUNLEVEL],
    ),
    (
        [&tables::COEF4_CODES, &tables::COEF5_CODES],
        [&tables::COEF4_RUNLEVEL, &tables::COEF5_RUNLEVEL],
    ),
];

/// Run/level decode of `num` coefficients into `levels` (one block, pre-zeroed).
fn run_level(
    br: &mut BitReader,
    vlc: &Vlc,
    runlevel: &[(u16, u16)],
    levels: &mut [f32],
    num: usize,
    frame_len_bits: u32,
    coef_nb_bits: u32,
) {
    let mask = levels.len() - 1;
    let mut offset = 0usize;
    while offset < num {
        match vlc.read(br) {
            Some(1) => break, // end of block
            Some(code @ 2..) => {
                let (run, level) = runlevel[usize::from(code) - 2];
                offset += usize::from(run);
                let level = f32::from(level);
                levels[offset & mask] = if br.bit() { level } else { -level };
            }
            _ => {
                // Escape: explicit level and run.
                let level = br.bits(coef_nb_bits) as f32;
                offset += br.bits(frame_len_bits) as usize;
                levels[offset & mask] = if br.bit() { level } else { -level };
            }
        }
        offset += 1;
    }
}

fn layout(sample_rate: u32, frame_len_bits: u32, k: u32, high_freq: f32) -> Layout {
    let frame_len = 1usize << frame_len_bits;
    let block_len = frame_len >> k;

    let arm = match sample_rate {
        44_100.. => Some(0),
        32_000.. => Some(1),
        22_050.. => Some(2),
        _ => None,
    };
    let hard = (frame_len_bits - BLOCK_MIN_BITS)
        .checked_sub(k)
        .filter(|&a| a < 3)
        .zip(arm)
        .and_then(|(a, arm)| tables::EXPONENT_BAND_EDGES[arm][a as usize]);
    let exponent_bands: Vec<usize> = match hard {
        Some(edges) => edges.windows(2).map(|w| usize::from(w[1] - w[0])).collect(),
        None => {
            // Critical-band edges rounded to multiples of four coefficients.
            let mut bands = Vec::new();
            let mut last = 0;
            for &f in &tables::CRITICAL_FREQS {
                let b = u64::from(sample_rate);
                let pos = (((block_len as u64 * 2 * u64::from(f)) + (b << 1)) / (4 * b)) << 2;
                let pos = (pos as usize).min(block_len);
                if pos > last {
                    bands.push(pos - last);
                }
                if pos >= block_len {
                    break;
                }
                last = pos;
            }
            bands
        }
    };

    let coefs_end = (frame_len - frame_len * 9 / 100) >> k;
    let high_band_start =
        ((block_len as f32 * 2.0 * high_freq) / sample_rate as f32 + 0.5) as usize;
    let mut high_bands = Vec::new();
    let mut pos = 0;
    for &w in &exponent_bands {
        let start = pos.max(high_band_start);
        pos += w;
        let end = pos.min(coefs_end);
        if end > start {
            high_bands.push(end - start);
        }
    }
    Layout {
        exponent_bands,
        coefs_end,
        high_band_start,
        high_bands,
    }
}
