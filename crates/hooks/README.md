# unbanished-hooks

Rule hooks from WebAssembly plugins (not in the original): small programs from mods that run
after every iteration of the town, read the date, the citizens and the stock, draw from the
game's own random stream and post lines to the event log. It implements the core's door,
`unbanished_sim::hooks::RuleHooks`, with [wasmi](https://github.com/wasmi-labs/wasmi), an
interpreter that runs the same on every platform. The game uses it behind its `rule-hooks`
feature. [RULE-HOOKS.md](../../docs/develop/RULE-HOOKS.md) explains the API and what keeps it
deterministic; [Writing a rule hook](../../docs/guide/WRITING-RULE-HOOKS.md) builds an example.

| Item | What it is |
|---|---|
| `Plugins` | The plugins of one game, run in order: `files(dir)` (the `.wasm` files of a folder, by name), `load`, `names`, and `RuleHooks` |
| `fingerprint` | A CRC over the plugins' names, lengths and bytes, in order (0 with none), for multiplayer's `Hello` |
| `plugin::Plugin` | One plugin: checked when loaded (no floats, only the game's imports, API version 1), run with a fixed fuel budget and switched off when it fails |
| `plugin::API_VERSION`, `plugin::FUEL` | API version 1; 2,000,000 units of fuel per call |

```rust,ignore
use unbanished_hooks::Plugins;

let files = Plugins::files(&user_dir.join("Hooks"));
let plugins = Plugins::load(&files)?;
sim.hooks = Some(Box::new(plugins));
```

`cargo run -p unbanished-hooks --example wat2wasm IN.wat OUT.wasm` turns a plugin in the
WebAssembly text format into a `.wasm` file.
