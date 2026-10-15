"""Gameplay storage search descriptions; serializers 140132940 and 140055e90."""
from ..crs import decoder
from .ui import ext


@decoder('PickingDescription')
def picking(r, header):
    # FUN_14012e1d0.
    with r.block():
        return dict(meshes=[[ext(r) for _ in range(r.u32())] for _ in range(r.u32())],
                    material=ext(r), flag_38=r.bool())


@decoder('HighlightDescription')
def highlight(r, header):
    # FUN_14004d330.
    with r.block():
        return dict(meshes=[[ext(r) for _ in range(r.u32())] for _ in range(r.u32())],
                    material=ext(r), floor_material=ext(r))


@decoder('StorageLocationDescription')
def storage_location(r, header):
    with r.block():
        return {'accepted_flags': r.u32()}


@decoder('RadiusDescription')
def radius(r, header):
    with r.block():
        return {'radius': r.i32(), 'display': ext(r)}
