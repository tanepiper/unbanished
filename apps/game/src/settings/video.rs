//! The video configuration (`Video::Configuration`).

use super::*;

/// `Video::Configuration` (Runtime `0x1800f9760`, saved by `0x1800f9560`).
#[derive(Debug, Clone, PartialEq)]
pub struct VideoConfig {
    /// `+0`: the "Settings" flag.
    pub settings: bool,
    /// `+4`: renderer (the original's DX9/DX11 interface index; the port's graphics backend).
    pub interface: i32,
    /// `+8`.
    pub adapter: i32,
    /// `+0xc/+0x10`: fullscreen resolution (−1: the desktop's).
    pub width: i32,
    pub height: i32,
    /// `+0x14/+0x18`, `+0x1c/+0x20`: the window's position and size (−1: default).
    pub window_x: i32,
    pub window_y: i32,
    pub window_width: i32,
    pub window_height: i32,
    /// `+0x24`: multisample mode (0 none, n: n+1 samples).
    pub multisample: i32,
    /// `+0x28`: texture filter (−1 bilinear, 0 trilinear, n anisotropic n×).
    pub texture_filter: i32,
    /// `+0x2c`: shadow resolution (0 high, 1 medium, 2 low: the map is `2048 >> n`).
    pub shadow_scalar: i32,
    /// `+0x30`: shadow quality (0 none … 3 high).
    pub shader_detail: i32,
    /// `+0x34`: reflections (0 none, 1 terrain, 2 everything).
    pub reflection_detail: i32,
    /// `+0x38/+0x3c`: refresh rate numerator and denominator.
    pub refresh_n: i32,
    pub refresh_d: i32,
    /// `+0x40`, `+0x41`.
    pub fullscreen: bool,
    pub vsync: bool,
}

impl Default for VideoConfig {
    fn default() -> Self {
        VideoConfig {
            settings: false,
            interface: 1,
            adapter: 0,
            width: -1,
            height: -1,
            window_x: -1,
            window_y: -1,
            window_width: -1,
            window_height: -1,
            multisample: 1,
            texture_filter: 0,
            shadow_scalar: 1,
            shader_detail: 2,
            reflection_detail: 1,
            refresh_n: 60,
            refresh_d: 1,
            fullscreen: true,
            vsync: true,
        }
    }
}

impl VideoConfig {
    pub fn load(p: &Persistence) -> VideoConfig {
        let mut c = VideoConfig::default();
        let s = "Video";
        p.get_bool(s, "Settings", &mut c.settings);
        p.get_int(s, "Interface", &mut c.interface);
        p.get_int(s, "Adapter", &mut c.adapter);
        p.get_int(s, "Width", &mut c.width);
        p.get_int(s, "Height", &mut c.height);
        p.get_int(s, "WindowWidth", &mut c.window_width);
        p.get_int(s, "WindowHeight", &mut c.window_height);
        p.get_int(s, "WindowX", &mut c.window_x);
        p.get_int(s, "WindowY", &mut c.window_y);
        p.get_int(s, "RefreshN", &mut c.refresh_n);
        p.get_int(s, "RefreshD", &mut c.refresh_d);
        p.get_int(s, "Multisample", &mut c.multisample);
        p.get_bool(s, "Fullscreen", &mut c.fullscreen);
        p.get_bool(s, "VSync", &mut c.vsync);
        p.get_int(s, "TextureFilter", &mut c.texture_filter);
        p.get_int(s, "ShadowScalar", &mut c.shadow_scalar);
        p.get_int(s, "ShaderDetail", &mut c.shader_detail);
        p.get_int(s, "ReflectionDetail", &mut c.reflection_detail);
        c
    }

    pub fn save(&self, p: &mut Persistence) {
        let s = "Video";
        p.set_int(s, "Interface", self.interface);
        p.set_int(s, "Adapter", self.adapter);
        p.set_int(s, "Width", self.width);
        p.set_int(s, "Height", self.height);
        p.set_int(s, "WindowWidth", self.window_width);
        p.set_int(s, "WindowHeight", self.window_height);
        p.set_int(s, "WindowX", self.window_x);
        p.set_int(s, "WindowY", self.window_y);
        p.set_int(s, "RefreshN", self.refresh_n);
        p.set_int(s, "RefreshD", self.refresh_d);
        p.set_int(s, "Multisample", self.multisample);
        p.set_bool(s, "Fullscreen", self.fullscreen);
        p.set_bool(s, "VSync", self.vsync);
        p.set_int(s, "TextureFilter", self.texture_filter);
        p.set_int(s, "ShadowScalar", self.shadow_scalar);
        p.set_int(s, "ShaderDetail", self.shader_detail);
        p.set_int(s, "ReflectionDetail", self.reflection_detail);
        p.set_bool(s, "Settings", self.settings);
    }
}
