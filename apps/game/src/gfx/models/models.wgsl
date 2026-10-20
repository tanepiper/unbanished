
@group(1) @binding(0) var diffuse_tex: texture_2d<f32>;
@group(1) @binding(1) var ao_tex: texture_2d<f32>;
@group(1) @binding(2) var samp: sampler;
// The range's rigid node transform, and `damage.x` 1 for a damaged material.
struct Range {
    rigid: mat4x4<f32>,
    damage: vec4<f32>,
};
@group(1) @binding(3) var<uniform> range: Range;
@group(1) @binding(4) var snow_tex: texture_2d<f32>;
@group(1) @binding(5) var damage_tex: texture_2d<f32>;

// The damaged materials (`OpaqueSnowDamageMaterial_ps` and the others): the damage texture,
// tiled 4× over the AO coordinates, replaces the diffuse by
// `min(1 − (1 − (r² + g² + r·b))², 0.75)` (none below 0).
fn damaged(base: vec3<f32>, uv_ao: vec2<f32>) -> vec3<f32> {
    let d = textureSample(damage_tex, samp, uv_ao * 4.0).rgb;
    if (range.damage.x < 0.5) {
        return base;
    }
    let l = 1.0 - (base.r * base.r + base.g * base.g + base.r * base.b);
    let w = min(1.0 - l * l, 0.75);
    if (w < 0.0) {
        return base;
    }
    return base + (d - base) * w;
}

struct VsIn {
    @location(0) position: vec4<i32>,
    @location(1) texcoord: vec4<i32>,
    @location(2) normal: vec4<f32>,
    // userData: billboard corner or foliage wind weight.
    @location(3) user: vec2<i32>,
    @location(5) m0: vec4<f32>,
    @location(6) m1: vec4<f32>,
    @location(7) m2: vec4<f32>,
    @location(8) m3: vec4<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) uv_ao: vec2<f32>,
    // x: diffuse (-n.L), y: ambient factor (n.L / 2 + 1.5), z: fog, w: leaf flag.
    @location(2) light: vec4<f32>,
    // xy: snow UV (world / 16), z: world normal z, w: world z (reflection clip).
    @location(3) snow: vec4<f32>,
    // Sun shadow map coordinate (lc[10..13]).
    @location(4) sh: vec3<f32>,
};

fn finish(world: vec3<f32>, n: vec3<f32>, texcoord: vec4<i32>, leaf: f32) -> VsOut {
    var out: VsOut;
    out.clip = g.view_proj * vec4<f32>(world, 1.0);
    out.uv = vec2<f32>(texcoord.xy) / 512.0;
    out.uv_ao = vec2<f32>(texcoord.zw) / 8192.0;
    let d = dot(n, g.light_dir.xyz);
    out.light = vec4<f32>(-d, d * 0.5 + 1.5, (out.clip.w + g.fog.x) * g.fog.y, leaf);
    out.snow = vec4<f32>(world.xy * 0.0625, n.z, world.z);
    out.sh = shadow_coord(world);
    return out;
}

@vertex
fn vs_main(v: VsIn) -> VsOut {
    let model = mat4x4<f32>(v.m0, v.m1, v.m2, v.m3) * range.rigid;
    let p = model * vec4<f32>(vec3<f32>(v.position.xyz) / 512.0, 1.0);
    let n = normalize((model * vec4<f32>(v.normal.xyz * 2.0 - 1.0, 0.0)).xyz);
    return finish(p.xyz, n, v.texcoord, 0.0);
}

// NormalSkin_charactervs: the instance's translation w carries the outfit (0..7) and selects one
// eighth of the texture atlas horizontally.
@vertex
fn vs_character(v: VsIn) -> VsOut {
    let appearance = v.m3.w;
    let model = mat4x4<f32>(v.m0, v.m1, v.m2, vec4<f32>(v.m3.xyz, 1.0)) * range.rigid;
    let p = model * vec4<f32>(vec3<f32>(v.position.xyz) / 512.0, 1.0);
    let n = normalize((model * vec4<f32>(v.normal.xyz * 2.0 - 1.0, 0.0)).xyz);
    var out = finish(p.xyz, n, v.texcoord, 0.0);
    out.uv.x = (f32(v.texcoord.x) * 0.001953125 + appearance) * 0.125;
    return out;
}

