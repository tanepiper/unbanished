//! Developer tools for Banished game data. Outputs go to git-ignored folders under `game-data/`.
//!
//! ```text
//! unbanished-cli export-audio [WINDATA] [OUT]   decode every SoundBuffer to MP3 via FFmpeg
//! unbanished-cli nexus …                         search and install mods from Nexus Mods
//! unbanished-cli banishedinfo …                   list, search and install mods from banishedinfo.com
//! ```

mod banishedinfo;
mod nexus;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use unbanished_formats::crc::type_id;
use unbanished_formats::crs;
use unbanished_formats::pkg::{Archive, Package};
use unbanished_formats::types::{self, SoundBuffer};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn open_archive(dir: &Path) -> Archive {
    let mut archive = Archive::default();
    let mut found = 0;
    for entry in std::fs::read_dir(dir).unwrap_or_else(|e| fail(&format!("{}: {e}", dir.display())))
    {
        let path = entry.unwrap().path();
        if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("pkg"))
        {
            let pkg = Package::parse(std::fs::read(&path).unwrap())
                .unwrap_or_else(|e| fail(&format!("{}: {e}", path.display())));
            archive.add(pkg);
            found += 1;
        }
    }
    if found == 0 {
        fail(&format!(
            "no .pkg files in {} (run tools/extract-assets.sh)",
            dir.display()
        ));
    }
    archive
}

fn fail(msg: &str) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(1);
}

/// Decode every `SoundBuffer` to MP3 (xWMA is not playable in browsers) and write `index.json`.
fn export_audio(windata: &Path, out: &Path) {
    if Command::new("ffmpeg")
        .arg("-version")
        .stdout(Stdio::null())
        .status()
        .is_err()
    {
        fail("ffmpeg not found on PATH; it is needed to decode xWMA (brew install ffmpeg)");
    }
    let archive = open_archive(windata);
    let tmp = out.join(".tmp");
    std::fs::create_dir_all(&tmp).unwrap();
    let sound_id = type_id(SoundBuffer::CLASS);
    let mut index = Vec::new();
    let mut failures = 0;
    for name in archive.names() {
        if !name.ends_with(".crs") {
            continue;
        }
        let data = archive.read(name).unwrap();
        if crs::header(&data).map(|h| h.type_id) != Ok(sound_id) {
            continue;
        }
        let snd = types::sound_buffer(&data).unwrap_or_else(|e| fail(&format!("{name}: {e}")));
        let stream = if snd.streamed {
            Some(
                archive
                    .read(&snd.stream_entry)
                    .unwrap_or_else(|e| fail(&format!("{name}: {e}"))),
            )
        } else {
            None
        };
        let rel = name.trim_end_matches(".crs").replace('\\', "/");
        let mp3 = out.join(format!("{rel}.mp3"));
        std::fs::create_dir_all(mp3.parent().unwrap()).unwrap();
        let xwma = tmp.join("sound.xwma");
        std::fs::write(&xwma, snd.to_xwma(stream.as_deref())).unwrap();
        let ok = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
            .arg(&xwma)
            .args(["-codec:a", "libmp3lame", "-b:a", "128k"])
            .arg(&mp3)
            .status()
            .is_ok_and(|s| s.success());
        if !ok {
            eprintln!("ffmpeg failed for {name}");
            failures += 1;
            continue;
        }
        index.push(serde_json::json!({
            "resource": name,
            "file": format!("{rel}.mp3"),
            "source": snd.source,
            "seconds": snd.length_seconds,
            "looping": snd.looping,
            "streamed": snd.streamed,
            "channels": snd.format.channels,
            "sample_rate": snd.format.sample_rate,
        }));
        println!("{rel}.mp3  {:.1}s", snd.length_seconds);
    }
    std::fs::remove_dir_all(&tmp).ok();
    std::fs::write(
        out.join("index.json"),
        serde_json::to_string_pretty(&index).unwrap(),
    )
    .unwrap();
    println!(
        "wrote {} sounds to {} ({failures} failed)",
        index.len(),
        out.display()
    );
    if failures > 0 {
        std::process::exit(1);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = repo_root();
    let arg = |i: usize, default: PathBuf| args.get(i).map(PathBuf::from).unwrap_or(default);
    match args.first().map(String::as_str) {
        Some("export-audio") => export_audio(
            &arg(1, root.join("game-data/original/WinData")),
            &arg(2, root.join("game-data/derived/audio")),
        ),
        Some("nexus") => nexus::run(args[1..].to_vec(), root.join("game-data/original/WinData")),
        Some("banishedinfo") => {
            banishedinfo::run(args[1..].to_vec(), root.join("game-data/original/WinData"))
        }
        _ => {
            eprintln!("usage: unbanished-cli export-audio [WINDATA] [OUT]\n       unbanished-cli nexus …\n       unbanished-cli banishedinfo …");
            std::process::exit(2);
        }
    }
}
