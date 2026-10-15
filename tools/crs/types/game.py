"""Gameplay storage search descriptions; serializers 140132940 and 140055e90."""
from ..crs import decoder
from .ui import ext


@decoder('StorageLocationDescription')
def storage_location(r, header):
    with r.block():
        return {'accepted_flags': r.u32()}


@decoder('RadiusDescription')
def radius(r, header):
    with r.block():
        return {'radius': r.i32(), 'display': ext(r)}
