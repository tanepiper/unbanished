//! `unbanished-cli nexus`: search Banished mods on Nexus Mods and install them, with the
//! player's API key (`UNBANISHED_NEXUS_API_KEY`, or `NexusApiKey=` in the game's settings file).

use std::path::PathBuf;

use unbanished_nexus::api::{self, Sort};
use unbanished_nexus::native::{self, Source, Step};
use unbanished_nexus::nxm::NxmLink;
use unbanished_nexus::App;

const USAGE: &str = "usage:
  unbanished-cli nexus whoami                    the account the API key belongs to
  unbanished-cli nexus search [TEXT] [--sort downloads|endorsements|updated|name] [--page N]
  unbanished-cli nexus files MOD_ID              a mod's files
  unbanished-cli nexus install MOD_ID [FILE_ID] [--to WINDATA]
                                               install a file (Premium); the main file by default
  unbanished-cli nexus install 'nxm://banished/mods/…' [--to WINDATA]
                                               install from a \"Mod Manager Download\" link

The API key comes from UNBANISHED_NEXUS_API_KEY, or the NexusApiKey= line of the game's
settings file. Searches and file lists need no key.";

fn app() -> App {
    App::un_banished(env!("CARGO_PKG_VERSION"))
}

fn fail(message: &str) -> ! {
    eprintln!("error: {message}");
    std::process::exit(1);
}

fn key() -> String {
    native::stored_key().unwrap_or_else(|| {
        fail(&format!(
            "no API key: set {} or add {}=YOUR_KEY to {}",
            native::KEY_VARIABLE,
            native::KEY_SETTING,
            native::settings_path().map_or("the settings file".into(), |p| p.display().to_string())
        ))
    })
}

/// The value after `--name`, and the arguments without the option and its value.
fn option(args: &mut Vec<String>, name: &str) -> Option<String> {
    let i = args.iter().position(|a| a == name)?;
    args.remove(i);
    (i < args.len()).then(|| args.remove(i))
}

fn number(text: &str, what: &str) -> u32 {
    text.parse()
        .unwrap_or_else(|_| fail(&format!("{what} must be a number, not {text:?}")))
}

pub fn run(mut args: Vec<String>, default_dir: PathBuf) {
    let to = option(&mut args, "--to")
        .map(PathBuf::from)
        .unwrap_or(default_dir);
    let sort = option(&mut args, "--sort");
    let page = option(&mut args, "--page").map_or(1, |p| number(&p, "the page").max(1));
    let app = app();
    let run = |r: &unbanished_nexus::Request, download| {
        native::send(r, download).unwrap_or_else(|e| fail(&e.to_string()))
    };
    match args.first().map(String::as_str) {
        Some("whoami") => {
            let user = api::parse_user(&run(&api::validate(&app, &key()), false))
                .unwrap_or_else(|e| fail(&e.to_string()));
            let premium = if user.premium {
                "Premium"
            } else {
                "not Premium"
            };
            println!("{} ({premium})", user.name);
        }
        Some("search") => {
            let sort = match sort.as_deref() {
                None | Some("downloads") => Sort::Downloads,
                Some("endorsements") => Sort::Endorsements,
                Some("updated") => Sort::Updated,
                Some("name") => Sort::Name,
                Some(other) => fail(&format!("unknown sort {other:?}")),
            };
            let text = args[1..].join(" ");
            let per_page = 20;
            let request = api::search(&app, &text, sort, (page - 1) * per_page, per_page);
            let found =
                api::parse_search(&run(&request, false)).unwrap_or_else(|e| fail(&e.to_string()));
            for m in &found.mods {
                println!(
                    "{:>5}  {}  (by {}, version {}, {} downloads)\n       {}",
                    m.mod_id, m.name, m.author, m.version, m.downloads, m.summary
                );
            }
            let pages = found.total.div_ceil(per_page).max(1);
            println!("page {page} of {pages}, {} mods", found.total);
        }
        Some("files") => {
            let mod_id = number(args.get(1).map_or("", String::as_str), "the mod id");
            let files = api::parse_files(&run(&api::files(&app, mod_id), false))
                .unwrap_or_else(|e| fail(&e.to_string()));
            let main = api::main_file(&files).map(|f| f.file_id);
            for f in &files {
                let mark = if Some(f.file_id) == main { "*" } else { " " };
                println!(
                    "{mark}{:>6}  {:<13} {:>10} bytes  {}  ({})",
                    f.file_id, f.category, f.size, f.name, f.file_name
                );
            }
            println!(
                "* the file `install {mod_id}` takes; {}",
                api::files_page(mod_id)
            );
        }
        Some("install") => {
            let what = args.get(1).cloned().unwrap_or_default();
            let source = if what.starts_with("nxm://") {
                Source::Link(NxmLink::parse(&what).unwrap_or_else(|e| fail(&e.to_string())))
            } else {
                let mod_id = number(&what, "the mod id");
                let files = api::parse_files(&run(&api::files(&app, mod_id), false))
                    .unwrap_or_else(|e| fail(&e.to_string()));
                let file = match args.get(2) {
                    Some(f) => {
                        let id = number(f, "the file id");
                        files.iter().find(|f| f.file_id == id).cloned()
                    }
                    None => api::main_file(&files).cloned(),
                }
                .unwrap_or_else(|| fail("no such file; see `nexus files MOD_ID`"));
                Source::File {
                    mod_id,
                    file_id: file.file_id,
                    file_name: file.file_name,
                }
            };
            if !to.is_dir() {
                fail(&format!("{} is not a folder", to.display()));
            }
            let mut last = 0;
            let result = native::install_from(&app, &key(), &source, &to, |step| match step {
                Step::Link => eprintln!("getting the download link…"),
                Step::Download(done, total) => {
                    let percent = total.map(|t| done * 100 / t.max(1));
                    if let Some(p) = percent.filter(|p| *p >= last + 10 || *p == 100) {
                        eprintln!("downloading… {p}%");
                        last = p;
                    }
                }
                Step::Install => eprintln!("installing…"),
            });
            match result {
                Ok(paths) => {
                    for p in paths {
                        println!("installed {}", p.display());
                    }
                    println!("enable it in the game's Mods dialog");
                }
                Err(unbanished_nexus::Error::NeedsPremium) => {
                    let mod_id = match &source {
                        Source::File { mod_id, .. } => *mod_id,
                        Source::Link(l) => l.mod_id,
                    };
                    fail(&format!(
                        "this account needs Premium for direct downloads. Open {}, press \"Mod \
                         Manager Download\" on the file, copy the nxm:// link and run \
                         `unbanished-cli nexus install 'nxm://…'`",
                        api::files_page(mod_id)
                    ))
                }
                Err(e) => fail(&e.to_string()),
            }
        }
        _ => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    }
}
