# Game loop step 15: optional resource entity and accounting path

This step follows the optional branch guarded by `NaturalResourceComponent+0x2d` in
`FUN_14012b5c0`. It also distinguishes the counter updated here from the Status Bar's displayed
resource count. Evidence is static Ghidra pseudocode for the Steam x64 executable and existing
resource-table traces; no live gameplay capture or full save-object decode was made.

## Guard and created entity

The GrowthComponent threshold path clears `NaturalResourceComponent+0x2d` before calling
`FUN_14012bc50`, so that path does not create the optional entity. Another caller must leave or set
the byte for the branch to run. When set, `FUN_14012b5c0`:

1. Requires the `NaturalResourceDescription` reference at `+0x20` to resolve.
2. Reads an integer from the referenced resource object at `+0x40` or `+0x44`, selected by component
   byte `+0x2c`.
3. Multiplies by the component float `+0x28`, capped at `1.0`, truncates to an integer, and stops
   if the result is not positive.
4. Clones an entity from the referenced raw-material resource, positions it near the resource cell
   with a `RandTol(2)` offset, sets the cloned entity's component integer at `+0x28` to the amount,
   clears that component's field at `+0x30`, and registers the new entity with the entity manager.

In shipped descriptions, the reference is commonly a `Template\\RawMaterial*.rsc:resource` path.
That identifies the created object as a raw-material resource entity from the template path, but
does not by itself establish whether the count is collectible inventory, a yield, or another
world-accounting quantity.

## Separate resource counters

The clone path calls `FUN_14008e3c0` with a null owner pointer and a false update flag. Therefore its
optional owner-component update does not run. The function still resolves the material category
with `FUN_14008e460` and, for a recognized category, increments the integer at
`game + 0xf0` → owner `+0x170` → `category * 0x20 + 0x10` by the amount.

This is distinct from the Status Bar count, which reads the integer at category entry `+0x00`. The
table layout traced in [GAME-LOOP-STEP-08.md](GAME-LOOP-STEP-08.md) also records a limit at `+0x08`
and another path using `+0x0c`; this natural-resource call updates `+0x10`. Its exact accounting
meaning is not established. If `FUN_14008e3c0` is called with a non-null owner and true update flag,
it additionally finds an owner component and invokes `FUN_140059930` to add the same resource
reference and amount to that component's keyed array.

The category resolver `FUN_14008e460` selects an index by testing flags in the referenced resource's
component word at `+0x5c`. The mapping covers indices 0–20, matching the number of Status Bar
resource categories, but this step does not assume the `+0x10` counter has the same meaning as the
Status Bar's `+0x00` value.

## Evidence and limits

- `analysis/export/Application-steam-x64.exe/decomp.c`: `FUN_14012b5c0` (`0x14012b5c0`) guards,
  clones and registers the entity; `FUN_14008e3c0` (`0x14008e3c0`) updates owner and category
  counters; `FUN_14008e460` (`0x14008e460`) maps resource flags to a category; `FUN_140059930`
  (`0x140059930`) accumulates a resource-reference/amount pair.
- `FUN_140125520` (`0x140125520`) clears component byte `+0x2d` before its threshold dispatch.
- [GAME-LOOP-STEP-08.md](GAME-LOOP-STEP-08.md) maps Status Bar category index and value fields;
  [GAME-LOOP-STEP-14.md](GAME-LOOP-STEP-14.md) correlates the description flag with shipped
  natural-resource families.

## Next trace

Identify the other callers of `FUN_14012b5c0` and the producer of component byte `+0x2d`. Then trace
the `+0x10` category counter and `FUN_140059930`'s keyed component array to their consumers, keeping
both distinct from the Status Bar's `+0x00` count until their serializers and update owners are
identified.
