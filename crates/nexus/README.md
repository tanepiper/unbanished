# unbanished-nexus

Optional [Nexus Mods](https://www.nexusmods.com/games/banished/mods) support for Un-Banished.
It is not part of the original game: Banished's Steam version browses the Steam Workshop in the
Mods menu's Browse tab, and Un-Banished can show that tab for Nexus Mods instead, when the player
has set an API key.

| Module | What it contains |
|---|---|
| `api` | The requests (search, a mod's files, the account, download links) and their answers |
| `nxm` | `nxm://` links from the website's "Mod Manager Download" button |
| `install` | The `.pkm` packages in a download (`.zip` or a bare `.pkm`) |
| `native` | With the `native` feature: sending requests and downloading (ureq), writing packages, and finding the API key |

The crate builds each request as a `Request` (method, address, headers, body) and reads each
answer from its bytes. So the game sends them with `fetch` in the browser and with
`native::send` on the desktop.

## Use it

```toml
[dependencies]
unbanished-nexus = { path = "../../crates/nexus", features = ["native"] }
```

```rust,ignore
use unbanished_nexus::{api, native, App};

let app = App::un_banished("0.2.0");
// Searches need no key.
let page = api::parse_search(&native::send(&api::search(&app, "terrain", api::Sort::Downloads, 0, 20), false)?)?;
for m in &page.mods {
    println!("{} {} by {}", m.mod_id, m.name, m.author);
}
// Downloads need the player's key, and Premium or an nxm:// link.
let key = native::stored_key().expect("set UNBANISHED_NEXUS_API_KEY");
let files = api::parse_files(&native::send(&api::files(&app, 19), false)?)?;
let file = api::main_file(&files).unwrap();
let source = native::Source::File { mod_id: 19, file_id: file.file_id, file_name: file.file_name.clone() };
native::install_from(&app, &key, &source, "WinData".as_ref(), |_| {})?;
```

## Rules

Nexus Mods asks every application to send its name and version (`Application-Name`,
`Application-Version`), which `api::headers` does. A personal API key is for testing and personal
use; a public build of Un-Banished needs a registered application. The key stays with the player
(the settings file, `UNBANISHED_NEXUS_API_KEY`, or the browser's storage) and is sent only to
`api.nexusmods.com`.

Of the 80 Banished mods listed in October 2026, the main file was a `.zip` for 62, a `.7z` for 7
and a `.rar` for 9. Only `.zip` and bare `.pkm` files are unpacked here.
