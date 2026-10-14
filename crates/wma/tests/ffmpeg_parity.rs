//! Decode every shipped SoundBuffer and compare against FFmpeg's wmav2 decoder.
//! Skipped when `game-data/original/WinData` or `ffmpeg` is missing.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use banished_formats::crc::type_id;
use banished_formats::crs;
use banished_formats::pkg::{Archive, Package};
use banished_formats::types::{self, SoundBuffer};
use banished_wma::{Decoder, Params};

const MIN_SNR_DB: f64 = 60.0;

fn windata() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../game-data/original/WinData")
}

/// FFmpeg's float output, so its platform-specific s16 conversion does not enter the comparison.
fn ffmpeg_decode(xwma: &Path) -> Vec<f32> {
    let out = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-i"])
        .arg(xwma)
        .args(["-f", "f32le", "-acodec", "pcm_f32le", "-"])
        .stderr(Stdio::inherit())
        .output()
        .expect("run ffmpeg");
    assert!(out.status.success(), "ffmpeg failed on {}", xwma.display());
    out.stdout
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
        .collect()
}

fn snr_db(reference: &[f32], ours: &[f32]) -> f64 {
    let (mut signal, mut noise) = (0f64, 0f64);
    for (&r, &o) in reference.iter().zip(ours) {
        signal += f64::from(r).powi(2);
        noise += (f64::from(r) - f64::from(o)).powi(2);
    }
    if noise == 0.0 {
        f64::INFINITY
    } else {
        10.0 * (signal / noise).log10()
    }
}

#[test]
fn matches_ffmpeg() {
    let dir = windata();
    if !dir.is_dir() {
        eprintln!("skipping: {} not found", dir.display());
        return;
    }
    if Command::new("ffmpeg")
        .arg("-version")
        .stdout(Stdio::null())
        .status()
        .is_err()
    {
        eprintln!("skipping: ffmpeg not on PATH");
        return;
    }

    let mut archive = Archive::default();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("pkg"))
        {
            archive.add(Package::parse(std::fs::read(&path).unwrap()).unwrap());
        }
    }
    let tmp = std::env::temp_dir().join(format!("banished-wma-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();

    let sound_id = type_id(SoundBuffer::CLASS);
    let (mut count, mut failures) = (0, Vec::new());
    for name in archive.names() {
        if !name.ends_with(".crs") {
            continue;
        }
        let data = archive.read(name).unwrap();
        if crs::header(&data).map(|h| h.type_id) != Ok(sound_id) {
            continue;
        }
        let snd = types::sound_buffer(&data).unwrap();
        let stream = snd
            .streamed
            .then(|| archive.read(&snd.stream_entry).unwrap());
        let packets = stream.as_deref().unwrap_or(&snd.data);
        let f = snd.format;

        let params = Params {
            tag: f.tag,
            ..Params::xwma(
                f.channels,
                f.sample_rate,
                f.avg_bytes_per_sec,
                f.block_align,
            )
        };
        let mut dec = Decoder::new(&params).unwrap_or_else(|e| panic!("{name}: {e}"));
        let mut pcm = Vec::new();
        let mut bad_packets = 0;
        let n_packets = packets.len().div_ceil(usize::from(f.block_align));
        for (i, packet) in packets.chunks(usize::from(f.block_align)).enumerate() {
            if let Err(e) = dec.decode_packet(packet, &mut pcm) {
                println!(
                    "     {name}: packet {i}/{n_packets} ({} bytes): {e}",
                    packet.len()
                );
                bad_packets += 1;
            }
        }
        dec.flush(&mut pcm);
        let ours = pcm;

        let xwma = tmp.join("sound.xwma");
        std::fs::write(&xwma, snd.to_xwma(stream.as_deref())).unwrap();
        let reference = ffmpeg_decode(&xwma);

        let snr = snr_db(&reference, &ours);
        let ok = reference.len() == ours.len() && snr > MIN_SNR_DB && bad_packets == 0;
        println!(
            "{} {name}: {}ch {}Hz align {} | samples ours {} ffmpeg {} | bad packets {bad_packets} | SNR {snr:.1} dB",
            if ok { "ok  " } else { "FAIL" },
            f.channels,
            f.sample_rate,
            f.block_align,
            ours.len(),
            reference.len(),
        );
        count += 1;
        if !ok {
            failures.push(name.to_string());
        }
    }
    std::fs::remove_dir_all(&tmp).ok();
    println!("{count} sounds, {} failed", failures.len());
    assert!(count > 0, "no SoundBuffers found");
    assert!(failures.is_empty(), "mismatches: {failures:#?}");
}
