# Save reference identities

This finding catalogs the external-reference IDs serialized at the start of each supplied game
state. The IDs pair save-internal integers with exact resource-path strings. They are a prerequisite
for decoding internal object records whose serialized fields point into the table; they are not
process pointers or globally stable resource IDs.

## Evidence and output

`tools/save_reference_inventory.py` reads the save container, expands the game-state compressed
stream with the existing LZF implementation, validates its outer version-3 block, and exports the
first nested block as [`SAVE-EXTERNAL-REFERENCES.tsv`](SAVE-EXTERNAL-REFERENCES.tsv). The TSV
preserves the source path spelling and includes the save filename, full save SHA-256, sequential
reference ID, and path. Regenerate it from the four supplied files with:

```sh
python3 tools/save_reference_inventory.py
```

| Save | SHA-256 | Reference rows | Unique paths |
|---|---|---:|---:|
| `starter-save.sav` | `10a59afbcdfb4238040400f664c20a3d42654529984cb331b164b379339056f9` | 5,352 | 5,352 |
| `Save2.sav` | `896d28ddbfcff14f43a2eb5b6688d6d1c07c1ad415a1a57dce2a9bcd450bf5fa` | 5,479 | 5,479 |
| `Auto.sav` | `e9884af36891cea7646c2f8c61a855ef5c732e823470051f17ffdf7d1cb35b34` | 5,479 | 5,479 |
| `allportez.sav` | `15256bf48a32f713aa61c270c8412d426e94ce110e7bd1f768261fadfb571653` | 12,583 | 12,583 |

Each table's IDs are sequential from 1, and each path appears once within its save. Across all four
saves there are 28,893 rows and 12,674 distinct serialized path strings. `Save2` and `Auto` have
identical path lists in identical ID order. `starter-save` and `Save2` share 5,324 paths, but only
185 retain the same ID. `Save2` and `allportez` share 5,416 paths and none retain the same ID. These
comparisons show why an ID must always be qualified by its save identity (the hash), then resolved
through that save's reference table.

## Crosswalk to package resources

Most reference strings use the engine's `.rsc:member` notation, while the package inventory stores
compiled members as `.crs` paths with the member appended to the filename. To compare the namespaces,
normalize backslashes to slashes, transform `base.rsc:member` to `base_member.crs`, and transform a
plain terminal `.rsc` to `.crs`. Under that explicit spelling conversion, 21,806 of the 28,893 rows
join exactly to [`GAME-RESOURCE-INVENTORY.tsv`](GAME-RESOURCE-INVENTORY.tsv), representing 5,596
distinct reference-string/package-path pairs. Example:

```text
Audio\Animal\Chicken.rsc:event
    -> Audio/Animal/Chicken_event.crs
```

The remaining unique paths are not silently discarded or guessed. They may use other engine
resource conventions, refer to mod or generated resources absent from the original package set, or
need additional mapping evidence. The four-save package baseline and path inventory are documented
in [GAME-IDENTITY-MAP.md](GAME-IDENTITY-MAP.md).

## What this establishes and what remains

The rows establish the exact path associated with a reference-table ID in each hashed save and give
a reproducible crosswalk for many paths in the shipped package namespace. They do not yet identify
which serialized object fields contain reference IDs, reconstruct object types from those records,
or prove that every string names a file directly present in the packages. The next save-decoding step
is to trace the game-state serializers and tag each reference-bearing field with its object type,
field offset, and save-local ID semantics. Runtime object pointers require a separate owner and
lifetime observation.
