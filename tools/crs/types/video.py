"""Video:: resource types (Runtime-steam-x64.dll)."""
from ..crs import decoder

# Video::ImageBuffer::Format -> DXGI format, from the VideoDX11 texture-creation table.
IMAGE_FORMATS = {
    0: 'BC1',       # DXGI 0x47
    1: 'BC3',       # DXGI 0x4d
    2: 'BC3',       # DXGI 0x4d (second BC3 variant, purpose unknown)
    3: 'A8',        # DXGI 0x41
    4: 'R8',        # DXGI 0x3d
    5: 'B8G8R8A8',  # DXGI 0x57
    6: 'R32F',      # DXGI 0x29
    7: 'R24G8',     # DXGI 0x2c (depth)
}


@decoder('ImageBuffer')
def image_buffer(r, file_version):
    # Video::ImageBuffer::Serialize (0x1800f4f40); field order matches ImageBuffer::Create.
    with r.block():
        img = {
            'width': r.i32(),
            'height': r.i32(),
            'image_type': r.u32(),       # 0 = 2D, otherwise cube
            'usage': r.u32(),
            'format': IMAGE_FORMATS.get(r.u32(), 'unknown'),
            'generate_mips': r.u32(),
            'address': (r.u32(), r.u32(), r.u32()),
            'filter': r.u32(),
            'source': r.string(),
        }
        # PlatformSerialize (0x1800f5980): usage 4/5 carry no pixel data.
        img['levels'] = []
        if img['usage'] - 4 & 0xFFFFFFFF >= 2:
            for _ in range(r.i32()):
                img['levels'].append(r.bytes(r.i32()))
    return img
