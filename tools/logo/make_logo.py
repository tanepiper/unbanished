"""Generate the Un-Banished logo files in docs/assets/.

Usage: python3 tools/logo/make_logo.py EBGaramond-VariableFont_wght.ttf

Writes:
  docs/assets/un-banished-icon.svg   the round emblem on its own (icons, favicon)
  docs/assets/un-banished-icon-small.svg  the emblem simplified for 16 to 48 pixels (favicon)
  docs/assets/un-banished-logo.svg   the banner: emblem, wordmark and tagline

The lettering is EB Garamond (SIL Open Font License 1.1), turned into outlines by
tools/logo/glyphs.swift (macOS Core Text), so the logo looks the same everywhere and needs
no font at view time. The font file is not stored in the repository.
"""
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
OUT = os.path.join(REPO, "docs", "assets")

# Palette: warm and earthy like a frontier town at dusk, but its own colours.
INK = "#17120d"        # background
INK_EDGE = "#2a2018"
GOLD_HI = "#f4dc9a"
GOLD = "#d8a648"
GOLD_LO = "#9c6a24"
PARCHMENT = "#e9dfc8"
MUTED = "#b3a88f"


def glyphs(font, weight, size, tracking, text):
    """Return (advance, path data) for TEXT set at SIZE, baseline at y = 0."""
    tool = os.path.join(os.environ.get("TMPDIR", "/tmp"), "unbanished-glyphs")
    src = os.path.join(HERE, "glyphs.swift")
    if not os.path.exists(tool) or os.path.getmtime(tool) < os.path.getmtime(src):
        subprocess.run(["swiftc", "-O", src, "-o", tool], check=True)
    out = subprocess.run(
        [tool, font, str(weight), str(size), str(tracking), text],
        check=True, capture_output=True, text=True,
    ).stdout.split("\n", 1)
    return float(out[0]), out[1].strip()