struct SkinIn {
    @location(0) position: vec4<i32>,
    @location(1) texcoord: vec4<i32>,
    @location(2) normal: vec4<f32>,
    @location(3) weights: vec4<f32>,
    @location(4) bones: vec4<u32>,
    @location(5) m0: vec4<f32>,
    @location(6) m1: vec4<f32>,
    @location(7) m2: vec4<f32>,
    @location(8) m3: vec4<f32>,
};

// NormalSkin_vs: blend four bones of the instance's palette, then the placement. The skinned
// point takes w = 1 (`skin_point`) before the placement; otherwise weight sums below 1 would
// scale each vertex's share of the world translation.
fn skinned(v: SkinIn) -> VsOut {
    let placement = mat4x4<f32>(v.m0, v.m1, v.m2, vec4<f32>(v.m3.xyz, 1.0));
    let skin_m = skin_matrix(v.weights, v.bones);
    let p = placement * skin_point(skin_m, vec3<f32>(v.position.xyz) / 512.0);
    let n = normalize((placement * skin_m * vec4<f32>(v.normal.xyz * 2.0 - 1.0, 0.0)).xyz);
    return finish(p.xyz, n, v.texcoord, 0.0);
}

@vertex
fn vs_skinned(v: SkinIn) -> VsOut {
    return skinned(v);
}

// NormalSkin_charactervs: as above, with the outfit column of the texture atlas.
@vertex
fn vs_skinned_character(v: SkinIn) -> VsOut {
    var out = skinned(v);
    out.uv.x = (f32(v.texcoord.x) * 0.001953125 + v.m3.w) * 0.125;
    return out;
}

// FoliageMaterial_vs: sway by sin(world.xy + time) weighted by userData.
@vertex
fn vs_foliage(v: VsIn) -> VsOut {
    let model = mat4x4<f32>(v.m0, v.m1, v.m2, v.m3) * range.rigid;
    var p = (model * vec4<f32>(vec3<f32>(v.position.xyz) / 512.0, 1.0)).xyz;
    let n = normalize((model * vec4<f32>(v.normal.xyz * 2.0 - 1.0, 0.0)).xyz);
    let wind = sin(p.xy + g.camera_forward.w) * vec2<f32>(v.user);
    p = vec3<f32>(p.xy + wind * 3.05175786e-06, p.z);
    return finish(p, n, v.texcoord, 0.0);
}

// BillboardMaterial_vs: vertices with a corner offset (userData) are leaf cards. They are pushed
// out along the camera's right and up axes (bent towards the view by the normal), sway with the
// wind, and are lit with a rounded crown normal.
@vertex
fn vs_billboard(v: VsIn) -> VsOut {
    let model = mat4x4<f32>(v.m0, v.m1, v.m2, v.m3) * range.rigid;
    let local = vec3<f32>(v.position.xyz) / 512.0;
    var world = (model * vec4<f32>(local, 1.0)).xyz;
    let scale = vec3<f32>(length(v.m0.xyz), length(v.m1.xyz), length(v.m2.xyz));
    let n = normalize((model * vec4<f32>(normalize(v.normal.xyz * 2.0 - 1.0), 0.0)).xyz);
    let corner = vec2<f32>(v.user);
    let leaf = select(0.0, 1.0, dot(corner, corner) >= 0.05);
    let right = g.camera_right.xyz;
    let forward = g.camera_forward.xyz;
    let up = g.camera_up.xyz;
    let nc = vec3<f32>(dot(n, right), dot(n, forward), dot(n, up)) * leaf;
    let r = normalize(right + 0.5 * nc.x * forward) * scale.x;
    let u = normalize(up + 0.5 * nc.z * forward) * scale.z;
    let wind = sin(world.xy + g.camera_forward.w) * 0.025 * dot(nc, nc) * scale.xz * leaf;
    world += r * wind.x + u * wind.y;
    let c = corner / 8192.0;
    world += r * c.x + u * c.y;
    let crown = normalize(vec3<f32>(local.x * scale.x, local.y * scale.y, local.z * scale.z + 4.0));
    let lit_normal = mix(n, crown, leaf);
    return finish(world, lit_normal, v.texcoord, leaf);
}

