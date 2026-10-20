//! `banished-cli banishedinfo`: list, search and install mods from banishedinfo.com.

use std::path::PathBuf;

use banished_banishedinfo::native::Client;
use banished_banishedinfo::{Mod, Sort, SECTIONS};

const USAGE: &str = "usage:
  banished-cli banishedinfo list [--sort featured|downloads|trending|recent] [--section NAME] [--page N]
  banished-cli banishedinfo search TEXT
  banished-cli banishedinfo versions MOD_ID
  banished-cli banishedinfo install MOD_ID [VERSION] [--to WINDATA]
                                               install a version (the newest by default)

Sections: audio, buildings, gameplay, maps, textures, translations, translated-mods.
Requests go to banishedinfo.com only when you run a command; no account is needed.";

fn fail(message: &str) -> ! {
    eprintln!("error: {message}");
    std::process::exit(1);
}

fn option(args: &mut Vec<String>, name: &str) -> Option<String> {
    let i = args.iter().position(|a| a == name)?;
    args.remove(i);
    (i < args.len()).then(|| args.remove(i))
}

fn print(mods: &[Mod]) {
    for m in mods {
        println!(
            "{:>5}  {}  (by {}, version {}, {} downloads, {}% liked)\n       {}",
            m.id,
            m.title,
            m.author,
            m.version,
            m.downloads,
            m.rating,
            m.info.lines().next().unwrap_or("")
        );
    }
}

pub fn run(mut args: Vec<String>, default_dir: PathBuf) {
    let to = option(&mut args, "--to")
        .map(PathBuf::from)
        .unwrap_or(default_dir);
    let sort = option(&mut args, "--sort");
    let section = option(&mut args, "--section").unwrap_or_default();
    let page: u32 = option(&mut args, "--page")
        .map(|p| {
            p.parse()
                .unwrap_or_else(|_| fail("the page must be a number"))
        })
        .unwrap_or(1)
        .max(1);
    let client = Client::new(env!("CARGO_PKG_VERSION"));
    let id = |a: Option<&String>| -> u32 {
        a.and_then(|s| s.split('-').next()?.parse().ok())
            .unwrap_or_else(|| fail("give the mod's id (the number in its address)"))
    };
    match args.first().map(String::as_str) {
        Some("list") => {
            let sort = match sort.as_deref() {
                None | Some("featured") => Sort::Featured,
                Some("downloads") => Sort::Downloads,
                Some("trending") => Sort::Trending,
                Some("recent") => Sort::Recent,
                Some(other) => fail(&format!("unknown sort {other:?}")),
            };
            if !SECTIONS.iter().any(|(s, _)| *s == section) {
                fail(&format!("unknown section {section:?}"));
            }
            let per_page = 20;
            let mods = client
                .list(sort, &section, (page - 1) * per_page, per_page)
                .unwrap_or_else(|e| fail(&e.to_string()));
            print(&mods);
            println!(
                "page {page}{}",
                if mods.len() < per_page as usize {
                    " (the last)"
                } else {
                    ""
                }
            );
        }
        Some("search") => {
            let text = args[1..].join(" ");
            let mods = client
                .search(&text)
                .unwrap_or_else(|e| fail(&e.to_string()));
            print(&mods);
            println!("{} mods", mods.len());
        }
        Some("versions") => {
            let m = Mod::with_id(id(args.get(1)));
            for v in client.versions(&m).unwrap_or_else(|e| fail(&e.to_string())) {
                println!("{:<10} {}", v.label, v.file);
            }
        }
        Some("install") => {
            let m = Mod::with_id(id(args.get(1)));
            let versions = client.versions(&m).unwrap_or_else(|e| fail(&e.to_string()));
            let version = match args.get(2) {
                Some(label) => versions
                    .iter()
                    .find(|v| &v.label == label)
                    .unwrap_or_else(|| {
                        fail(&format!("no version {label}; see `versions {}`", m.id))
                    }),
                None => &versions[0],
            };
            if !to.is_dir() {
                fail(&format!("{} is not a folder", to.display()));
            }
            eprintln!("downloading version {}…", version.label);
            match client.install(version, &to, |_, _| {}) {
                Ok(paths) => {
                    for p in paths {
                        println!("installed {}", p.display());
                    }
                    println!("enable it in the game's Mods dialog");
                }
                Err(e) => fail(&format!("{e} (the mod's page: {})", m.page())),
            }
        }
        _ => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    }
}
