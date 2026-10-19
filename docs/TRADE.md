# Trade: the trading post, merchants and the trader boat

Port: `crates/game/src/commerce/trade.rs` (TradeComponent, offers, automatic trading, TradeWork) and
`crates/game/src/commerce/boat.rs` (BoatComponent). Test: `a_merchant_comes_by_boat`
(`apps/game/src/tools/placement_tool.rs`).

## TradeDescription

`Template\TradingPost.rsc:trade` (serializer `FUN_14005d150`, constructor `FUN_14005d400`):

- `+0x30` the boat (`Template\TraderBoat.rsc`);
- `+0x48` the merchants (`FUN_14005d510`, 0x48 bytes): name (string table, key), `+0x20` goods
  they always offer (none in the data), `+0x30` order kinds (ints: 0 crops, 1 trees, 2 livestock
  for the general and seed merchants, 2 for livestock; read by the order window), `+0x40` lots,
  `+0x10` the raw-material flags they deal in, `+0x14` the flags they buy at full value, `+0x18`
  at three quarters;

| Merchant | Lots | `+0x10` | `+0x14` | `+0x18` |
|---|---|---|---|---|
| MerchantFood | 15 | `0x20` | `0x36220` | `0x1c40` |
| MerchantGoods | 5 | `0x35e40` | `0x35e40` | 0 |
| MerchantGeneral | 20 | `0x37e60` | `0x37e60` | 0 |
| MerchantSeed | 2 | 0 | `0x15e40` | 0 |
| MerchantLivestock | 2 | 0 | `0x15e40` | 0 |

- `+0x58` rows by population (`FUN_14005d480`): below 30 / 60 / 90 / 120 / 150 / 180 people, a
  merchant every 18 ± 6 / 12 ± 4 / 7 ± 4 / 6 ± 2 / 6 ± 2 / 5 ± 1 months, lot and amount divisors
  8 / 6 / 4 / 3 / 2 / 1; past the last row, the last;
- `+0x20/+0x28` the trade window's sprite sheet and strings, `+0x38` 14 categories (flags,
  two string keys: logs, stone, iron, firewood, coal, tools, leather, coats, grain, meat, fruit,
  vegetables, herbs, ale);
- `+0x68` (0.33) and `+0x6c` (2): trading's visitor disease risk and its months.

**ValueDescription** (`<Template>.rsc:value`, serializer `FUN_14005d6c0`, constructor
`FUN_14005d7e0`): icon (sprite sheet, name), name (string table, key), `+0x40` the value of
one, `+0x44` a lot (default 100), `+0x48` the smallest lot. Raw materials, seeds (natural
resources) and livestock have one.

## TradeComponent

Vtable `0x140160df0`, constructor `FUN_14005cd90`; serializer `FUN_14005c6b0`. Fields: `+0x28`
trading by itself (0 never, 1 on a merchant's arrival, 2 on its leaving), `+0x2c` the merchant
docked (−1), `+0x30` one order per merchant (name, goods, state 0 none / 1 once / 2 every time),
`+0x40` per category an automatic amount (flags, index, amount), `+0x50` the merchant's goods
(good, value, amount), `+0x60` the goods the post keeps (raw material → amount), `+0x70`
TradeWork on, `+0x71` held (the trade window is open), `+0x78` the timer, `+0x80` the boat.

**Activation** (slot `+0x30`, `FUN_14005cb30`): the post joins the town's (`+0x30`); each
merchant a name (`FUN_14005cb80`: `Rand(2) == 0` a man, then `FUN_1400aa130` from that sex's
database); each category no amount. The work button (slot `+0x70`, `FUN_14005c960`) turns the
workplace and its TradeWork on or off.

| State | Enter / update / exit | Transitions | Messages |
|---|---|---|---|
| 2 | `FUN_14005b530`: the TradeWork (mask 1) when the workplace is on and none is registered; the timer `(row months + RandFloatTol × range) × 300 s` (`FUN_14005bdb0`, the row from `FUN_14005be50`). `FUN_14005b3c0`: when it is out, the boat entity is made, set up and activated; → 0x12 | 3 (`FUN_14005bac0`), 0x12 | |
| 0x12 | — | 3, 0x13, 2 | 9 (`FUN_14005b1b0`): docked; event 13 at severity 2 (colour `0xff8080ff`), → 0x13. 0xb (`FUN_14005b320`): the boat went without docking, → 2 |
| 0x13 | `FUN_14005b160`: the merchant's goods (`FUN_14005bee0`); it stays 900 s; with `+0x28 == 1` it trades by itself. `FUN_14005b0e0`: the stay runs down; out and not held, → 2. `FUN_14005b030` (exit): with `+0x28 == 2` it trades by itself; the boat casts off (message 0); the goods and merchant go | 3, 2 | 0xa (`FUN_14005bba0`): → 2 at once |

`FUN_14005bac0` (→ 3): the post leaves the town's; its boat sails out (`FUN_140046040` with no
post); the goods and merchant go.

