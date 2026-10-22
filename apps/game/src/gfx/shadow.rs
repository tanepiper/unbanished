//! The sun's shadow map (`Graphics::SunLight`), ported from Runtime-steam-x64.dll. See
//! docs/SHADOWS.md.
//!
//! Each frame the sun fits an orthographic camera around the part of the ground the main camera
//! sees (`SunLight::GetSunCamera` 0x1801098a0), renders the casters' depth into a square depth
//! buffer (`SunLight::DrawShadowMap` 0x180109450), and the lit pixel shaders compare against it
//! through `lc[10..13]`, the shadow projection set by `SunLight::Draw` 0x180109040.
//!
//! The Options dialog's shadow resolution sets the map size and its shadow quality the number
//! of taps (`render::VideoSettings`). Deviation: casters are not culled by the main camera's frustum as the original's sun draw group is (`BeginDrawGroup`
//! with the main camera as cull camera), so objects just off screen still cast into view. The
//! planar reflection samples the main view's shadow map rather than one fitted to its own camera.

use glam::{Mat4, Vec3, Vec4};

/// `SunLight::SunLight` (0x18010aa80) creates a 2048² depth ImageBuffer (format 7, flag 0x40
/// "shadow map"); shadow maps are shifted right by `Video::Configuration +0x2c` (the
/// "ShadowScalar" setting, Options' shadow resolution High/Medium/Low = 0/1/2), default 1.
pub fn shadow_size(scalar: i32) -> u32 {
    2048 >> scalar.clamp(0, 2)
}

pub const SHADOW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// The main camera as `GetSunCamera` reads it from the renderer's first draw group: the
/// frustum's near-plane extents and the camera's axes (`+0x6c` rows: right, forward, up) and
/// position (`+0x90`).
#[derive(Clone, Copy, Debug)]
pub struct MainCamera {
    pub eye: Vec3,
    pub right: Vec3,
    pub forward: Vec3,
    pub up: Vec3,
    /// Horizontal and vertical field of view in degrees, and the near distance
    /// (`Volume::Frustum::Frustum(near, far, hfov, vfov)` 0x1800fca70).
    pub hfov: f32,
    pub vfov: f32,
    pub near: f32,
}

/// The sun camera (`Graphics::Camera` built by `GetSunCamera`): axes from
/// `Math::Transform3::FromLookAt(0, sun direction)`, a texel-snapped position, and an
/// orthographic frustum of half-width `radius`, near 1, far 256.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SunCamera {
    pub right: Vec3,
    pub forward: Vec3,
    pub up: Vec3,
    pub position: Vec3,
    pub radius: f32,
}

pub const SUN_NEAR: f32 = 1.0;
pub const SUN_FAR: f32 = 256.0;

fn normalize(v: Vec3) -> Vec3 {
    let l = v.length();
    if l == 0.0 {
        v
    } else {
        v * (1.0 / l)
    }
}

/// `Volume::Frustum::GetCorners` (0x1800fc0e0), the four near-plane corners of a perspective
/// frustum in camera space (x right, y forward, z up). The extents come from the planes the
/// constructor stores (`−sin·n / cos` per side).
pub fn near_corners(cam: &MainCamera) -> [Vec3; 4] {
    // The original's degree-to-radian constant, verbatim.
    #[allow(clippy::excessive_precision)]
    const DEG: f32 = 0.0174532924;
    let (sh, ch) = (cam.hfov * 0.5 * DEG).sin_cos();
    let (sv, cv) = (cam.vfov * 0.5 * DEG).sin_cos();
    let n = cam.near;
    let left = (-sh * n) / ch;
    let right = (n * -sh) / -ch;
    // Planes +0x40 (0, sin, -cos) and +0x50 (0, sin, cos).
    let z0 = (n * -sv) / -cv;
    let z1 = (n * -sv) / cv;
    [
        Vec3::new(left, n, z0),
        Vec3::new(right, n, z0),
        Vec3::new(left, n, z1),
        Vec3::new(right, n, z1),
    ]
}

/// `Math::Transform3::FromLookAt(from, to)` (0x180065220): forward `to − from`, right
/// `forward × up` with up = z (or ∓y when looking within 0.999 of straight up/down), then
/// `right × forward`. Rows: right, forward, up.
pub fn from_look_at(from: Vec3, to: Vec3) -> [Vec3; 3] {
    let f = normalize(to - from);
    let up = if f.z > 0.999 {
        Vec3::new(0.0, -1.0, 0.0)
    } else if f.z < -0.999 {
        Vec3::new(0.0, 1.0, 0.0)
    } else {
        Vec3::Z
    };
    let r = normalize(f.cross(up));
    let u = normalize(r.cross(f));
    [r, f, u]
}

