"""Reader for Shining Rock compiled resources (.crs).

Container (Object::External::Save, Runtime 0x1800be4a0):

  block {                               IO::BinaryWriter ctor (0x1800a9980)
    u32 0xaaaaaaaa
    u32 size                            bytes after the version field
    u32 version
    ...payload
    u32 0xbbbbbbbb                      terminator, counted in size
  }

  outer block payload:
    u32 0x5412a91f                      magic
    u32 typeId                          CRC32(UTF-16LE class name)
    u32 fileVersion                     Class::GetFileVersion()
    u32 depCount
    depCount x { String path, u64 FILETIME }
    ...object, one nested block per class level of Serialize(Stream&)

  String (System::String::Serialize, 0x1800cc1a0):
    u32 n | 0x80000000  -> n bytes of 8-bit text including NUL
    u32 n               -> n UTF-16 code units including NUL

Decoders for individual classes live in tools/crs/types/ and register with @decoder.
"""
import struct
import zlib

BLOCK_MARKER = 0xAAAAAAAA
BLOCK_END = 0xBBBBBBBB
MAGIC = 0x5412A91F

DECODERS = {}


def type_id(name):
    return zlib.crc32(name.encode('utf-16-le'))


def decoder(class_name):
    """Register a function(reader, version) -> dict for objects of class_name."""
    def wrap(fn):
        DECODERS[type_id(class_name)] = (class_name, fn)
        return fn
    return wrap


class FormatError(ValueError):
    pass


class Reader:
    def __init__(self, data, pos=0, end=None):
        self.data = data
        self.pos = pos
        self.end = len(data) if end is None else end

    def _take(self, n):
        if self.pos + n > self.end:
            raise FormatError(f'read of {n} bytes at {self.pos:#x} overruns limit {self.end:#x}')
        b = self.data[self.pos:self.pos + n]
        self.pos += n
        return b

    def _unpack(self, fmt):
        size = struct.calcsize(fmt)
        return struct.unpack('<' + fmt, self._take(size))

    def u8(self): return self._unpack('B')[0]
    def i8(self): return self._unpack('b')[0]
    def bool(self): return self._unpack('B')[0] != 0
    def u16(self): return self._unpack('H')[0]
    def i16(self): return self._unpack('h')[0]
    def u32(self): return self._unpack('I')[0]
    def i32(self): return self._unpack('i')[0]
    def u64(self): return self._unpack('Q')[0]
    def i64(self): return self._unpack('q')[0]
    def f32(self): return self._unpack('f')[0]

    def array(self, fmt, count):
        return list(self._unpack(f'{count}{fmt}')) if count else []

    def bytes(self, n):
        return self._take(n)

    def string(self):
        n = self.u32()
        if n & 0x80000000:
            raw = self._take(n & 0x7FFFFFFF)
            return raw.split(b'\0', 1)[0].decode('latin-1')
        raw = self._take(n * 2)
        return raw.decode('utf-16-le').split('\0', 1)[0]

    def block(self):
        """Enter a nested block. Use as a context manager; checks exact consumption on exit."""
        return _Block(self)

    def remaining(self):
        return self.end - self.pos


class _Block:
    def __init__(self, reader):
        self.r = reader

    def __enter__(self):
        r = self.r
        start = r.pos
        if r.u32() != BLOCK_MARKER:
            raise FormatError(f'expected block marker at {start:#x}')
        size, self.version = r.u32(), r.u32()
        self.outer_end = r.end
        self.end = r.pos + size
        if size < 4 or self.end > r.end:
            raise FormatError(f'block at {start:#x} (size {size}) overruns parent')
        if int.from_bytes(r.data[self.end - 4:self.end], 'little') != BLOCK_END:
            raise FormatError(f'block at {start:#x} lacks terminator')
        r.end = self.end - 4
        return self

    def __exit__(self, exc_type, exc, tb):
        r = self.r
        if exc_type is None and r.pos != self.end - 4:
            raise FormatError(f'block ending {self.end:#x} has {self.end - 4 - r.pos} unread bytes')
        r.pos, r.end = self.end, self.outer_end
        return False


def read_resource(data, decode=True):
    """Parse a .crs file. Returns dict with header fields and, if a decoder exists, 'object'."""
    r = Reader(data)
    with r.block() as outer:
        if r.u32() != MAGIC:
            raise FormatError('missing resource magic')
        tid, file_version, ndeps = r.u32(), r.u32(), r.u32()
        deps = [(r.string(), r.u64()) for _ in range(ndeps)]
        entry = DECODERS.get(tid)
        res = {
            'type_id': tid,
            'class': entry[0] if entry else None,
            'file_version': file_version,
            'block_version': outer.version,
            'dependencies': [d for d, _ in deps],
        }
        if decode and entry:
            res['object'] = entry[1](r, file_version)
        else:
            r.pos = r.end  # skip undecoded payload
    if r.pos != len(data):
        raise FormatError(f'{len(data) - r.pos} trailing bytes after resource')
    return res