def emblem(cx, cy, r, ids):
    """The round emblem: a timber house with a steep roof, a pine and a low sun at dusk,
    inside a double gold ring. IDS prefixes the gradient and clip ids so two emblems can
    share one file."""
    s = r / 100.0  # the drawing below is on a 200 x 200 grid centred on (0, 0)

    def P(x, y):
        return f"{cx + x * s:.2f} {cy + y * s:.2f}"

    def path(points):
        return "M" + " L".join(P(x, y) for x, y in points) + " Z"

    house_wall = [(-52, 18), (-52, 52), (14, 52), (14, 18)]
    house_roof = [(-62, 22), (-19, -30), (24, 22)]
    roof_edge = [(-62, 22), (-19, -30), (24, 22)]
    chimney = [(-2, -14), (-2, 4), (8, 4), (8, -2)]
    door = [(-30, 52), (-30, 32), (-18, 32), (-18, 52)]
    window = [(-4, 30), (-4, 40), (6, 40), (6, 30)]
    pine = [(56, -36), (78, 8), (66, 8), (84, 34), (28, 34), (46, 8), (34, 8)]
    pine2 = [(36, -6), (52, 24), (44, 24), (56, 44), (16, 44), (28, 24), (20, 24)]
    trunk = [(53, 34), (53, 50), (59, 50), (59, 34)]
    smoke = (f"M{P(3, -18)} C{P(-6, -30)} {P(12, -38)} {P(2, -50)} "
             f"C{P(-6, -60)} {P(8, -68)} {P(0, -78)}")
    hill_back = (f"M{P(-100, 20)} C{P(-60, 2)} {P(-20, 14)} {P(10, 8)} "
                 f"C{P(40, 2)} {P(70, 6)} {P(100, 0)} L{P(100, 100)} L{P(-100, 100)} Z")
    hill_front = (f"M{P(-100, 44)} C{P(-50, 34)} {P(0, 50)} {P(40, 44)} "
                  f"C{P(70, 40)} {P(90, 44)} {P(100, 42)} L{P(100, 100)} L{P(-100, 100)} Z")
    furrows = "".join(
        f'<path d="M{P(-96 + i * 2, 62 + i * 9)} C{P(-40, 56 + i * 9)} {P(30, 66 + i * 9)} '
        f'{P(96, 58 + i * 9)}" fill="none" stroke="#1f2a17" stroke-width="{2.2 * s:.2f}" '
        f'stroke-linecap="round" opacity="0.8"/>'
        for i in range(4)
    )
    sw = f"{3 * s:.2f}"
    return f"""
  <g>
    <defs>
      <linearGradient id="{ids}sky" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0" stop-color="#1e2c3a"/>
        <stop offset="0.55" stop-color="#4b3b33"/>
        <stop offset="1" stop-color="#a8642a"/>
      </linearGradient>
      <radialGradient id="{ids}sun" cx="0.5" cy="0.5" r="0.5">
        <stop offset="0" stop-color="#ffe7a6"/>
        <stop offset="0.6" stop-color="#f3b356"/>
        <stop offset="1" stop-color="#e08a32" stop-opacity="0"/>
      </radialGradient>
      <linearGradient id="{ids}ring" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0" stop-color="{GOLD_HI}"/>
        <stop offset="0.5" stop-color="{GOLD}"/>
        <stop offset="1" stop-color="{GOLD_LO}"/>
      </linearGradient>
      <clipPath id="{ids}disc"><circle cx="{cx}" cy="{cy}" r="{86 * s:.2f}"/></clipPath>
    </defs>
    <circle cx="{cx}" cy="{cy}" r="{98 * s:.2f}" fill="{INK}"/>
    <g clip-path="url(#{ids}disc)">
      <rect x="{cx - 100 * s:.2f}" y="{cy - 100 * s:.2f}" width="{200 * s:.2f}" height="{200 * s:.2f}" fill="url(#{ids}sky)"/>
      <circle cx="{cx + 34 * s:.2f}" cy="{cy + 6 * s:.2f}" r="{46 * s:.2f}" fill="url(#{ids}sun)"/>
      <circle cx="{cx + 34 * s:.2f}" cy="{cy + 6 * s:.2f}" r="{20 * s:.2f}" fill="#ffd98a"/>
      <path d="{hill_back}" fill="#36452a"/>
      <path d="{smoke}" fill="none" stroke="#d9cfbd" stroke-width="{3.5 * s:.2f}" stroke-linecap="round" opacity="0.55"/>
      <path d="{path(trunk)}" fill="#3a2717"/>
      <path d="{path(pine)}" fill="#22331c"/>
      <path d="{path(pine2)}" fill="#2b3f22"/>
      <path d="{path(chimney)}" fill="#3a2717"/>
      <path d="{path(house_wall)}" fill="#6e4528"/>
      <g stroke="#4a2e19" stroke-width="{2 * s:.2f}">
        <line x1="{cx - 52 * s:.2f}" y1="{cy + 29 * s:.2f}" x2="{cx + 14 * s:.2f}" y2="{cy + 29 * s:.2f}"/>
        <line x1="{cx - 52 * s:.2f}" y1="{cy + 41 * s:.2f}" x2="{cx + 14 * s:.2f}" y2="{cy + 41 * s:.2f}"/>
      </g>
      <path d="{path(house_roof)}" fill="#3b2a1d"/>
      <path d="M{P(*roof_edge[0])} L{P(*roof_edge[1])} L{P(*roof_edge[2])}" fill="none" stroke="url(#{ids}ring)" stroke-width="{sw}" stroke-linejoin="round" stroke-linecap="round"/>
      <path d="{path(door)}" fill="#241710"/>
      <path d="{path(window)}" fill="#ffd27a"/>
      <path d="{hill_front}" fill="#2a3820"/>
      {furrows}
    </g>
    <circle cx="{cx}" cy="{cy}" r="{87 * s:.2f}" fill="none" stroke="url(#{ids}ring)" stroke-width="{5 * s:.2f}"/>
    <circle cx="{cx}" cy="{cy}" r="{95 * s:.2f}" fill="none" stroke="url(#{ids}ring)" stroke-width="{2 * s:.2f}" opacity="0.7"/>
  </g>"""


def icon():
    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" width="256" height="256" role="img" aria-labelledby="t">
  <title id="t">Un-Banished</title>{emblem(128, 128, 126, "i")}
