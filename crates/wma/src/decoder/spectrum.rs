//! A block's spectrum: the exponents, the coefficients by run and level, the noise fill and the window.

use super::*;

pub(super) type CoefTables = ([&'static [(u8, u32)]; 2], [&'static [(u16, u16)]; 2]);

pub(super) const COEF_TABLES: [CoefTables; 3] = [
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
pub(super) fn run_level(
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

impl Decoder {
    pub(super) fn decode_spectrum(
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

    pub(super) fn decode_exponents(
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
    pub(super) fn dequantise_with_noise(
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
    pub(super) fn window(&mut self, ci: usize, index: usize) {
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
