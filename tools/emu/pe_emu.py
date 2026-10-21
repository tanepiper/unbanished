"""Run functions from the original Banished binaries under Unicorn to capture ground truth.

Loads Application-steam-x64.exe and Runtime-steam-x64.dll at their preferred bases, links the
exe's Runtime imports to the DLL's exports, stubs OS imports, and replaces the engine allocator with
a simple heap. `call()` invokes a function with the Windows x64 calling convention.

Only for local analysis: it reads the binaries from the git-ignored game-data/original folder.
Requires `.venv` with unicorn and pefile (`tools/re-setup.sh --install`).

Helpers: `malloc`, `write`, `read`, `bytes_in(data)` (copy bytes in, get the address), `u8/u32/
i32/u64`, `f32s(addr, n)`, `cstr(addr)`; `call` (rax), `call_f32` (xmm0), `call_i32` (signed);
`F32(x)` and `F64(x)` mark float arguments. Common C runtime imports (malloc, free, memset,
memcpy, strlen...) and kernel32 locks have working stand-ins in `OS_STUBS`; an import with none
stops the run with its name.
"""
import struct

import pefile
from unicorn import Uc, UC_ARCH_X86, UC_MODE_64, UC_HOOK_CODE, UC_HOOK_MEM_INVALID, UcError
from unicorn.x86_const import (
    UC_X86_REG_RAX, UC_X86_REG_RCX, UC_X86_REG_RDX, UC_X86_REG_R8, UC_X86_REG_R9, UC_X86_REG_RSP,
    UC_X86_REG_RIP, UC_X86_REG_XMM0, UC_X86_REG_XMM1, UC_X86_REG_XMM2, UC_X86_REG_XMM3,
    UC_X86_REG_GS_BASE,
)

PAGE = 0x1000
STACK_BASE, STACK_SIZE = 0x7f0000000000, 0x4000000
HEAP_BASE, HEAP_SIZE = 0x7e0000000000, 0x80000000
STUB_BASE = 0x7d0000000000
TEB_BASE = 0x7c0000000000
RETURN_TRAP = STUB_BASE + 0xff000

INT_REGS = [UC_X86_REG_RCX, UC_X86_REG_RDX, UC_X86_REG_R8, UC_X86_REG_R9]
XMM_REGS = [UC_X86_REG_XMM0, UC_X86_REG_XMM1, UC_X86_REG_XMM2, UC_X86_REG_XMM3]


def align(v, a=PAGE):
    return (v + a - 1) & ~(a - 1)


class F32(float):
    """Marks a float argument (passed in an XMM register)."""


class F64(float):
    """Marks a double argument (passed in an XMM register)."""


