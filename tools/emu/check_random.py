"""Ground truth for System::Random and Utility::PerlinNoise from the original Runtime DLL."""
import sys
sys.path.insert(0, 'tools/emu')
from pe_emu import Emu, F32

emu = Emu('game-data/original/Application-steam-x64.exe', 'game-data/original/Runtime-steam-x64.dll')
X = emu.exports
rnd = emu.malloc(2504)
emu.call(X['??0Random@System@@QEAA@I@Z'], rnd, 290)
vals = [emu.call(X['?Rand@Random@System@@QEAAIXZ'], rnd) & 0xFFFFFFFF for _ in range(5)]
tol = [emu.call(X['?RandTol@Random@System@@QEAAHH@Z'], rnd, 0xffffff) & 0xFFFFFFFF for _ in range(2)]
ft = [emu.call_f32(X['?RandFloatTol@Random@System@@QEAAMXZ'], rnd) for _ in range(2)]
print('rand', vals, 'tol', [t - (1 << 32) if t >> 31 else t for t in tol], 'floattol', ft)
pn = emu.malloc(16)
emu.call(X['??0PerlinNoise@Utility@@QEAA@MMHH@Z'], pn, F32(0.5), F32(5.0), 12345, -6789)
for (x, y) in [(0.0, 0.0), (1.25, 7.5), (100.75, 33.1), (511.5, 255.25)]:
    print('perlin', x, y, repr(emu.call_f32(X['?Generate@PerlinNoise@Utility@@QEBAMMM@Z'], pn, F32(x), F32(y))))
