"""UI:: resource types (Runtime-steam-x64.dll). Each class level is its own block; derived
classes serialize their base first. External pointers are `bool present + String reference`."""
from ..crs import decoder


def ext(r):
    return r.string() if r.bool() else None


def ext_list(r):
    return [ext(r) for _ in range(r.i32())]


def strings(r, n):
    return [r.string() for _ in range(n)]


def element(r):
    # UI::ElementDescription::Serialize (0x180119390)
    with r.block():
        return {
            'rect': [r.i32() for _ in range(4)],      # x, y, w, h (0x20..0x2c)
            'anchor': [r.i32(), r.i32()],              # 0x30, 0x34
            'flags_40': r.u32(), 'flags_44': r.u32(), 'flags_3c': r.u32(),
            'int_38': r.i32(),
            'ref_48': ext(r), 'ref_50': ext(r), 'ref_58': ext(r),
            'name': r.string(),
        }


def derived(r, base, fields):
    """Read the base class, then this class's own block with `fields(r)`."""
    out = base(r)
    with r.block():
        out.update(fields(r))
    return out


def group(r):
    return derived(r, element, lambda r: {'children': ext_list(r)})


def button(r):
    return derived(r, element, lambda r: {
        'sprites': [ext(r) for _ in range(5)], 'params': [r.u32() for _ in range(4)]})


def image(r):
    return derived(r, element, lambda r: {'sprite_sheet': ext(r), 'sprite': r.string(),
                                          'int_78': r.i32(), 'int_7c': r.i32()})


def text(r):
    def fields(r):
        d = {'font': ext(r), 'string_table': ext(r), 'text': r.string(),
             'bool_84': r.bool(), 'bool_85': r.bool(), 'int_88': r.i32(), 'color': r.u32()}
        d['int_8c'] = r.i32()
        return d
    return derived(r, element, fields)


DESCRIPTIONS = {
    'GroupDescription': group,
    'ButtonDescription': button,
    'ImageDescription': image,
    'TextDescription': text,
    'CheckDescription': lambda r: derived(r, button, lambda r: {'check_sprite': ext(r)}),
    'ComboDescription': lambda r: derived(r, button, lambda r: {'refs': [ext(r) for _ in range(3)]}),
    'PageDescription': lambda r: derived(r, group, lambda r: {'page': r.i32()}),
    'RadioDescription': lambda r: derived(r, group, lambda r: {'selected': r.i32()}),
    'LabelDescription': lambda r: derived(r, text, lambda r: {
        'sprite_sheet': ext(r), 'sprite': r.string(), 'u90': r.u32(), 'u94': r.u32(),
        'margins': [r.i32() for _ in range(4)]}),
    'BorderDescription': lambda r: derived(r, element, lambda r: {
        'sprite_sheet': ext(r), 'sprites': strings(r, 9)}),
    'RibbonDescription': lambda r: derived(r, element, lambda r: {
        'vertical': r.bool(), 'bool_69': r.bool(), 'spacing': r.i32(), 'children': ext_list(r)}),
    'AnimatedImageDescription': lambda r: derived(r, image, lambda r: {
        'frames': strings(r, r.i32()), 'rate': r.i32()}),
    'ClipRegionDescription': lambda r: derived(r, element, lambda r: {
        'int_68': r.i32(), 'int_6c': r.i32(), 'content': ext(r)}),
    'ContainerDescription': lambda r: derived(r, element, lambda r: {
        'ints': [r.i32() for _ in range(3)], 'refs': [ext(r) for _ in range(3)],
        'bool_70': r.bool(), 'bool_71': r.bool()}),
    'EditDescription': lambda r: derived(r, element, lambda r: {
        'font': ext(r), 'text': r.string(), 'max_length': r.i32(), 'bool_7c': r.bool(), 'string_80': r.string()}),
    'GraphDescription': lambda r: derived(r, element, lambda r: {
        'ints': [r.i32() for _ in range(3)], 'u74': r.u32(), 'sprite_sheet': ext(r), 'sprite': r.string()}),
    'LayoutDescription': lambda r: derived(r, element, lambda r: {'items': [
        {'element': ext(r), 'strings': strings(r, 3), 'u': [r.u32(), r.u32()], 'i': [r.i32(), r.i32()],
         'flag': r.bool()} for _ in range(r.i32())]}),
    'ListDescription': lambda r: derived(r, element, lambda r: {
        'refs': [ext(r), ext(r)], 'sprite_sheet': ext(r), 'sprite': r.string(), 'u': [r.u32(), r.u32()]}),
    'NumberEditDescription': lambda r: derived(r, element, lambda r: {
        'font': ext(r), 'ints': [r.i32() for _ in range(4)], 'flag': r.bool()}),
    'NumberDescription': lambda r: derived(r, element, lambda r: {
        'font': ext(r), 'color': r.u32(), 'int_74': r.i32(), 'float_78': r.f32(), 'flag': r.bool()}),
    'SliderDescription': lambda r: derived(r, element, lambda r: {
        'ref_68': ext(r), 'sprite_sheet': ext(r), 'sprites': strings(r, 5),
        'ints': [r.i32(), r.i32()], 'range': [r.f32(), r.f32()], 'int_b0': r.i32()}),
    'SpinnerDescription': lambda r: derived(r, element, lambda r: {
        'refs': [ext(r) for _ in range(3)], 'int_80': r.i32()}),
    'TabDescription': lambda r: derived(r, element, lambda r: {
        'refs': [ext(r) for _ in range(3)], 'sprite_sheet': ext(r), 'sprites': strings(r, 3),
        'int_a0': r.i32(), 'tabs': [(ext(r), ext(r)) for _ in range(r.i32())]}),
    'ProgressDescription': lambda r: derived(r, element, lambda r: {
        'color': r.u32(), 'sprite_sheet': ext(r), 'sprites': strings(r, 6), 'ref_a8': ext(r)}),
    'ScrollDescription': lambda r: derived(r, element, lambda r: {
        'sprite_sheet': ext(r), 'sprites': strings(r, 9), 'vertical': r.bool(), 'int_bc': r.i32(),
        'u': [r.u32(), r.u32()]}),
    'TableDescription': lambda r: derived(r, element, lambda r: {
        'ints': [r.i32() for _ in range(3)],
        'cells': [{'column': r.i32(), 'row': r.i32(), 'element': ext(r)} for _ in range(r.i32())]}),
}