fn lit(base: vec3<f32>, ao: f32, in: VsOut) -> vec3<f32> {
    let light = g.ambient_color.rgb * in.light.y * ao
        + g.light_color.rgb * sun_shadow(in.sh) * clamp(in.light.x, 0.0, 1.0);
    let c = base * light;
    let fog = min(g.fog_color.w, clamp(in.light.z, 0.0, 1.0));
    return c + fog * (g.fog_color.rgb - c);
}

// OpaqueMaterial_ps.
@fragment
fn fs_opaque(in: VsOut) -> @location(0) vec4<f32> {
    let base = textureSample(diffuse_tex, samp, in.uv);
    let ao = textureSample(ao_tex, samp, in.uv_ao).r;
    let c = lit(damaged(base.rgb, in.uv_ao), ao, in);
    reflect_clip(in.snow.w);
    return vec4<f32>(c, base.a);
}

// OpaqueSnowMaterial_ps: snow where diffuse alpha × up-facing × snow amount × AO².
@fragment
fn fs_snow(in: VsOut) -> @location(0) vec4<f32> {
    let base = textureSample(diffuse_tex, samp, in.uv);
    let s = textureSample(snow_tex, samp, in.snow.xy);
    let ao = textureSample(ao_tex, samp, in.uv_ao).r;
    let m = base.a * in.snow.z * g.user.x * ao * ao;
    let c = lit(mix(damaged(base.rgb, in.uv_ao), s.rgb, m), ao, in);
    reflect_clip(in.snow.w);
    return vec4<f32>(c, 0.0);
}

@fragment
fn fs_alpha(in: VsOut) -> @location(0) vec4<f32> {
    let base = textureSample(diffuse_tex, samp, in.uv);
    let ao = textureSample(ao_tex, samp, in.uv_ao).r;
    let c = lit(damaged(base.rgb, in.uv_ao), ao, in);
    if (base.a < 0.5) {
        discard;
    }
    reflect_clip(in.snow.w);
    return vec4<f32>(c, base.a);
}

// FoliageMaterial_ps: alpha cut; snow on upward faces weighted by AO⁴.
@fragment
fn fs_foliage(in: VsOut) -> @location(0) vec4<f32> {
    let base = textureSample(diffuse_tex, samp, in.uv);
    let ao = textureSample(ao_tex, samp, in.uv_ao).r;
    let s = textureSample(snow_tex, samp, in.snow.xy);
    let a2 = ao * ao;
    let m = a2 * a2 * in.snow.z * g.user.x;
    let c = lit(mix(base.rgb, s.rgb, m), ao, in);
    if (base.a < 0.5) {
        discard;
    }
    reflect_clip(in.snow.w);
    return vec4<f32>(c, 0.0);
}

// BillboardMaterial_ps: leaves thin out with userPosition.z and turn autumn-coloured with .y.
@fragment
fn fs_billboard(in: VsOut) -> @location(0) vec4<f32> {
    let base = textureSample(diffuse_tex, samp, in.uv);
    let ao = textureSample(ao_tex, samp, in.uv_ao).r;
    let alpha = base.a - in.light.w * g.user.z;
    let lum = dot(base.rgb, base.rgb);
    let autumn = lum * vec3<f32>(0.913725495, 0.725490212, 0.278431386);
    let rgb = base.rgb + in.light.w * g.user.y * (autumn - base.rgb);
    let c = lit(rgb, ao, in);
    if (alpha < 0.5) {
        discard;
    }
    reflect_clip(in.snow.w);
    return vec4<f32>(c, 0.0);
}

// Shadow pass (`*_depth` passes). OpaqueMaterial_DepthAlpha_ps (AlphaTest and Foliage) cuts at
// 0.25, half the colour pass's threshold.
@fragment
fn fs_depth_alpha(in: VsOut) {
    if (textureSample(diffuse_tex, samp, in.uv).a < 0.25) {
        discard;
    }
}

// BillboardMaterial_Depth_ps: the colour pass's leaf thinning and 0.5 cut.
@fragment
fn fs_depth_billboard(in: VsOut) {
    if (textureSample(diffuse_tex, samp, in.uv).a - in.light.w * g.user.z < 0.5) {
        discard;
    }
}
