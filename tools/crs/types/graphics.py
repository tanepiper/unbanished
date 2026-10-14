"""Graphics:: resource types (Runtime-steam-x64.dll)."""
from ..crs import decoder


def external_ref(r):
    # Object::ExternalPtr serialization (e.g. FUN_18005dbc0): bool present, then "Path\File.rsc:object".
    return r.string() if r.bool() else None


def _buffer(r):
    # Video::{Index,Vertex}Buffer -> backend *BufferData::Serialize: u8 flag, u32 byte length, bytes.
    flag = r.u8()
    return {'flag': flag, 'data': r.bytes(r.u32())}


@decoder('GraphicsMesh')
def graphics_mesh(r, file_version):
    # Graphics::GraphicsMesh::Serialize (0x180101c40)
    with r.block() as blk:
        mesh = {
            'source': r.string(),
            'unused_min': r.array('f', 3),
            'unused_max': r.array('f', 3),
            'unknown_int': r.i32(),
            'unknown_floats': (r.f32(), r.f32()),
        }
        parts = []
        for _ in range(r.i32()):
            part = {'lod_distance': r.f32(), 'name': r.string(), 'ao_texture': external_ref(r)}
            if blk.version == 0:
                raise NotImplementedError('GraphicsMesh block version 0 (FUN_1800a55c0)')
            parts.append(part)
        mesh['parts'] = parts
        mesh['material_instances'] = [[external_ref(r) for _ in range(r.i32())] for _ in range(r.i32())]
        mesh['bones'] = [{
            'name': r.string(),
            'parent': r.i32(),
            'rotation': r.array('f', 4),
            'translation': r.array('f', 3),
            'inv_rotation': r.array('f', 4),
            'inv_translation': r.array('f', 3),
        } for _ in range(r.i32())]
        mesh['bounds_min'] = r.array('f', 3)
        mesh['bounds_max'] = r.array('f', 3)
        if blk.version > 0:
            # PlatformSerialize (0x1800a5830): DX9 sub-block, then DX11 sub-block.
            with r.block():
                r.pos = r.end  # DX9 layout not needed by the port
            with r.block():
                r.i32()
                for part in parts:
                    part['ranges'] = [dict(zip(
                        ('start_vertex', 'vertex_count', 'start_index', 'index_count', 'base_vertex', 'bone'),
                        r.array('i', 6))) for _ in range(r.i32())]
                    # Static geometry uses ModelLayout (24-byte stride), skinned uses
                    # ModelSkinLayout (32-byte stride). Positions are int16 / 512.
                    part['static_indices'] = _buffer(r)
                    part['static_vertices'] = _buffer(r)
                    part['skinned_indices'] = _buffer(r)
                    part['skinned_vertices'] = _buffer(r)
                    part['unknown_tail'] = r.i32()
    return mesh


@decoder('VertexLayout')
def vertex_layout(r, file_version):
    # Video::VertexLayout::Serialize (0x1800f86c0); format enum per VideoDX11 DXGI table.
    with r.block():
        elements = [dict(zip(('semantic', 'stream', 'offset', 'usage', 'usage_index', 'format'),
                             [r.string()] + [r.i32() for _ in range(5)])) for _ in range(r.i32())]
        signature = r.bytes(r.i32())
    for e in elements:
        e['format'] = VERTEX_FORMATS.get(e['format'], e['format'])
    return {'elements': elements, 'dxbc_signature': signature}


VERTEX_FORMATS = {
    0: 'float4', 1: 'float3', 2: 'float2', 3: 'float1',
    4: 'sint16x4', 5: 'sint16x2', 6: 'bgra8unorm', 7: 'uint8x4', 8: 'unorm8x4',
}


@decoder('MaterialInstance')
def material_instance(r, file_version):
    # Graphics::MaterialInstance::Serialize (0x180106300)
    with r.block():
        material = external_ref(r)
        textures = {r.string(): external_ref(r) for _ in range(r.i32())}
        vectors = {r.string(): r.array('f', 4) for _ in range(r.i32())}
    return {'material': material, 'textures': textures, 'vectors': vectors}