for _name, _fn in DESCRIPTIONS.items():
    decoder(_name)(lambda r, v, fn=_fn: fn(r))


@decoder('Dialog')
def dialog(r, v):
    # UI::Dialog::Serialize (0x180115950)
    with r.block():
        d = {'u40': r.u32(), 'ref_38': ext(r), 'ref_20': ext(r), 'string_28': r.string(), 'u30': r.u32()}
        d['named'] = [(ext(r), r.string()) for _ in range(r.i32())]
        d['children'] = ext_list(r)
        d['bindings'] = [{'u': r.u32(), 'a': ext(r), 'b': ext(r), 'name': r.string()} for _ in range(r.i32())]
        d['refs'] = ext_list(r)
        d['int_34'] = r.i32()
    return d


@decoder('StringTable')
def string_table(r, v):
    with r.block():
        return {'strings': dict((r.string(), r.string()) for _ in range(r.i32()))}


@decoder('SpriteSheet')
def sprite_sheet(r, v):
    # UI::SpriteSheet::Serialize (0x18011ffc0); sprite records via FUN_1800795a0
    with r.block():
        d = {'string_40': r.string(), 'string_38': r.string(), 'width': r.i32(), 'height': r.i32(),
             'flag': r.bool()}
        d['sprites'] = [{'name': r.string(), 'source': r.string(), 'x': r.i32(), 'y': r.i32(),
                         'i_c': r.i32(), 'i_10': r.i32(), 'i_8': r.i32(),
                         'uv': [r.f32() for _ in range(4)]} for _ in range(r.i32())]
        d['image'] = ext(r)
    return d


@decoder('Font')
def font(r, v):
    # UI::Font::Serialize (0x180119f60)
    with r.block() as blk:
        d = {'name': r.string(), 'size': r.i32(), 'string_80': r.string(), 'i70': r.i32(), 'i74': r.i32(),
             'string_88': r.string(), 'string_78': r.string(),
             'character_set': ext(r), 'image': ext(r), 'ref_68': ext(r)}
        if blk.version < 2:
            raise NotImplementedError('Font block version < 2 (u16 character map)')
        d['char_map'] = r.array('i', r.i32())          # code point -> glyph index (-1/0xffff = none)
        d['glyphs'] = [r.array('i', 4) for _ in range(r.i32())]   # a, c, b, ? (b = ink width)
        d['int_54'] = r.i32()
        d['int_58'] = r.i32()
        d['int_5c'] = r.i32()
    return d


@decoder('CharacterSet')
def character_set(r, v):
    with r.block():
        return {'characters': r.string()}
