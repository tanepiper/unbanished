import re, sys
src = sys.argv[1]; out = sys.argv[2]
codes = open(f"{src}/src/wire_codes.rs").read()
bands = open(f"{src}/src/band_partition.rs").read()

def arr(text, name):
    m = re.search(r"pub const %s: \[\((u\d+), (u\d+)\); (\d+)\] = \[(.*?)\n\];" % name, text, re.S)
    body = m.group(4)
    items = re.findall(r"\((\d+), (0b[01]+|\d+)\)", body)
    assert len(items) == int(m.group(3)), name
    return [(int(a), int(b, 0)) for a, b in items]

def band(name):
    m = re.search(r"const %s: \[u16; (\d+)\] = \[(.*?)\];" % name, bands, re.S)
    v = [int(x) for x in re.findall(r"\d+", m.group(2))]
    assert len(v) == int(m.group(1))
    return v

o = []
o.append("""//! Codec constant tables for WMA v1/v2.
//!
//! These are bitstream-defining constants (VLC codewords, run/level maps, band layouts) that any
//! compatible decoder must reproduce exactly. They were taken from the MIT-licensed
//! `oxideav-wma` 0.0.4 crate (Copyright (c) 2026 Karpelès Lab Inc., MIT License), whose
//! provenance notes state they were extracted from the vendor WMA decoder's data sections.
//! See THIRD_PARTY.md. Regenerate with `tools/gen-wma-tables.py <oxideav-wma-0.0.4 dir> <out>`;
//! do not edit by hand.
//!
//! VLC entries are `(length, codeword)`, codewords read MSB first. Run/level maps give
//! `(run, |level|)` for coefficient symbols `2..`; symbol 0 is escape, symbol 1 end-of-block.
""")
def emit_codes(rust, name):
    v = arr(codes, name)
    o.append(f"pub static {rust}: [(u8, u32); {len(v)}] = [")
    for i in range(0, len(v), 6):
        o.append("    " + " ".join(f"({a}, 0x{b:x})," for a, b in v[i:i+6]))
    o.append("];\n")
def emit_rl(rust, name):
    v = arr(codes, name)
    o.append(f"pub static {rust}: [(u16, u16); {len(v)}] = [")
    for i in range(0, len(v), 10):
        o.append("    " + " ".join(f"({a}, {b})," for a, b in v[i:i+10]))
    o.append("];\n")

o.append("/// Coefficient VLCs: index `2 * class + (side channel of an M/S pair)`.")
for i, (c, r) in enumerate([("CLASS1","CLASS1"),("CLASS1_ALT","CLASS1_ALT"),("CLASS2","CLASS2"),("CLASS2_ALT","CLASS2_ALT"),("CLASS3","CLASS3"),("CLASS3_ALT","CLASS3_ALT")]):
    emit_codes(f"COEF{i}_CODES", f"COEF_CODES_{c}")
    emit_rl(f"COEF{i}_RUNLEVEL", f"RUNLEVEL_{r}")
o.append("/// Exponent (scale factor) delta VLC; delta = symbol - 60.")
emit_codes("EXP_CODES", "SCALE_CODES")
o.append("/// High-band noise gain delta VLC; delta = symbol - 18.")
emit_codes("HGAIN_CODES", "GAIN_CODES")
o.append("/// Bark critical-band upper edges in Hz.")
o.append("pub static CRITICAL_FREQS: [u32; 25] = [\n    100, 200, 300, 400, 510, 630, 770, 920, 1080, 1270, 1480, 1720, 2000, 2320, 2700, 3150, 3700,\n    4400, 5300, 6400, 7700, 9500, 12000, 15500, 24500,\n];\n")
o.append("/// Hard-coded exponent band edges for short blocks, `[rate arm][log2(block_len / 128)]`")
o.append("/// with rate arms >= 44100, >= 32000 and >= 22050 Hz. `None` falls back to the computed layout.")
names = [["HI_128","HI_256","HI_512"],["MID_128","MID_256","MID_512"],["LO_128","LO_256",None]]
o.append("pub static EXPONENT_BAND_EDGES: [[Option<&[u16]>; 3]; 3] = [")
for row in names:
    o.append("    [")
    for n in row:
        o.append(f"        Some(&{band(n)}),".replace("[","[").replace("]","]") if n else "        None,")
    o.append("    ],")
o.append("];")
open(out, "w").write("\n".join(o) + "\n")