/// Port of `Graphics::SunLight::GetSunCamera` (0x1801098a0).
///
/// Each near-plane corner ray of the main camera is met with the plane z = −2. A ray that hits
/// it gives the hit point twice (if within 1000 of the corner); otherwise the corner and the
/// point 1000 further along the ray, whose z is replaced by a clamp to −2..15 of an earlier
/// point's z (the original indexes its 8-point array with the corner stride, so corner i reads
/// point component `2 + 3i`; kept). The eight points' centroid, moved 128 back towards the sun,
/// is the camera position; the frustum half-width is the largest centroid distance times
/// `sqrt(|forward.z|)`. The position is snapped to whole shadow texels along the sun axes.
pub fn sun_camera(cam: &MainCamera, light_dir: Vec3, size: u32) -> SunCamera {
    let rotate = |v: Vec3| cam.right * v.x + cam.forward * v.y + cam.up * v.z;
    let mut points = [0.0f32; 24];
    let k = -cam.eye.z - 2.0;
    for (i, c) in near_corners(cam).into_iter().enumerate() {
        let n = normalize(c);
        let far = n * 1000.0 + c;
        let dir = rotate(n);
        let t = (1.0 / dir.z) * k;
        let length = (c - far).length();
        let a = if t > 0.0 {
            cam.eye + dir * t
        } else {
            rotate(c) + cam.eye
        };
        points[6 * i..6 * i + 3].copy_from_slice(&a.to_array());
        if t > 0.0 && length > t {
            points[6 * i + 3..6 * i + 6].copy_from_slice(&(cam.eye + dir * t).to_array());
        } else {
            let b = rotate(far) + cam.eye;
            points[6 * i + 3..6 * i + 6].copy_from_slice(&b.to_array());
            points[6 * i + 5] = points[2 + 3 * i].clamp(-2.0, 15.0);
        }
    }
    let point = |j: usize| Vec3::from_slice(&points[3 * j..3 * j + 3]);
    let mut sum = Vec3::ZERO;
    for j in 0..8 {
        sum += point(j);
    }
    let centroid = sum * 0.125;
    let mut max = 0.0f32;
    for j in 0..8 {
        let d = centroid - point(j);
        let d2 = d.x * d.x + d.y * d.y + d.z * d.z;
        if d2 > max {
            max = d2;
        }
    }
    let radius = max.sqrt() * (-cam.forward.z).abs().sqrt();
    let center = centroid + light_dir * -128.0;
    let texel = (radius * 2.0) / size as f32;
    let [r, f, u] = from_look_at(Vec3::ZERO, light_dir);
    let snap = |axis: Vec3| {
        let v = axis.dot(center);
        v - v % texel
    };
    let (a, b, d) = (snap(r), snap(f), snap(u));
    SunCamera {
        right: r,
        forward: f,
        up: u,
        position: r * a + f * b + u * d,
        radius,
    }
}

impl SunCamera {
    /// World → sun camera space (x right, y forward, z up): `Transform3::Inverse`.
    fn view(&self) -> Mat4 {
        let t = self.position;
        Mat4::from_cols(
            Vec4::new(self.right.x, self.forward.x, self.up.x, 0.0),
            Vec4::new(self.right.y, self.forward.y, self.up.y, 0.0),
            Vec4::new(self.right.z, self.forward.z, self.up.z, 0.0),
            Vec4::new(
                -self.right.dot(t),
                -self.forward.dot(t),
                -self.up.dot(t),
                1.0,
            ),
        )
    }

    /// Sun camera space → D3D clip space: `Camera::SetFrustum` (0x180100e50) applies the z-up
    /// swap `(x, y, z) → (x, z, −y)` before `Volume::Frustum::Projection` (0x1800fc650), the
    /// orthographic D3D form (x, z over ±radius; depth `(y − 1) / 255`).
    fn projection(&self) -> Mat4 {
        let s = 2.0 / (self.radius - -self.radius);
        let d = SUN_FAR - SUN_NEAR;
        Mat4::from_cols(
            Vec4::new(s, 0.0, 0.0, 0.0),
            Vec4::new(0.0, 0.0, 1.0 / d, 0.0),
            Vec4::new(0.0, s, 0.0, 0.0),
            Vec4::new(0.0, 0.0, -SUN_NEAR / d, 1.0),
        )
    }

    /// The shadow lookup (`lc[10..13]` in every lit vertex shader): `SunLight::Draw` multiplies
    /// the view and projection by the D3D texture bias (u = x/2 + ½, v = −y/2 + ½, z kept).
    pub fn lookup(&self) -> Mat4 {
        let bias = Mat4::from_cols(
            Vec4::new(0.5, 0.0, 0.0, 0.0),
            Vec4::new(0.0, -0.5, 0.0, 0.0),
            Vec4::new(0.0, 0.0, 1.0, 0.0),
            Vec4::new(0.5, 0.5, 0.0, 1.0),
        );
        bias * self.projection() * self.view()
    }