</svg>
"""


def icon_small():
    """The emblem reduced to the house and the ring, for 16 to 48 pixel icons."""
    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64" role="img" aria-labelledby="t">
  <title id="t">Un-Banished</title>
  <defs>
    <linearGradient id="g" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="{GOLD_HI}"/>
      <stop offset="1" stop-color="{GOLD}"/>
    </linearGradient>
  </defs>
  <circle cx="32" cy="32" r="31" fill="{INK}"/>
  <circle cx="32" cy="32" r="28" fill="#26323e" stroke="url(#g)" stroke-width="3.5"/>
  <path d="M8 40 Q32 34 56 40 L56 46 Q32 58 8 46 Z" fill="#33442a"/>
  <path d="M13 33 L30 15 L47 33 Z" fill="url(#g)"/>
  <rect x="18" y="32" width="24" height="12" fill="#8a5a33"/>
  <rect x="27" y="36" width="6" height="8" fill="{INK}"/>
  <rect x="36" y="35" width="4" height="4" fill="#ffd27a"/>
</svg>
"""


def torn_edge(y, width, amp, step, seed):
    """A rough edge along y, like torn paper, as "x y" points from left to right."""
    import random
    rnd = random.Random(seed)
    pts = []
    x = 0.0
    while x < width:
        pts.append((x, y + rnd.uniform(-amp, amp)))
        x += rnd.uniform(step * 0.5, step * 1.5)
    pts.append((width, y + rnd.uniform(-amp, amp)))
    return " L".join(f"{px:.1f} {py:.1f}" for px, py in pts)


def banner(font, font_italic):
    h = 260
    word_w, word = glyphs(font, 600, 92, 1.5, "Un-Banished")
    tag_w, tag = glyphs(font_italic, 450, 25, 0.3, "A from-scratch engine for a classic city builder")
    sub_w, sub = glyphs(font, 500, 17, 1.6, "WINDOWS · MACOS · LINUX · BROWSER")
    x0 = 268
    rule_w = max(word_w, tag_w) + 8
    w = int(x0 + rule_w + 44)
    top = torn_edge(14, w, 4, 18, 7)
    bottom = torn_edge(h - 14, w, 4, 18, 11)
    # The paper: the top edge left to right, then the bottom edge right to left.
    paper = "M" + top + " L" + " L".join(reversed(bottom.split(" L"))) + " Z"
    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}" role="img" aria-labelledby="t d">
  <title id="t">Un-Banished</title>
  <desc id="d">A round emblem with a timber house, a pine and a low sun, beside the name Un-Banished in gold.</desc>
  <defs>
    <linearGradient id="gold" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="{GOLD_HI}"/>
      <stop offset="0.45" stop-color="{GOLD}"/>
      <stop offset="0.55" stop-color="#c28d38"/>
      <stop offset="1" stop-color="{GOLD_LO}"/>
    </linearGradient>
    <linearGradient id="paper" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#1d1711"/>
      <stop offset="1" stop-color="{INK}"/>
    </linearGradient>
    <linearGradient id="rule" x1="0" y1="0" x2="1" y2="0">
      <stop offset="0" stop-color="{GOLD}" stop-opacity="0.9"/>
      <stop offset="1" stop-color="{GOLD}" stop-opacity="0"/>
    </linearGradient>
  </defs>
  <path d="{paper}" fill="url(#paper)" stroke="{INK_EDGE}" stroke-width="1.5"/>{emblem(138, h / 2, 104, "b")}
  <g transform="translate({x0} 132)">
    <path d="{word}" transform="translate(2.5 3)" fill="#000" opacity="0.55"/>
    <path d="{word}" fill="url(#gold)" stroke="#5a3d16" stroke-width="1.2" paint-order="stroke"/>
  </g>
  <rect x="{x0 + 2}" y="150" width="{rule_w:.0f}" height="1.6" fill="url(#rule)"/>
  <g transform="translate({x0 + 3} 184)"><path d="{tag}" fill="{PARCHMENT}"/></g>
  <g transform="translate({x0 + 4} 216)"><path d="{sub}" fill="{MUTED}"/></g>
</svg>
"""


def main():
    font = sys.argv[1]
    italic = sys.argv[2] if len(sys.argv) > 2 else font.replace("VariableFont", "Italic-VariableFont")
    with open(os.path.join(OUT, "un-banished-icon.svg"), "w") as f:
        f.write(icon())
    with open(os.path.join(OUT, "un-banished-icon-small.svg"), "w") as f:
        f.write(icon_small())
    with open(os.path.join(OUT, "un-banished-logo.svg"), "w") as f:
        f.write(banner(font, italic))
    print("wrote the icon, small icon and banner in docs/assets/")


if __name__ == "__main__":
    main()
