// Terrain surface, rewritten from the disassembly of TerrainMaterial_vs / TerrainMaterial_ps
// program 2, the default shadow detail (D3D9; see docs/TERRAIN-DERIVED-GEOMETRY.md and
// docs/SHADOWS.md). World space is the terrain's own: x/y in cells (map-centred),
// z = height (saved value / 256), z up.

// `Globals`, `g` and `reflect_clip` come from scene_gfx::GLOBALS_WGSL (prepended).
@group(1) @binding(0) var grass: texture_2d<f32>;
@group(1) @binding(1) var dirt: texture_2d<f32>;
@group(1) @binding(2) var snow: texture_2d<f32>;
@group(1) @binding(3) var rock: texture_2d<f32>;
@group(1) @binding(4) var transition_map: texture_2d<f32>;
@group(1) @binding(5) var wrap: sampler;
@group(1) @binding(6) var clamp_s: sampler;

struct VsIn {
    @location(0) pos: vec4<i32>,
    @location(1) uv: vec2<i32>,
    @location(2) normal: vec4<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    // xy: detail UV (cells / 16); zw: transition-map UV.
    @location(0) t0: vec4<f32>,
    // x: diffuse, y: ambient factor, z: fog.
    @location(1) t1: vec3<f32>,
    @location(2) t2: vec3<f32>,
    // x: rock V (height / 16), y: underwater factor.
    @location(3) t3: vec2<f32>,
    @location(4) world_z: f32,
    // t4: sun shadow map coordinate (lc[10..13], programs 1-3).
    @location(5) t4: vec3<f32>,
};

@vertex
fn vs_main(v: VsIn) -> VsOut {
    var o: VsOut;
    let n = normalize(v.normal.xyz * 2.0 - 1.0);
    let d = dot(n, g.light_dir.xyz);
    o.t2 = n;
    o.t1.x = -d;
    o.t1.y = (d + 3.0) * 0.5;
    let p = vec3<f32>(f32(v.pos.x), f32(v.pos.y), f32(v.pos.z) * 0.00390625);
    o.clip = g.view_proj * vec4<f32>(p, 1.0);
    o.world_z = p.z;
    o.t4 = shadow_coord(p);
    // The original uses clip-space z of a D3D projection; view depth (w) is its equivalent here.
    o.t1.z = (o.clip.w + g.fog.x) * g.fog.y;
    o.t0 = vec4<f32>(f32(v.pos.x) * 0.0625, f32(v.pos.y) * 0.0625,
                     f32(v.uv.x) * 6.10351562e-05, f32(v.uv.y) * 6.10351562e-05);
    let z = f32(v.pos.z);
    o.t3.x = z * 0.000244140625;
    o.t3.y = (z * 0.00390625 + 0.75) * -0.666666687;
    return o;
}

@fragment
fn fs_main(i: VsOut) -> @location(0) vec4<f32> {
    let rock_top = textureSample(rock, wrap, i.t0.xy);
    let base_grass = textureSample(grass, wrap, i.t0.xy);
    let base_dirt = textureSample(dirt, wrap, i.t0.xy);
    let base_snow = textureSample(snow, wrap, i.t0.xy);
    let rock_x = textureSample(rock, wrap, vec2<f32>(i.t0.y, i.t3.x));
    let rock_y = textureSample(rock, wrap, vec2<f32>(i.t0.x, i.t3.x));
    let tr = textureSample(transition_map, clamp_s, i.t0.zw);
    let w = abs(i.t2);
    let rock_c = rock_top.rgb * w.z + (rock_x.rgb * w.x + rock_y.rgb * w.y);
    let ground = mix(base_grass.rgb, base_dirt.rgb, tr.z);
    var color = mix(ground, rock_c, tr.y);
    let up2 = i.t2.z * i.t2.z;
    let snow_amount = max(up2 * up2 * g.user.x, tr.w);
    color = mix(color, base_snow.rgb, snow_amount);
    let under = clamp(i.t3.y, 0.0, 1.0);
    color = mix(color, vec3<f32>(0.0250000004, 0.123000003, 0.189999998), under);
    let light = g.light_color.rgb * sun_shadow(i.t4) * clamp(i.t1.x, 0.0, 1.0)
        + g.ambient_color.rgb * i.t1.y * tr.x;
    let lit = color * light;
    let fog = clamp(min(g.fog_color.w, i.t1.z), 0.0, 1.0);
    // After all samples: textureSample needs uniform control flow.
    reflect_clip(i.world_z);
    return vec4<f32>(lit + fog * (g.fog_color.rgb - lit), i.t3.y);
}
