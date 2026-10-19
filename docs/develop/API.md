# API documentation

The libraries of Un-Banished are documented in two ways:

- **A README for each crate.** It says what the crate is for, lists its main types and
  functions, and gives examples. Start there. [Apps and crates](CRATES.md) links to every README.
- **The Rust API documentation.** `cargo doc` generates it from the doc comments in the source
  code. It lists every public module, type and function. The website publishes it at
  [/api/](https://tanepiper.github.io/unbanished/api/index.html), and it is rebuilt after each push to `main`.

## Why both

The doc comments in the source code are written for people who change the code. They name the
original function that each part ports (for example `FUN_1400d4860`), the field offsets in the
original's objects, and the evidence. This detail is necessary, and it is complete, because each
public item has its comment next to it. But it does not tell a new reader where to start.

The READMEs are short and written by hand. They explain the purpose of a crate, the few types
that most callers need, and how to use them, with examples. They do not try to list everything.

So the API documentation is generated, not summarised by hand: a hand-written copy of every
public item would go out of date after the next change. The READMEs only summarise, and they
link to the generated pages for the details.

## The crates on /api/

| Crate | API documentation |
|---|---|
| `banished-core` | [banished_core](https://tanepiper.github.io/unbanished/api/banished_core/index.html) |
| `banished-terrain` | [banished_terrain](https://tanepiper.github.io/unbanished/api/banished_terrain/index.html) |
| `banished-path` | [banished_path](https://tanepiper.github.io/unbanished/api/banished_path/index.html) |
| `banished-game` | [banished_game](https://tanepiper.github.io/unbanished/api/banished_game/index.html) |
| `banished-formats` | [banished_formats](https://tanepiper.github.io/unbanished/api/banished_formats/index.html) |
| `banished-ui` | [banished_ui](https://tanepiper.github.io/unbanished/api/banished_ui/index.html) |
| `banished-gfx` | [banished_gfx](https://tanepiper.github.io/unbanished/api/banished_gfx/index.html) |
| `banished-audio` | [banished_audio](https://tanepiper.github.io/unbanished/api/banished_audio/index.html) |
| `banished-wma` | [banished_wma](https://tanepiper.github.io/unbanished/api/banished_wma/index.html) |

The apps (`banished`, `banished-viewer`, `banished-cli`) are programs, not libraries. Their
READMEs describe how to use them.

## Build it yourself

```bash
cargo doc --no-deps -p banished-formats --open      # one crate, opened in your browser
site/scripts/build-api.sh                           # all the libraries, into site/public/api/
```

`build-api.sh` uses its own target folder, `target/api`. On Linux, it needs the ALSA headers, as
a normal build does (`libasound2-dev`).
