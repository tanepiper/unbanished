//! Compare two screenshots (for example an original-game capture against a `--boot-capture`).
//!
//! ```text
//! cargo run -q --release -p banished-app --example imgdiff -- A.png B.png
//!     [--out diff.png] [--threshold 16] [--resize] [--ignore x,y,w,h]... [--json]
//!     [--crop-a x,y,w,h | --autocrop-a] [--crop-b x,y,w,h] [--blur R] [--compose panels.png]
//!     [--print-crop]
//! ```
//!
//! Reports mean absolute error, PSNR, the share of pixels whose largest channel difference
//! exceeds the threshold, their bounding box, and a 4 x 4 grid of mismatch shares so a change
//! can be located. `--resize` scales B to A's size (nearest neighbour) when captures differ in
//! resolution; `--ignore` masks regions such as the clock or a cursor. `--out` writes a heat
//! map: A in grey with mismatching pixels in red (brighter = larger difference).
//! `--crop-a`/`--crop-b` compare a region of each image (e.g. the same dialog in windows of
//! different sizes); `--blur R` box-blurs both first, which tolerates the video compression of
//! a Steam Remote Play stream of the original game. `--compose` writes A, B and a 50/50 blend
//! side by side (before blurring), which shows offsets at a glance. `--autocrop-a` removes black
//! letterbox/pillarbox bars around A (a stream of the original at another aspect ratio);
//! `--print-crop` prints A's content rectangle as `x,y,w,h` and exits.

use std::fs::File;
use std::io::BufWriter;

struct Image {
    width: usize,
    height: usize,
    rgba: Vec<u8>,
}

fn load(path: &str) -> Image {
    let file = File::open(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let decoder = png::Decoder::new(std::io::BufReader::new(file));
    let mut reader = decoder
        .read_info()
        .unwrap_or_else(|e| panic!("{path}: {e}"));
    let mut buf = vec![0; reader.output_buffer_size().expect("image too large")];
    let info = reader.next_frame(&mut buf).unwrap();
    let (w, h) = (info.width as usize, info.height as usize);
    let channels = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Grayscale => 1,
        other => panic!("{path}: unsupported colour type {other:?}"),
    };
    assert_eq!(
        info.bit_depth,
        png::BitDepth::Eight,
        "{path}: only 8-bit PNGs"
    );
    let mut rgba = Vec::with_capacity(w * h * 4);
    for px in buf[..w * h * channels].chunks_exact(channels) {
        match channels {
            4 => rgba.extend_from_slice(px),
            3 => rgba.extend_from_slice(&[px[0], px[1], px[2], 255]),
            2 => rgba.extend_from_slice(&[px[0], px[0], px[0], px[1]]),
            _ => rgba.extend_from_slice(&[px[0], px[0], px[0], 255]),
        }
    }
    Image {
        width: w,
        height: h,
        rgba,
    }
}

fn resize(img: &Image, w: usize, h: usize) -> Image {
    let mut rgba = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        let sy = y * img.height / h;
        for x in 0..w {
            let sx = x * img.width / w;
            let i = (sy * img.width + sx) * 4;
            rgba.extend_from_slice(&img.rgba[i..i + 4]);
        }
    }
    Image {
        width: w,
        height: h,
        rgba,
    }
}

fn crop(img: &Image, r: [usize; 4]) -> Image {
    let [x0, y0, w, h] = r;
    assert!(
        x0 + w <= img.width && y0 + h <= img.height,
        "crop outside image"
    );
    let mut rgba = Vec::with_capacity(w * h * 4);
    for y in y0..y0 + h {
        let i = (y * img.width + x0) * 4;
        rgba.extend_from_slice(&img.rgba[i..i + w * 4]);
    }
    Image {
        width: w,
        height: h,
        rgba,
    }
}

/// Separable box blur of radius `r` (edges clamped).
fn blur(img: &Image, r: usize) -> Image {
    if r == 0 {
        return Image {
            width: img.width,
            height: img.height,
            rgba: img.rgba.clone(),
        };
    }
    let (w, h) = (img.width, img.height);
    let pass = |src: &[u8], horizontal: bool| -> Vec<u8> {
        let mut out = vec![0u8; src.len()];
        for y in 0..h {
            for x in 0..w {
                let mut acc = [0u32; 4];
                let mut n = 0;
                for d in -(r as isize)..=(r as isize) {
                    let (sx, sy) = if horizontal {
                        ((x as isize + d).clamp(0, w as isize - 1) as usize, y)
                    } else {
                        (x, (y as isize + d).clamp(0, h as isize - 1) as usize)
                    };
                    let i = (sy * w + sx) * 4;
                    for c in 0..4 {
                        acc[c] += src[i + c] as u32;
                    }
                    n += 1;
                }
                let o = (y * w + x) * 4;
                for c in 0..4 {
                    out[o + c] = (acc[c] / n) as u8;
                }
            }
        }
        out
    };
    let rgba = pass(&pass(&img.rgba, true), false);
    Image {
        width: w,
        height: h,
        rgba,
    }
}

