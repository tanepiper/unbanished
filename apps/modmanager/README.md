# banished-modmanager

The Un-Banished mod manager (binary `banished-mods`): a small desktop program to choose mods
before the game starts. It is not part of the original game.

<p align="center">
  <img src="https://tanepiper.github.io/unbanished/assets/screenshots/mod-manager.webp" alt="The mod manager with five mods, three of them on, and the details of the selected mod, Better Fields" width="860">
</p>
<p align="center">
  <img src="https://tanepiper.github.io/unbanished/assets/screenshots/mod-manager-nexus.webp" alt="The Nexus Mods tab: the API key box, a search, and the most downloaded mods with previews" width="425">
  <img src="https://tanepiper.github.io/unbanished/assets/screenshots/mod-manager-banishedinfo.webp" alt="The banishedinfo.com tab: a search, an order and a section, and mods with Install newest and Versions buttons" width="425">
</p>

- Turn mods on and off, filter them, and change their order (drag, or ⤒ ↑ ↓). The first mod in
  the list wins when two mods change the same file; conflicts are marked.
- See a mod's icon, description, conflicts and files.
- Save and load named mod lists (`Documents/Banished/ModLists/NAME.txt`: one `+file` or `-file`
  line per mod, in order).
- Search and install mods from Nexus Mods (the **Nexus Mods** tab; installs need the player's API
  key; `banished-nexus`).
- List, search and install mods from banishedinfo.com (the **banishedinfo.com** tab; no account;
  `banished-banishedinfo`). Both site tabs show previews (PNG, JPEG or WebP).
- Play: save, then start `banished` (beside this program, or on the `PATH`) with the same game
  data.

```bash
cargo run --release -p banished-modmanager -- path/to/WinData
```

It reads the `.pkm` packages and writes the settings file through `banished-mods`, the same code
the game's Mods dialog uses, so both agree on what is enabled and in what order. The settings
file is read again before each save, so values the game wrote are kept. Close the game first:
the game writes the file when it closes.

The window is made with [egui](https://github.com/emilk/egui) (eframe, OpenGL).

For tests, `BANISHED_MODS_SCREENSHOT=out.png` saves the window after four seconds and quits;
`BANISHED_MODS_TAB=nexus` (or `banishedinfo`) opens that site's tab and `BANISHED_MODS_SELECT=N` selects a mod.
`BANISHED_SETTINGS` and `BANISHED_USER_DIR` work as for the game.
