# Font placement and missing glyphs

## Static engine evidence

The existing Ghidra 12.1.4 export in
`analysis/export/Runtime-steam-x64.dll/decomp.c` establishes the baseline formula.
The export and original binaries remain local and ignored by Git.

- `UI::Font::Serialize`, `0x180119f60`, reads size at Font `+0x50`, the first
  trailing serialized integer at `+0x54`, and ellipsis/space advances at `+0x58/5c`.
- `UI::TextElement::Draw`, `0x180121b10`, passes element top + size − the integer
  at `+0x54` to `SpriteContext::AddFont`. Thus that serialized integer is the
  descent and the text baseline is **top + size − descent**.
- `UI::Font::ComputeStringLayout`, `0x180119860`, starts line coordinates at zero
  and advances subsequent lines by size.
- `UI::Font::ComputeLineLayout`, `0x180119500`, stores glyph y as line y − metric
  d − sprite height; x adds metric a and previous c, then advances by b.
- `UI::SpriteContext::AddFont`, `0x18011f540`, adds the supplied baseline origin
  to those layout coordinates without an additional vertical adjustment.

Consequently the port uses **glyph top = element top + size − descent + line
offset − d − sprite height**. The previous maximum of sprite height + d was an
estimate and is no longer used.

Source DLL SHA-256:
`d1c777b56674303ee276e807b88a7b73089d779f5dc923731ed45ef214cc7936`.
Ghidra export SHA-256:
`d705a243b8d6ab5573229b66c58e4b2ee98147c5e4ac7d5c9c1a62835da2b8cf`.
These are static observations from the existing export, not runtime execution or
new REA Evidence IDs. The current REA Ghidra provider reports unconfigured; no
other decompiler provider was used.

## Shipped-data checks

All four shipped fonts were loaded through `Resources` and measured against their
sprite sheets:

| Resource | Size | Descent | Traced baseline | Previous estimate |
| --- | ---: | ---: | ---: | ---: |
| FontLarge | 48 | 9 | 39 | 33 |
| FontMedium | 18 | 3 | 15 | 13 |
| FontSmall | 14 | 3 | 11 | 10 |
| Font | 24 | 5 | 19 | 17 |

Each character map contains 170 entries (0 through 169). For each font, `A · ☃`
now produces five positioned glyphs, including both spaces, a synthesized middle
dot, and a question mark. No font images or other game assets are added to Git.

## Fallback policy

Fallback is a port behavior; the native `ComputeLineLayout` skips absent glyphs.
An actual glyph from a supplied font always takes precedence. For missing glyphs:

1. A nonbreaking space uses the atlas space and retains its advance.
2. A middle dot uses the atlas period bitmap, vertically centered on the x-height
   (or capital H height if x is unavailable). Its width and bearings remain those
   of the period. This makes separators visible without an additional font atlas.
3. Other characters use U+FFFD when available, otherwise the question mark. These
   replacement glyphs participate in width measurement and wrapping.

`TextLayout::missing_characters` reports the distinct absent input characters and
debug logging reports substitutions. A font with neither a direct glyph nor a
replacement cannot draw that character, but it is still recorded in this list.
This does not add full Unicode coverage: arbitrary scripts remain represented by
replacement glyphs, and the synthesized middle dot is an approximation.

The focused tests cover serialized baseline/descent, multiline spacing, middle
dot placement, direct Unicode glyph precedence, replacement width and wrapping,
nonbreaking space advances, and reporting when no replacement exists. Browser
pixel comparison against native game execution remains outside this evidence.