/// The rectangle inside black bars: rows/columns whose mean brightness stays under 8.
fn content_rect(img: &Image) -> [usize; 4] {
    let dark_row = |y: usize| {
        let row = &img.rgba[y * img.width * 4..(y + 1) * img.width * 4];
        let sum: u64 = row
            .chunks_exact(4)
            .map(|p| p[0] as u64 + p[1] as u64 + p[2] as u64)
            .sum();
        sum / (img.width as u64 * 3) < 8
    };
    let dark_col = |x: usize, y0: usize, y1: usize| {
        let sum: u64 = (y0..y1)
            .map(|y| {
                let i = (y * img.width + x) * 4;
                img.rgba[i] as u64 + img.rgba[i + 1] as u64 + img.rgba[i + 2] as u64
            })
            .sum();
        sum / ((y1 - y0).max(1) as u64 * 3) < 8
    };
    let mut y0 = 0;
    while y0 < img.height / 2 && dark_row(y0) {
        y0 += 1;
    }
    let mut y1 = img.height;
    while y1 > img.height / 2 && dark_row(y1 - 1) {
        y1 -= 1;
    }
    let mut x0 = 0;
    while x0 < img.width / 2 && dark_col(x0, y0, y1) {
        x0 += 1;
    }
    let mut x1 = img.width;
    while x1 > img.width / 2 && dark_col(x1 - 1, y0, y1) {
        x1 -= 1;
    }
    [x0, y0, x1 - x0, y1 - y0]
}