class Emu:
    def __init__(self, exe_path, dll_path):
        self.uc = Uc(UC_ARCH_X86, UC_MODE_64)
        self.stubs = {}       # address -> (name, handler)
        self.next_stub = STUB_BASE
        self.heap_top = HEAP_BASE
        self.allocs = {}
        self.uc.mem_map(STACK_BASE, STACK_SIZE)
        self.uc.mem_map(HEAP_BASE, HEAP_SIZE)
        self.uc.mem_map(STUB_BASE, 0x100000)
        self.uc.mem_map(TEB_BASE, 0x10000)
        self.uc.reg_write(UC_X86_REG_GS_BASE, TEB_BASE)
        self.uc.mem_write(TEB_BASE + 0x30, struct.pack('<Q', TEB_BASE))
        self.uc.mem_write(TEB_BASE + 0x08, struct.pack('<Q', STACK_BASE + STACK_SIZE))  # stack base
        self.uc.mem_write(TEB_BASE + 0x10, struct.pack('<Q', STACK_BASE))               # stack limit
        self.dll = self._load(dll_path)
        self.exe = self._load(exe_path)
        self.exports = self._exports(self.dll)
        self._link(self.dll)
        self._link(self.exe)
        # Replace the engine allocator with our heap.
        for name, fn in [
            ('?MemAlloc@MemoryAllocator@System@@SAPEAX_KH@Z', lambda a: self.malloc(a[0])),
            ('?MemFree@MemoryAllocator@System@@SAXPEAX@Z', lambda a: 0),
            ('?Allocate@MemoryAllocator@System@@QEAAPEAX_KH@Z', lambda a: self.malloc(a[1])),
            ('?Free@MemoryAllocator@System@@QEAAXPEAX@Z', lambda a: 0),
        ]:
            if name in self.exports:
                self.hook_function(self.exports[name], name, fn)
        self.uc.hook_add(UC_HOOK_CODE, self._on_stub, begin=STUB_BASE, end=STUB_BASE + 0x100000)
        self.uc.hook_add(UC_HOOK_MEM_INVALID, self._on_bad_mem)

    # ---------------------------------------------------------------- loading

    def _load(self, path):
        pe = pefile.PE(path, fast_load=True)
        pe.parse_data_directories()
        base = pe.OPTIONAL_HEADER.ImageBase
        self.uc.mem_map(base, align(pe.OPTIONAL_HEADER.SizeOfImage))
        self.uc.mem_write(base, pe.get_memory_mapped_image()[: pe.OPTIONAL_HEADER.SizeOfImage])
        return pe

    def _exports(self, pe):
        out = {}
        base = pe.OPTIONAL_HEADER.ImageBase
        for e in pe.DIRECTORY_ENTRY_EXPORT.symbols:
            addr = base + e.address
            if e.name:
                out[e.name.decode()] = addr
            out[f'#{e.ordinal}'] = addr
        return out

    def _new_stub(self, name, handler=None):
        addr = self.next_stub
        self.next_stub += 0x10
        self.uc.mem_write(addr, b'\xc3')  # ret (the hook runs before it)
        self.stubs[addr] = (name, handler)
        return addr

    def _link(self, pe):
        base = pe.OPTIONAL_HEADER.ImageBase
        for d in getattr(pe, 'DIRECTORY_ENTRY_IMPORT', []):
            dll = d.dll.decode().lower()
            for imp in d.imports:
                name = imp.name.decode() if imp.name else f'#{imp.ordinal}'
                if dll.startswith('runtime') and name in self.exports:
                    target = self.exports[name]
                else:
                    target = self._new_stub(f'{dll}!{name}', OS_STUBS.get(name))
                self.uc.mem_write(imp.address, struct.pack('<Q', target))

    # ---------------------------------------------------------------- hooks

    def hook_function(self, addr, name, handler):
        """Replace a function: jump to a stub that runs `handler(int_args)` and returns its value."""
        stub = self._new_stub(name, handler)
        # mov rax, stub; jmp rax
        self.uc.mem_write(addr, b'\x48\xb8' + struct.pack('<Q', stub) + b'\xff\xe0')

    def _on_stub(self, uc, address, size, _):
        if address == RETURN_TRAP:
            uc.emu_stop()
            return
        name, handler = self.stubs.get(address, (None, None))
        if name is None:
            return
        if handler is None:
            raise RuntimeError(f'unhandled import {name} called: add a handler to OS_STUBS, '
                               'or hook its caller with hook_function')
        args = [uc.reg_read(r) for r in INT_REGS]
        # A handler takes the four integer argument registers; one with a parameter named `emu`
        # also gets the emulator (the C runtime stand-ins use it).
        code = handler.__code__
        wants_emu = 'emu' in code.co_varnames[:code.co_argcount]
        result = handler(args, self) if wants_emu else handler(args)
        if result is not None:
            uc.reg_write(UC_X86_REG_RAX, result & 0xFFFF_FFFF_FFFF_FFFF)

    def _on_bad_mem(self, uc, access, address, size, value, _):
        rip = uc.reg_read(UC_X86_REG_RIP)
        where = f'{rip:#x}'
        for pe in (self.exe, self.dll):
            base = pe.OPTIONAL_HEADER.ImageBase
            if base <= rip < base + pe.OPTIONAL_HEADER.SizeOfImage:
                kind = 'exe' if pe is self.exe else 'dll'
                where = f'{rip:#x} ({kind}+{rip - base:#x})'
        print(f'invalid memory access {access} at {address:#x} (size {size}) from rip {where}: '
              'usually a pointer field the emulated object is missing')
        return False

    # ---------------------------------------------------------------- memory

    def malloc(self, size, alignment=16):
        self.heap_top = align(self.heap_top, alignment)
        addr = self.heap_top
        self.heap_top += max(size, 1)
        if self.heap_top > HEAP_BASE + HEAP_SIZE:
            raise MemoryError('emulated heap exhausted')
        self.uc.mem_write(addr, bytes(size))
        return addr

    def read(self, addr, n):
        return bytes(self.uc.mem_read(addr, n))

    def write(self, addr, data):
        self.uc.mem_write(addr, data)

    def u8(self, addr):
        return self.read(addr, 1)[0]

    def bytes_in(self, data):
        """Copy `data` into emulated memory and return its address."""
        addr = self.malloc(len(data) or 1)
        self.write(addr, data)
        return addr

    def cstr(self, addr, limit=4096):
        """A NUL-terminated byte string at `addr`."""
        return self.read(addr, limit).split(b'\0', 1)[0].decode('latin-1')

    def u32(self, addr):
        return struct.unpack('<I', self.read(addr, 4))[0]

    def i32(self, addr):
        return struct.unpack('<i', self.read(addr, 4))[0]

    def u64(self, addr):
        return struct.unpack('<Q', self.read(addr, 8))[0]

    def f32s(self, addr, n):
        return struct.unpack(f'<{n}f', self.read(addr, 4 * n))

    # ---------------------------------------------------------------- calls

    def call(self, addr, *args, timeout_insns=0):
        """Call `addr` with Windows x64 arguments: ints/pointers, or `F32` floats. Returns RAX."""
        sp = STACK_BASE + STACK_SIZE - 0x10000
        stack_args = []
        for i, a in enumerate(args):
            if i < 4:
                if isinstance(a, F32):
                    self.uc.reg_write(XMM_REGS[i], struct.unpack('<I', struct.pack('<f', a))[0])
                elif isinstance(a, F64):
                    self.uc.reg_write(XMM_REGS[i], struct.unpack('<Q', struct.pack('<d', a))[0])
                else:
                    self.uc.reg_write(INT_REGS[i], a & 0xFFFF_FFFF_FFFF_FFFF)
            else:
                stack_args.append(a)
        frame = 0x20 + 8 * len(stack_args)
        sp -= frame + 8
        sp &= ~0xF
        sp -= 8  # simulate the call's return address push alignment
        self.write(sp, struct.pack('<Q', RETURN_TRAP))
        for i, a in enumerate(stack_args):
            if isinstance(a, F32):
                v = struct.unpack('<I', struct.pack('<f', a))[0]
            elif isinstance(a, F64):
                v = struct.unpack('<Q', struct.pack('<d', a))[0]
            else:
                v = a
            self.write(sp + 8 + 0x20 + 8 * i, struct.pack('<Q', v & 0xFFFF_FFFF_FFFF_FFFF))
        self.uc.reg_write(UC_X86_REG_RSP, sp)
        try:
            self.uc.emu_start(addr, RETURN_TRAP + 1, count=timeout_insns)
        except UcError as e:
            rip = self.uc.reg_read(UC_X86_REG_RIP)
            raise RuntimeError(f'emulation failed at rip {rip:#x}: {e}') from e
        return self.uc.reg_read(UC_X86_REG_RAX)

    def call_i32(self, addr, *args, **kw):
        """`call`, with rax read as a signed 32-bit int."""
        v = self.call(addr, *args, **kw) & 0xFFFFFFFF
        return v - (1 << 32) if v & 0x80000000 else v

    def call_f32(self, addr, *args):
        self.call(addr, *args)
        return struct.unpack('<f', struct.pack('<I', self.uc.reg_read(UC_X86_REG_XMM0) & 0xFFFFFFFF))[0]


