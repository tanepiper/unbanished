// Static/skinned model shader for the viewer. Vertex formats follow MeshLayout/ModelLayout and
// ModelSkinLayout: sint16x4 position (/512), sint16x4 texcoords (xy /512 diffuse, zw /8192 AO),
// unorm8x4 normal; skinned adds unorm8x4 weights and uint8x4 bone indices. Skinned vertices are
// stored in a per-mesh frame and must go through the bone palette even in bind pose.

struct Globals {
    view_proj: mat4x4<f32>,
    light_dir: vec4<f32>,
    eye: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(1) @binding(0) var diffuse_tex: texture_2d<f32>;
@group(1) @binding(1) var ao_tex: texture_2d<f32>;
@group(1) @binding(2) var samp: sampler;

struct MaterialParams {
    // xy scale, zw offset applied to the diffuse UV (OpaqueCharacterMaterial picks an atlas cell).
    uv_transform: vec4<f32>,
    rigid_transform: mat4x4<f32>,
};
@group(1) @binding(3) var<uniform> material: MaterialParams;

const MAX_BONES: u32 = 32u;
struct Skin {
    bones: array<mat4x4<f32>, MAX_BONES>,
};
@group(2) @binding(0) var<uniform> skin: Skin;

struct VsIn {
    @location(0) position: vec4<i32>,
    @location(1) texcoord: vec4<i32>,
    @location(2) normal: vec4<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) uv_ao: vec2<f32>,
    @location(2) normal: vec3<f32>,
};

fn finish(p: vec3<f32>, n: vec3<f32>, texcoord: vec4<i32>) -> VsOut {
    var out: VsOut;
    out.clip = globals.view_proj * vec4<f32>(p, 1.0);
    out.uv = vec2<f32>(texcoord.xy) / 512.0;
    out.uv_ao = vec2<f32>(texcoord.zw) / 8192.0;
    out.normal = n;
    return out;
}

@vertex
fn vs_main(v: VsIn) -> VsOut {
    let p = material.rigid_transform * vec4<f32>(vec3<f32>(v.position.xyz) / 512.0, 1.0);
    let n = material.rigid_transform * vec4<f32>(v.normal.xyz * 2.0 - 1.0, 0.0);
    return finish(p.xyz, n.xyz, v.texcoord);
}

struct SkinnedIn {
    @location(0) position: vec4<i32>,
    @location(1) texcoord: vec4<i32>,
    @location(2) normal: vec4<f32>,
    @location(3) weights: vec4<f32>,
    @location(4) bones: vec4<u32>,
};

@vertex
fn vs_skinned(v: SkinnedIn) -> VsOut {
    let m = skin.bones[v.bones.x] * v.weights.x + skin.bones[v.bones.y] * v.weights.y
          + skin.bones[v.bones.z] * v.weights.z + skin.bones[v.bones.w] * v.weights.w;
    let p = m * vec4<f32>(vec3<f32>(v.position.xyz) / 512.0, 1.0);
    let n = m * vec4<f32>(v.normal.xyz * 2.0 - 1.0, 0.0);
    return finish(p.xyz, n.xyz, v.texcoord);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let base = textureSample(diffuse_tex, samp, in.uv * material.uv_transform.xy + material.uv_transform.zw);
    let ao = textureSample(ao_tex, samp, in.uv_ao).r;
    if (base.a < 0.5) {
        discard;
    }
    let n = normalize(in.normal);
    let sun = max(dot(n, -globals.light_dir.xyz), 0.0);
    let light = (0.35 + 0.75 * sun) * ao;
    return vec4<f32>(base.rgb * light, 1.0);
}
