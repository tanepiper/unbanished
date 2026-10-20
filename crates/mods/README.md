# unbanished-mods

The mod list of Banished 1.0.7 (`IO::Modification`), shared by the Un-Banished game and its mod
manager, so both read and write the same settings in the same way.

| Item | What it is |
|---|---|
| `Modification` | The `.pkm` packages found, with which are enabled (`ModEnabled_<file>`), their order (`ModIndex_<file>`) and their conflicts; `refresh` (`RefreshList`), `save_list` (`SaveList`), `enable`, `swap`, `add` |
| `Mod` | One package: name, author, description, versions, crc, icon and files |
| `Settings` | Where the list keeps its values: the game's `Persistence`, or a `TextSettings` |
| `text::TextSettings` | The settings file as `name=value` lines, keeping every value it does not set |
| `find` | The `.pkm` files in a folder (native) |

```rust,ignore
use unbanished_mods::{text::TextSettings, Modification};

let path = unbanished_mods::text::settings_path().unwrap();
let mut settings = TextSettings::load(&path)?;
let mut mods = Modification::refresh(unbanished_mods::find("WinData".as_ref()), &settings);
mods.enable(0, true);
mods.save_list(&mut settings);
settings.save(&path)?;
```
