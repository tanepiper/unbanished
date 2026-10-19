#!/usr/bin/env python3
"""Draw the work-loop diagram and its step icons.

Writes docs/assets/process-loop.svg (the seven steps on a grid, for the documents on GitHub)
and docs/assets/loop/<step>.svg (the icons, also used by the website's WorkLoop.vue).

    python3 tools/logo/make_process_loop.py
"""
import os
repo = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
GOLD='#d8a648'
icons={
 'read':   ('<path d="M3 7.5 12 3.5l9 4v9l-9 4-9-4z"/><path d="M3 7.5l9 4 9-4M12 11.5v9"/><path d="M7.5 5.5l9 4"/>','A package box'),
 'decompile':('<path d="M8 6 3 12l5 6M16 6l5 6-5 6M14 4l-4 16"/>','Code brackets'),
 'trace':  ('<circle cx="10.5" cy="10.5" r="6.5"/><path d="M15.5 15.5 21 21M7.5 9h6M7.5 12h4"/>','A magnifying glass over lines of text'),
 'measure':('<path d="M3.5 17a8.5 8.5 0 1 1 17 0"/><path d="M12 17l4.5-6"/><circle cx="12" cy="17" r="1.4"/><path d="M6 12.5l1.2.8M12 8.5V10M18 12.5l-1.2.8"/>','A gauge'),
 'rebuild':('<path d="M14.5 5.5a4.5 4.5 0 0 0-5.6 5.9L3.5 16.8a1.8 1.8 0 0 0 2.6 2.6l5.4-5.4a4.5 4.5 0 0 0 5.9-5.6l-2.7 2.7-2.4-.4-.4-2.4z"/>','A wrench'),
 'check':  ('<path d="M12 3l8 3v6c0 4.8-3.4 8-8 9-4.6-1-8-4.2-8-9V6z"/><path d="M8.5 12.2l2.4 2.4 4.6-5"/>','A shield with a tick'),
 'commit': ('<circle cx="12" cy="12" r="3.2"/><path d="M3 12h5.8M15.2 12H21"/><path d="M6 4.5h12M6 19.5h8"/>','A commit on a line, between notes'),
}
for k,(p,alt) in icons.items():
    open(f'{repo}/docs/assets/loop/{k}.svg','w').write(
f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="48" height="48" role="img" aria-label="{alt}" fill="none" stroke="{GOLD}" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">{p}</svg>
''')
steps=[('1','Read the data files','The .pkg packages','and their formats','read'),
 ('2','Decompile','Ghidra turns machine','code into pseudocode','decompile'),
 ('3','Trace','Understand one function;','write it in plain words','trace'),
 ('4','Measure','Run the original function','in an emulator','measure'),
 ('5','Rebuild in Rust','New code from the notes,','citing the original','rebuild'),
 ('6','Check','Tests against the emulator,','saves and screenshots','check'),
 ('7','Write down, commit','Documents and checklist,','one commit per piece','commit')]
W,H,G=222,96,28   # box size and gap
X0,Y1,Y2=26,40,40+H+56
col=lambda c: X0+c*(W+G)
pos={'1':(0,Y1),'2':(1,Y1),'3':(2,Y1),'4':(3,Y1),'5':(3,Y2),'6':(2,Y2),'7':(1,Y2)}
VW=X0*2+4*W+3*G
VH=Y2+H+64
out=[f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {VW} {VH}" width="{VW}" height="{VH}" role="img" aria-labelledby="t d" font-family="'Inter', system-ui, -apple-system, 'Segoe UI', Helvetica, Arial, sans-serif">
  <title id="t">The Un-Banished work loop</title>
  <desc id="d">Seven steps in a loop: 1 read the data files, 2 decompile with Ghidra, 3 trace one function, 4 measure by running the original function in an emulator, 5 rebuild in Rust, 6 check against the original, 7 write it down and commit. If the check fails, the work goes back to step 3.</desc>
  <defs>
    <marker id="a" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
      <path d="M0 0 L10 5 L0 10 Z" fill="#b5852f"/>
    </marker>
    <style>
      .box {{ fill: #1d1711; stroke: #b5852f; stroke-width: 1.5; }}
      .key {{ fill: #2a2016; stroke: #f4dc9a; stroke-width: 2; }}
      .n {{ fill: #f4dc9a; font-size: 13px; font-weight: 700; }}
      .l {{ fill: #efe6d2; font-size: 15px; font-weight: 600; }}
      .s {{ fill: #b3a88f; font-size: 12px; }}
      .ln {{ stroke: #b5852f; stroke-width: 2; fill: none; }}
      .back {{ fill: #c9bfa6; font-size: 12px; font-style: italic; }}
      .ic {{ fill: none; stroke: {GOLD}; stroke-width: 1.6; stroke-linecap: round; stroke-linejoin: round; }}
    </style>
''']
for k,(p,_) in icons.items():
    out.append(f'    <symbol id="i-{k}" viewBox="0 0 24 24">{p}</symbol>\n')
out.append(f'  </defs>\n  <rect x="1" y="1" width="{VW-2}" height="{VH-2}" rx="14" fill="#14100c"/>\n')
for n,title,s1,s2,ic in steps:
    c,y=pos[n]; x=col(c)
    cls='key' if n=='3' else 'box'
    out.append(f'''  <g>
    <rect class="{cls}" x="{x}" y="{y}" width="{W}" height="{H}" rx="10"/>
    <use href="#i-{ic}" class="ic" x="{x+14}" y="{y+13}" width="24" height="24"/>
    <text class="n" x="{x+48}" y="{y+30}">{n}</text>
    <text class="l" x="{x+64}" y="{y+30}">{title}</text>
    <text class="s" x="{x+16}" y="{y+60}">{s1}</text>
    <text class="s" x="{x+16}" y="{y+77}">{s2}</text>
  </g>
''')
mid1=Y1+H/2; mid2=Y2+H/2
for c in range(3):  # row 1 arrows
    out.append(f'  <path class="ln" d="M{col(c)+W+2} {mid1} H{col(c+1)-4}" marker-end="url(#a)"/>\n')
# 4 -> 5 straight down
cx4=col(3)+W/2
out.append(f'  <path class="ln" d="M{cx4} {Y1+H+2} V{Y2-4}" marker-end="url(#a)"/>\n')
for c in (3,2):  # row 2 arrows, right to left
    out.append(f'  <path class="ln" d="M{col(c)-2} {mid2} H{col(c-1)+W+4}" marker-end="url(#a)"/>\n')
# 6 -> 3 dashed straight up
cx3=col(2)+W/2
out.append(f'  <path class="ln" d="M{cx3} {Y2-2} V{Y1+H+4}" stroke-dasharray="5 4" marker-end="url(#a)"/>\n')
out.append(f'  <text class="back" x="{cx3-12}" y="{(Y1+H+Y2)/2+4}" text-anchor="end">does not match: trace again</text>\n')
out.append(f'  <text class="s" x="{X0}" y="{VH-24}">Every part of the game went through this loop.</text>\n</svg>\n')
open(f'{repo}/docs/assets/process-loop.svg','w').write(''.join(out))
