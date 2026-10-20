"""Record every System::Random draw an emulated original function makes, in order.

Attach to an `Emu` before calling original code:

    from rng_trace import RngTrace
    trace = RngTrace(emu)            # hooks the Runtime's Random methods
    emu.call(0x14010fa30, ai)        # ... original code ...
    trace.write('out/loiter.rng')    # `kind<TAB>caller` lines

Only outermost draws are recorded (RandFloat calling Rand() logs once), with the calling
function named from tools/triage (names.tsv, class vtable roles) plus the call-site offset.
Compare with the port's `unbanished_game::random::trace` output using tools/triage/rngdiff.py.
"""
import bisect
import os
import struct
import sys

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_RDX, UC_X86_REG_R8, UC_X86_REG_RSP

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
sys.path.insert(0, os.path.join(ROOT, 'tools', 'triage'))

# Mangled Runtime exports -> trace kind (argument registers after `this` in rcx).
METHODS = {
    '?Rand@Random@System@@QEAAIXZ': ('Rand', 0),
    '?Rand@Random@System@@QEAAII@Z': ('Rand({})', 1),
    '?Rand@Random@System@@QEAAHHH@Z': ('Rand({},{})', 2),
    '?RandTol@Random@System@@QEAAHH@Z': ('RandTol({})', 1),
    '?RandFloat@Random@System@@QEAAMXZ': ('RandFloat', 0),
    '?RandFloatTol@Random@System@@QEAAMXZ': ('RandFloatTol', 0),
}


def _signed(v):
    v &= 0xffffffff
    return v - (1 << 32) if v & 0x80000000 else v


class RngTrace:
    def __init__(self, emu, label_callers=True):
        self.emu = emu
        self.log = []
        addrs = {emu.exports[m]: kind for m, kind in METHODS.items() if m in emu.exports}
        self.lo, self.hi = min(addrs), max(addrs) + 0x100
        self._functions = None
        self._index = None
        if label_callers:
            self._load_functions()
        for addr, (fmt, nargs) in addrs.items():
            emu.uc.hook_add(UC_HOOK_CODE, self._make_hook(fmt, nargs), begin=addr, end=addr)

    def _load_functions(self):
        starts = {}
        for binary in ('Application-steam-x64.exe', 'Runtime-steam-x64.dll'):
            path = os.path.join(ROOT, 'analysis', 'export', binary, 'functions.tsv')
            if os.path.exists(path):
                for line in open(path):
                    cols = line.split('\t')
                    starts[int(cols[0], 16)] = cols[0]
        self._functions = sorted(starts)
        self._names = starts
        try:
            from ghidra_index import load_names
            self._labels = load_names()
        except Exception:  # noqa: BLE001 - labels are optional
            self._labels = {}

    def caller(self, ret):
        if not self._functions:
            return f'{ret:#x}'
        i = bisect.bisect_right(self._functions, ret) - 1
        start = self._functions[i]
        name = self._labels.get(f'{start:x}', f'FUN_{start:x}')
        return f'{name}+{ret - start:#x}'

    def _make_hook(self, fmt, nargs):
        def hook(uc, address, size, _):
            ret = struct.unpack('<Q', bytes(uc.mem_read(uc.reg_read(UC_X86_REG_RSP), 8)))[0]
            if self.lo <= ret < self.hi:
                return  # nested inside another Random method
            args = [_signed(uc.reg_read(r)) for r in (UC_X86_REG_RDX, UC_X86_REG_R8)[:nargs]]
            if fmt == 'Rand({})':
                args = [a & 0xffffffff for a in args]
            self.log.append((fmt.format(*args), self.caller(ret)))
        return hook

    def clear(self):
        self.log.clear()

    def write(self, path):
        os.makedirs(os.path.dirname(os.path.abspath(path)), exist_ok=True)
        with open(path, 'w') as f:
            f.writelines(f'{k}\t{c}\n' for k, c in self.log)
