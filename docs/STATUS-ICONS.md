# Status icon billboards

Slice E. `StatusIconComponent` (type `0x7f050e80`, constructor `FUN_140131590`, component vtable
`0x14016aa50`) embeds a `Graphics::Geometry` at `+0x28` (vtable `0x1401654e0`) that the scene
draws. Port: `apps/game/src/status_icons.rs`, fed by `StartingTown::status_icons`.

## Description

`StatusIconDescription` (`0x1fbcf488`, serializer `FUN_140131650`, constructor `FUN_140039280`):

| Offset | Field | Shipped |
|---|---|---|
| `+0x20` | icon sprite sheet | `StatusIcons\BuildingIconSpriteSheet`, `StatusIcons\CitizenIconSpriteSheet` |
| `+0x28` | size (default 1.0) | buildings 0.04, citizens 0.025 |
| `+0x2c` | height offset, not serialized | always 1.0 |
| `+0x30` | screen-constant size | 1 |

All 31 shipped descriptions parse (`crates/formats/tests/templates.rs`).

## Geometry

`FUN_1400af910` (from the description getter `FUN_1401313a0`) sets geometry `+0xc0` size,
`+0xc4` sprite count, `+0xc8` mask, `+0xcc` screen-constant flag, `+0xd0` sheet, render class 2
and bounds. The component's `+0xf0` mask is the geometry's `+0xc8`; it is serialized by
`FUN_1401314c0` and read by StatusBarUI and StatusIconUI.

**Placement** (`FUN_1401310a0`): with the game option `StatusIconEnable` the geometry is in the
scene. Its transform is the model position plus the model box (`ModelComponent +0x5c`) height
(no MultiObject), otherwise the map rectangle's centre cell on the ground plus `+0x2c`.

**Draw** (`FUN_1400af0e0`):

- colour white with alpha lerped from the option `StatusIconOpacity / 100`;
- `size = +0xc0`; when screen-constant, divided by `|ndc.x(anchor) − ndc.x(anchor + right)|`
  through the inverse camera and projection, so the icon spans `size` of normalized device x;
- `size *= StatusIconScale * 0.01`; half = size / 2;
- the first icon's centre is `(0, 0, half)` above the anchor (world z); each set bit `i` in
  ascending order emits a quad of half-extents `right · half` and `up · half` with sprite `i`'s
  UVs (`SpriteSheet::GetUVs`), indices `0,1,2 0,2,3`; the next icon moves `up · size`.

Options (`DAT_1401cbde0`, defaults before `FUN_14009e030` loads `Game/StatusIcon*`): opacity
100, enabled, scale 100. The port uses these defaults until the Options dialog is ported.

## Port notes

- The quad builder is a direct transcription; the GPU pass draws after the selection outline,
  alpha-blended, depth-tested without depth writes. The original's render class 2 state is
  inferred, not traced.
- The citizen anchor uses the citizen mesh's bounds height (male/female mesh), unscaled; whether
  `+0x5c` is scaled for children is not established.
- Buildings: the bits follow `StatusIcons\BuildingIconSpriteSheet`'s sprites, kept in
  `WorkPlace::status` (houses add `Home::status`):

  | Bit | Sprite | Set / cleared by |
  |---|---|---|
  | 0 | BuildPaused | switched off while built (states 0, 1, 8; `FUN_1401350d0`); cleared on, and when building ends (`FUN_14010cad0`) |
  | 1 | WorkDisabled | switched off otherwise; cleared on |
  | 2 | LimitProduction | the works' tries (stock at its limit), a full school, doctor or chapel (`FUN_140044590`); cleared off, by `RemoveWork`, when the last worker leaves |
  | 3 | WorkUnavailable | ConsumeProduce without its inputs (`FUN_1400ecd10`); cleared likewise |
  | 4 | WorkersUnavailable | enabled with no workers of its own profession (`FUN_1401350d0`, `FUN_140134cb0`); cleared by a worker joining (`FUN_140134ef0`) or off |
  | 5 | BuildDestroy | marked for demolition (not ported with the demolition tool) |
  | 6, 7 | food, firewood | houses (`FUN_14012ee40`) |

  `StartingTown::building_icons` anchors them above the finished mesh's height, or for a
  MultiObject site at the map rectangle's centre plus 1.

Capture: seed 1234 New Game, zoom (`--wheel 8@9 --wheel 8@9.3 --wheel 6@9.6 --wheel -12@10`),
22 s: every homeless citizen shows the home icon; icons behind the barn are hidden.
