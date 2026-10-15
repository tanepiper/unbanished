//! Compare two screenshots (for example an original-game capture against a `--boot-capture`).
//!
//! ```text
//! cargo run -q --release -p banished-app --example imgdiff -- A.png B.png
//!     [--out diff.png] [--threshold 16] [--resize] [--ignore x,y,w,h]... [--json]
//! ```
//!
//! Reports mean absolute error, PSNR, the share of pixels whose largest channel difference
//! exceeds the threshold, their bounding box, and a 4 x 4 grid of mismatch shares so a change
//! can be located. `--resize` scales B to A's size (nearest neighbour) when captures differ in
//! resolution; `--ignore` masks regions such as the clock or a cursor. `--out` writes a heat
//! map: A in grey with mismatching pixels in red (brighter = larger difference).

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
    let mut reader = decoder.read_info().unwrap_or_else(|e| panic!("{path}: {e}"));
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
    assert_eq!(info.bit_depth, png::BitDepth::Eight, "{path}: only 8-bit PNGs");
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

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut paths = Vec::new();
    let (mut out, mut threshold, mut do_resize, mut json) = (None, 16u8, false, false);
    let mut ignore: Vec<[usize; 4]> = Vec::new();
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
    assert!(paths.len() == 2, "usage: imgdiff A.png B.png [options]");
    let a = load(&paths[0]);
    let mut b = load(&paths[1]);
    if (a.width, a.height) != (b.width, b.height) {
        assert!(
            do_resize,
            "sizes differ ({}x{} vs {}x{}); pass --resize",
            a.width, a.height, b.width, b.height
        );
        b = resize(&b, a.width, a.height);
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
            println!("differences inside x {} y {} w {} h {}", b[0], b[1], b[2], b[3]);
        }
        println!("mismatch % by quarter (rows top to bottom):");
        for row in &grid_pct {
            println!("  {}", row.iter().map(|v| format!("{v:6.1}")).collect::<Vec<_>>().join(" "));
        }
    }
    if let Some(path) = out {
        let file = BufWriter::new(File::create(&path).unwrap());
        let mut encoder = png::Encoder::new(file, w as u32, h as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().unwrap().write_image_data(&heat).unwrap();
    }
}
