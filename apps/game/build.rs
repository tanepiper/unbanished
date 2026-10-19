//! Sets `UNBANISHED_BUILD`, the build id in the engine version `0.1.0-<build>` (see `version.rs`).
//!
//! The build id is chosen in this order:
//! 1. `UNBANISHED_BUILD` from the environment, when it is set and not empty (CI and packagers);
//! 2. the short git commit hash (7 characters), with `-dirty` when tracked files have changes
//!    that are not committed;
//! 3. `GITHUB_SHA` cut to 7 characters (a CI checkout where `git` cannot run);
//! 4. `unknown` (a source archive without `.git`, or no `git` program).
//!
//! The build script runs on the host, so native and wasm32 builds get the same id.
use std::path::Path;
use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Ask cargo to run this script again when the commit or the working tree changes.
fn watch(root: &Path) {
    for path in ["HEAD", "index", "packed-refs"] {
        if let Some(p) = git(&["rev-parse", "--git-path", path]) {
            if Path::new(&p).exists() {
                println!("cargo:rerun-if-changed={p}");
            }
        }
    }
    if let Some(r) = git(&["symbolic-ref", "-q", "HEAD"]) {
        if let Some(p) = git(&["rev-parse", "--git-path", &r]) {
            if Path::new(&p).exists() {
                println!("cargo:rerun-if-changed={p}");
            }
        }
    }
    // Edits that are not staged yet change the `-dirty` suffix too. These are the sources that
    // go into the game; build output folders are left out so a build does not trigger itself.
    for path in [
        "Cargo.toml",
        "Cargo.lock",
        "crates",
        "apps/game/src",
        "apps/game/index.html",
        "apps/game/Cargo.toml",
    ] {
        let p = root.join(path);
        if p.exists() {
            println!("cargo:rerun-if-changed={}", p.display());
        }
    }
}

fn from_git() -> Option<String> {
    let hash = git(&["rev-parse", "--short=7", "HEAD"])?;
    let dirty = git(&["status", "--porcelain", "--untracked-files=no"])
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    Some(if dirty { format!("{hash}-dirty") } else { hash })
}

fn main() {
    println!("cargo:rerun-if-env-changed=UNBANISHED_BUILD");
    println!("cargo:rerun-if-env-changed=GITHUB_SHA");
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let root = Path::new(&manifest).join("../..");
    let build = match std::env::var("UNBANISHED_BUILD") {
        Ok(b) if !b.trim().is_empty() => b.trim().to_string(),
        _ => {
            let git_build = from_git();
            if git_build.is_some() {
                watch(&root);
            }
            git_build
                .or_else(|| {
                    std::env::var("GITHUB_SHA")
                        .ok()
                        .filter(|s| s.len() >= 7)
                        .map(|s| s[..7].to_string())
                })
                .unwrap_or_else(|| "unknown".to_string())
        }
    };
    println!("cargo:rustc-env=UNBANISHED_BUILD={build}");
}