    /// The depth pass's view-projection. `DrawShadowMap` draws into the viewport (1, 1,
    /// size − 2, size − 2); D3D9 rasterizes at integer pixel centres, so each texel holds the
    /// depth half a texel up-left of where it is sampled. wgpu samples at pixel centres + ½, so
    /// the geometry is moved by that half pixel to store the same depths (this is what keeps
    /// sun-facing ground from self-shadowing in the original).
    pub fn depth_view_proj(&self, size: u32) -> Mat4 {
        let px = 1.0 / (size - 2) as f32;
        Mat4::from_translation(Vec3::new(px, -px, 0.0)) * self.projection() * self.view()
    }
}

/// GPU side: the depth target, the comparison sampler the lit shaders read it through, and a
/// 1×1 stand-in bound while the map itself is the render target.
pub struct ShadowMap {
    pub size: u32,
    pub view: wgpu::TextureView,
    pub dummy: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    /// This frame's shadow lookup (`None`: no sun shadow, e.g. the minimap).
    pub lookup: Option<Mat4>,
}

impl ShadowMap {
    pub fn new(device: &wgpu::Device, size: u32) -> ShadowMap {
        let make = |label, size| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: size,
                        height: size,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: SHADOW_FORMAT,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        // Hardware shadow comparison (`ref <= stored` passes) with bilinear PCF; outside the
        // map the cleared border (depth 1) is lit.
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow compare"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        ShadowMap {
            size,
            view: make("sun shadow map", size),
            dummy: make("shadow stand-in", 1),
            sampler,
            lookup: None,
        }
    }

    /// Globals bindings 1 and 2: the shadow map and its comparison sampler.
    pub fn layout_entries() -> [wgpu::BindGroupLayoutEntry; 2] {
        [
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                count: None,
            },
        ]
    }

    /// `DrawShadowMap`: clear depth to 1 and draw into the viewport inset by one pixel.
    pub fn begin_pass<'e>(&'e self, encoder: &'e mut wgpu::CommandEncoder) -> wgpu::RenderPass<'e> {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("sun shadow map"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let inner = (self.size - 2) as f32;
        pass.set_viewport(1.0, 1.0, inner, inner, 0.0, 1.0);
        pass
    }
}

/// How a material's `rsDepth` render state rasterizes the shadow pass.
#[derive(Clone, Copy)]
pub enum DepthState {
    /// `OpaqueMaterial_rsDepth` (Opaque, Snow, InWater, Skin, Terrain): cull mode CW where the
    /// colour pass culls CCW, so casters store their back faces. Single-sided ground never
    /// reaches the map and sun-facing surfaces cannot shadow themselves.
    BackFaces,
    /// `AlphaTestMaterial_rsDepth` (AlphaTest, Foliage): the colour pass's culling, with depth
    /// bias and slope bias 1e-4.
    FrontFacesBiased,
    /// `BillboardMaterial_rsDepth`: no culling.
    TwoSided,
}

/// The faces the original's colour passes keep (they cull CCW in D3D9's screen space). With
/// the port's projections these are wgpu's back faces (clockwise under `FrontFace::Ccw`):
/// ground seen from above is culled as `Face::Back`.
const KEPT_BY_COLOUR_PASS: wgpu::Face = wgpu::Face::Back;

/// D3D9 depth bias 1e-4 in D24 units (2^-24 per step).
const ALPHA_TEST_BIAS: i32 = 1678;

/// A depth-only pipeline for the shadow pass (`*_depth` material passes with `rsDepth`: depth
/// test LessEqual and write, no colour). `fs` is the alpha-tested depth pixel shader, if any.
#[allow(clippy::too_many_arguments)]
pub fn depth_pipeline(
    device: &wgpu::Device,
    label: &str,
    layout: &wgpu::PipelineLayout,
    module: &wgpu::ShaderModule,
    vs: &str,
    fs: Option<&str>,
    buffers: &[Option<wgpu::VertexBufferLayout>],
    state: DepthState,
) -> wgpu::RenderPipeline {
    let other = |f: wgpu::Face| match f {
        wgpu::Face::Front => wgpu::Face::Back,
        wgpu::Face::Back => wgpu::Face::Front,
    };
    let (cull_mode, bias) = match state {
        DepthState::BackFaces => (Some(KEPT_BY_COLOUR_PASS), Default::default()),
        DepthState::FrontFacesBiased => (
            Some(other(KEPT_BY_COLOUR_PASS)),
            wgpu::DepthBiasState {
                constant: ALPHA_TEST_BIAS,
                slope_scale: 1e-4,
                clamp: 0.0,
            },
        ),
        DepthState::TwoSided => (None, Default::default()),
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some(vs),
            compilation_options: Default::default(),
            buffers,
        },
        fragment: fs.map(|fs| wgpu::FragmentState {
            module,
            entry_point: Some(fs),
            compilation_options: Default::default(),
            targets: &[],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: SHADOW_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: Default::default(),
            bias,
        }),
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    })
}

#[cfg(test)]
mod tests;
