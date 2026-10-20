# banished-banishedinfo

Optional [banishedinfo.com](https://banishedinfo.com/mods/browse) support for Un-Banished: list
and search the site's Banished mods, read a mod's versions, and install the `.pkm` packages of a
version. It is not part of the original game.

The site has no public API. The crate reads what its own pages use:

| What | Where |
|---|---|
| The mod list, by order and section | `/mods/ajax?action=mods&start=…&count=…&section=…&sort=…` (JSON) |
| Search results | the search page's `var mods = […]` |
| A mod's versions | the links `/mods/download/<id>-<name>/<n>` on its page |
| A version's file | `/mods/file/<id>-<name>/<n>`, which redirects to the download |

No account is needed. Requests carry a user agent that names Un-Banished, and are made only
when the player lists, searches or installs. The site sends no CORS headers, so only native
builds can use it (feature `native`).

```rust,ignore
use banished_banishedinfo::{native::Client, Sort};

let client = Client::new("0.2.0");
for m in client.list(Sort::Downloads, "", 0, 20)? {
    println!("{} {} by {}", m.id, m.title, m.author);
}
let m = banished_banishedinfo::Mod::with_id(34);
let newest = client.versions(&m)?.remove(0);
client.install(&newest, "WinData".as_ref(), |_, _| {})?;
```