**The merchant's goods** (`FUN_14005bee0`): `Rand(merchants)`; with an order (state ≠ 0, goods),
those goods, else `FUN_140059fd0`: its own goods then every ValueDescription template whose raw
material has a flag in `+0x10`, in the factory's order. A single order (state 1) is used up.
Then `max(lots / row lot divisor, 1)` times a random good: `max(value lot / row amount divisor,
smallest lot, 1)` of it at its value (× 1.25 when ordered, at least 1), added to its offer if
it has one. The seed and livestock merchants deal in no flags; without an order they have
nothing (the original reads the empty list without a check; the port brings nothing).

**The merchant's value** (`FUN_14005ae80`): with no merchant docked, the good's value; else
the full value for the merchant's `+0x14` flags, three quarters for `+0x18`, else nothing.

**Trading by itself** (`FUN_14005a430`): first the visitors' disease risk (`FUN_14005adf0`: the
town's `+0x1a8` at least `+0x68` for at least `+0x6c` months). Then per category with an amount:
the post's wealth is its goods at the merchant's value; the merchant's goods of that category
are bought one at a time, in turn, while fewer than the amount are bought and their value is
below the wealth (each one must fit the post's storage, else buying stops); then the post's
goods pay, item by item, one at a time, at the merchant's value, until the bought value is
paid (goods worth nothing to the merchant all go). The town's `+0x1ac` counts the value
traded away. The goods bought join the post's storage (`FUN_1401324d0`).

## TradeWork

Vtable `0x140167e30` (constructor `FUN_1400fc370`; `+0x48` each hauler and what it carries,
negative away from the post). Its try (`FUN_1400fb210`): at the most workers, 1; a hauler
carrying anything drops it off. For each of the town's raw materials, the amount the post keeps
less what is on its way and what it holds: too much is carried away (as much as the hauler
holds and the post has) to a storage that takes it, from a random use point; too little is
fetched from a storage that has it (as much as it has and the hauler holds, if the post takes
it) to a random use point. Complete, fail, cancel and refresh (`FUN_1400fb1f0`,
`FUN_1400fb1e0`, `FUN_1400fc0b0`, `FUN_1400fafb0`) keep the haulers.

## The boat

BoatComponent (vtable `0x14015f060`, constructor `FUN_140047160`, states `FUN_140047200`):
`+0x28` the post, `+0x40`/`+0x4c` the dock and its heading, `+0x50` the point it heads for,
`+0x60` the route (`+0x68` count, `+0x70` index). Setup (`FUN_140046040`): the post's "boat"
point (`FUN_140128d50`); the route is the map's first river line before it enters the map,
then a water path (`FUN_1400a5490`: costs 10 for land and lake shore, 5 for types 7 and 9, 1 for
river, diagonal, mask `0x380`) from where it enters to the dock and on to where it leaves, then
the river after its first point off the map again. The dock heading turns with the river's flow
at the route point nearest the dock (two points back, `FUN_1400ac7e0`).

| State | Enter / update / exit | Messages |
|---|---|---|
| 0 | `FUN_140045d50`: placed at the route's start, heading for the first point six away; clip 1. `FUN_140045bb0`: sail on (`FUN_140046d30`: within 3 of the point, the next at least 6 on, `FUN_1400ac590`); at the route's end gone (the post gets message 0xb); within 8 of the dock → 1 | 1 → 4 |
| 1 | `FUN_140045ab0`: clip 2. `FUN_140045950`: toward the dock at `clamp(distance × 0.125, 0.33, 1)` speed, turned to the dock heading; within 0.2 → 2 | 1 → 4 |
| 2 | `FUN_1400458d0`: clip 0; the post gets message 9 | 0 → 3; 1 → 4 |
| 3 | `FUN_140045820`: clip 3. `FUN_140045670`: back toward the point it heads for; within 3 → 4 | |
| 4 | `FUN_140045630`: clip 1. `FUN_1400455a0`: sail on; at the route's end gone | |

## The exchange

The trade window's button (`FUN_1400868f0`, see [BUILDING-WINDOWS.md](BUILDING-WINDOWS.md)
"TradeUI"), while a merchant is docked: the visitors' disease risk (`FUN_14005adf0`); each
good sold leaves the post's storage (`FUN_140132620`) and counts as used (`FUN_14008e460`);
the sold goods' value counts as traded away (stock manager `+0x1ac`). Each good bought leaves
the merchant's goods (`FUN_14005c400`, the offer gone at none) and arrives by its
description's components: a raw material goes into the post's storage (`FUN_1401324d0`) and
becomes known (`FUN_140097330`); a seed (NaturalResourceDescription) becomes known
(`FUN_140096ef0`); livestock becomes known and its animals appear at the post
(`FUN_14005b640`): at random between the post's first two create points, males and females in
turn from a random first (`Rand(2)`), turned at random (`RandFloatTol × 180`), not wild, kept
to the cells between the points until a pasture takes them; each `RandFloat` × half its
lifespan old, which is not yet set when this runs, so none.

Automatic trading also makes what it buys known.

The stock manager's known raw materials (`+0x110`): the start profile's, then each as a pile
of it is set up (RawMaterialComponent slot `+0x20`, `FUN_14012e790`) or it is traded for. The
trade window lists only these.

## Not ported

The per-good trade statistics (`+0x170`).