fn rect(s: &str) -> [usize; 4] {
    let v: Vec<usize> = s.split(',').map(|s| s.parse().unwrap()).collect();
    [v[0], v[1], v[2], v[3]]
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut paths = Vec::new();
    let (mut out, mut threshold, mut do_resize, mut json) = (None, 16u8, false, false);
    let mut ignore: Vec<[usize; 4]> = Vec::new();
    let (mut crop_a, mut crop_b, mut blur_r) = (None, None, 0usize);
    let mut compose: Option<String> = None;
    let (mut autocrop, mut print_crop) = (false, false);
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--out" => {
                i += 1;
                out = Some(args[i].clone());
            }
            "--threshold" => {
                i += 1;
                threshold = args[i].parse().expect("threshold 0-255");
            }
            "--resize" => do_resize = true,
            "--crop-a" => {
                i += 1;
                crop_a = Some(rect(&args[i]));
            }
            "--crop-b" => {
                i += 1;
                crop_b = Some(rect(&args[i]));
            }
            "--autocrop-a" => autocrop = true,
            "--print-crop" => print_crop = true,
            "--compose" => {
                i += 1;
                compose = Some(args[i].clone());
            }
            "--blur" => {
                i += 1;
                blur_r = args[i].parse().expect("blur radius");
            }
            "--json" => json = true,
            "--ignore" => {
                i += 1;
                let v: Vec<usize> = args[i].split(',').map(|s| s.parse().unwrap()).collect();
                ignore.push([v[0], v[1], v[2], v[3]]);
            }
            p => paths.push(p.to_string()),
        }
        i += 1;
    }
    if print_crop {
        let r = content_rect(&load(&paths[0]));
        println!("{},{},{},{}", r[0], r[1], r[2], r[3]);
        return;
    }
    assert!(paths.len() == 2, "usage: imgdiff A.png B.png [options]");
    let mut a = load(&paths[0]);
    if autocrop {
        let r = content_rect(&a);
        a = crop(&a, r);
    }
    let mut b = load(&paths[1]);
    if let Some(r) = crop_a {
        a = crop(&a, r);
    }
    if let Some(r) = crop_b {
        b = crop(&b, r);
    }
    if (a.width, a.height) != (b.width, b.height) {
        assert!(
            do_resize,
            "sizes differ ({}x{} vs {}x{}); pass --resize",
            a.width, a.height, b.width, b.height
        );
        b = resize(&b, a.width, a.height);
    }
    if let Some(path) = &compose {
        let (w, h) = (a.width, a.height);
        let mut out = vec![0u8; w * 3 * h * 4];
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) * 4;
                for (panel, px) in [
                    (0, [a.rgba[i], a.rgba[i + 1], a.rgba[i + 2]]),
                    (1, [b.rgba[i], b.rgba[i + 1], b.rgba[i + 2]]),
                    (
                        2,
                        std::array::from_fn(|c| {
                            ((a.rgba[i + c] as u16 + b.rgba[i + c] as u16) / 2) as u8
                        }),
                    ),
                ] {
                    let o = (y * w * 3 + panel * w + x) * 4;
                    out[o..o + 4].copy_from_slice(&[px[0], px[1], px[2], 255]);
                }
            }
        }
        let file = BufWriter::new(File::create(path).unwrap());
        let mut encoder = png::Encoder::new(file, (w * 3) as u32, h as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&out)
            .unwrap();
    }
    if blur_r > 0 {
        a = blur(&a, blur_r);
        b = blur(&b, blur_r);
    }
    let (w, h) = (a.width, a.height);
    let masked = |x: usize, y: usize| {
        ignore
            .iter()
            .any(|r| x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3])
    };
    let (mut sum_abs, mut sum_sq, mut count, mut bad) = (0f64, 0f64, 0usize, 0usize);
    let mut max_delta = 0u8;
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
    let mut grid = [[(0usize, 0usize); 4]; 4];
    let mut heat = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            let grey = ((a.rgba[i] as u32 + a.rgba[i + 1] as u32 + a.rgba[i + 2] as u32) / 6) as u8;
            heat[i..i + 4].copy_from_slice(&[grey, grey, grey, 255]);
            if masked(x, y) {
                heat[i + 2] = grey.saturating_add(60);
                continue;
            }
            let mut d = 0u8;
            for c in 0..3 {
                let e = a.rgba[i + c].abs_diff(b.rgba[i + c]);
                sum_abs += e as f64;
                sum_sq += (e as f64) * (e as f64);
                d = d.max(e);
            }
            count += 1;
            max_delta = max_delta.max(d);
            let cell = &mut grid[y * 4 / h][x * 4 / w];
            cell.1 += 1;
            if d > threshold {
                bad += 1;
                cell.0 += 1;
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
                heat[i..i + 4].copy_from_slice(&[128u8.saturating_add(d / 2), 0, 0, 255]);
            }
        }
    }
    let samples = (count * 3).max(1) as f64;
    let mae = sum_abs / samples;
    let mse = sum_sq / samples;
    let psnr = if mse == 0.0 {
        f64::INFINITY
    } else {
        10.0 * (255.0f64 * 255.0 / mse).log10()
    };
    let share = bad as f64 * 100.0 / count.max(1) as f64;
    let bbox = (bad > 0).then(|| [x0, y0, x1 - x0 + 1, y1 - y0 + 1]);
    let grid_pct: Vec<Vec<f64>> = grid
        .iter()
        .map(|row| {
            row.iter()
                .map(|&(b, n)| (b as f64 * 1000.0 / n.max(1) as f64).round() / 10.0)
                .collect()
        })
        .collect();
    if json {
        println!(
            "{{\"width\":{w},\"height\":{h},\"mae\":{mae:.3},\"psnr\":{},\"mismatch_pct\":{share:.3},\"max_delta\":{max_delta},\"bbox\":{},\"grid_pct\":{:?}}}",
            if psnr.is_finite() { format!("{psnr:.2}") } else { "null".into() },
            bbox.map_or("null".into(), |b| format!("{b:?}")),
            grid_pct
        );
    } else {
        println!("{w}x{h}  MAE {mae:.3}  PSNR {psnr:.2} dB  max delta {max_delta}");
        println!("{share:.3}% of pixels differ by more than {threshold}");
        if let Some(b) = bbox {
            println!(
                "differences inside x {} y {} w {} h {}",
                b[0], b[1], b[2], b[3]
            );
        }
        println!("mismatch % by quarter (rows top to bottom):");
        for row in &grid_pct {
            println!(
                "  {}",
                row.iter()
                    .map(|v| format!("{v:6.1}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            );
        }
    }
    if let Some(path) = out {
        let file = BufWriter::new(File::create(&path).unwrap());
        let mut encoder = png::Encoder::new(file, w as u32, h as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&heat)
            .unwrap();
    }
}
