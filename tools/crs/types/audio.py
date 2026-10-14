"""Audio:: resource types (Runtime-steam-x64.dll)."""
import struct

from ..crs import decoder


@decoder('SoundBuffer')
def sound_buffer(r, file_version):
    # Audio::SoundBuffer::Serialize (0x1800e2cd0) + PlatformSerialize (0x1800e2e10)
    with r.block():
        s = {'looping': r.bool(), 'streamed': r.bool(), 'stream_path': r.string(), 'source': r.string(),
             'length': r.f32()}
        data_len, wma_len = r.i32(), r.i32()
        s['wma'] = r.bytes(wma_len)              # xWMA decoded-packet-cumulative-bytes table
        fmt = r.bytes(0x28)                       # WAVEFORMATEX(TENSIBLE)
        s['format'] = dict(zip(('tag', 'channels', 'rate', 'avg_bytes', 'block_align', 'bits', 'cb_size'),
                               struct.unpack_from('<HHIIHHH', fmt)))
        s['format_raw'] = fmt
        s['data'] = r.bytes(data_len)
    return s