def _no_op(_args):
    return 0


def _memset(a, emu):
    emu.write(a[0], bytes([a[1] & 0xFF]) * a[2])
    return a[0]


def _memcpy(a, emu):
    emu.write(a[0], emu.read(a[1], a[2]))  # also memmove: the read completes before the write
    return a[0]


def _realloc(a, emu):
    new = emu.malloc(a[1])
    if a[0]:
        emu.write(new, emu.read(a[0], a[1]))  # may read past a smaller old block; harmless here
    return new


OS_STUBS = {
    'EnterCriticalSection': _no_op,
    'LeaveCriticalSection': _no_op,
    'InitializeCriticalSection': _no_op,
    'InitializeCriticalSectionAndSpinCount': lambda a: 1,
    'DeleteCriticalSection': _no_op,
    'GetLastError': _no_op,
    'SetLastError': _no_op,
    'GetCurrentThreadId': lambda a: 1,
    # The C runtime: allocation on the emulated heap (never freed; runs are short), memory and
    # string helpers.
    'malloc': lambda a, emu: emu.malloc(a[0]),
    'calloc': lambda a, emu: emu.malloc(a[0] * a[1]),
    'realloc': _realloc,
    'free': _no_op,
    'memset': _memset,
    'memcpy': _memcpy,
    'memmove': _memcpy,
    'strlen': lambda a, emu: len(emu.cstr(a[0])),
}
