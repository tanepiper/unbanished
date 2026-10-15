"""Graphics:: resource types (Runtime-steam-x64.dll)."""
from ..crs import decoder


@decoder('PickingMesh')
def picking_mesh(r, file_version):
    # PickingMesh::Serialize 1801251f0, SubMesh 18007b6f0, triangles 18007ae20.
    with r.block():
        out = dict(source=r.string(), name=r.string(), skinned=r.bool())
        meshes = []
        for _ in range(r.u32()):
            mesh = dict(name=r.string(), node=r.i32(), rotation=r.array('f', 4),
                        translation=r.array('f', 3), bounds_min=r.array('f', 3),
                        bounds_max=r.array('f', 3))
            mesh['vertices'] = [r.array('f', 3) for _ in range(r.u32())]
            mesh['normals'] = [r.array('f', 3) for _ in range(r.u32())]
            mesh['triangles'] = [dict(vertices=r.array('H', 3), normals=r.array('H', 3))
                                 for _ in range(r.u32())]
            mesh['edges'] = [r.array('H', 2) for _ in range(r.u32())]
            meshes.append(mesh)
        out['meshes'] = meshes
        return out


@decoder('AnimationGroup')
def animation_group(r, file_version):
    # Runtime AnimationGroup::Serialize 0x1800fdf30 / Animation::Serialize 0x1800fd910.
    with r.block():
        clips = []
        for _ in range(r.u32()):
            clip = dict(name=r.string(), mesh_name=r.string(), source_name=r.string(),
                        start_frame=r.i32(), end_frame=r.i32(), declared_rate=r.f32(),
                        looping=r.bool(), loop_start_frame=r.i32(), instance_count=r.i32(),
                        instanced=r.bool())
            clip['events'] = [dict(frame=r.i32(), resource=external_ref(r))
                              for _ in range(r.u32())]
            clip['frame_count'] = r.u32()
            clip['sample_rate'] = r.f32()
            clip['sample_interval'] = r.f32()
            clip['bone_count'] = r.u32()
            samples = r.u32()
            assert samples == clip['frame_count'] * clip['bone_count']
            clip['samples'] = [r.array('f', 7) for _ in range(samples)]
            clip['duration_ticks'] = r.i64()
            clips.append(clip)
        blends = [dict(source=r.string(), target=r.string(), seconds=r.f32())
                  for _ in range(r.u32())]
        default_blend = r.f32()
        matrix = r.array('f', r.u32())
        return dict(clips=clips, blends=blends, default_blend=default_blend, blend_matrix=matrix)


def external_ref(r):
    # Object::ExternalPtr serialization (e.g. FUN_18005dbc0): bool present, then "Path\File.rsc:object".
    return r.string() if r.bool() else None


def _buffer(r):
    # Video::{Index,Vertex}Buffer -> backend *BufferData::Serialize: u8 flag, u32 byte length, bytes.
    flag = r.u8()
    return {'flag': flag, 'data': r.bytes(r.u32())}


@decoder('EdgeMesh')
def edge_mesh(r, file_version):
    # Graphics::EdgeMesh::Serialize 1800a3120, ranges 180015130.
    with r.block():
        out = dict(refs=[external_ref(r) for _ in range(5)])
        out['ranges'] = [r.array('i', 6) for _ in range(r.u32())]
        out['skin_ranges'] = [r.array('i', 6) for _ in range(r.u32())]
        out['buffers'] = [_buffer(r) for _ in range(8)]
        out['bounds_min'] = r.array('f', 3)
        out['bounds_max'] = r.array('f', 3)
        return out


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
                        ('start_vertex', 'vertex_count', 'start_index', 'index_count', 'material_index', 'bone'),
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
