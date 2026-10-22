//! What the display offers: its modes, adapters and graphics interfaces.

/// `Video::Device::DisplayMode`: a resolution and its refresh rates (numerator, denominator).
#[derive(Debug, Clone, PartialEq)]
pub struct DisplayMode {
    pub width: i32,
    pub height: i32,
    pub refresh: Vec<(i32, i32)>,
}

/// `Video::Device::Adapter` (0x38 bytes): its name and display modes.
#[derive(Debug, Clone, PartialEq)]
pub struct Adapter {
    pub name: String,
    pub modes: Vec<DisplayMode>,
}

/// `Video::GraphicsInterface`: a renderer (the original lists DirectX 9 and 11; the port lists
/// the wgpu backends) and its adapters.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphicsInterface {
    pub name: String,
    pub adapters: Vec<Adapter>,
}

/// The video device's capabilities as the Options dialog lists them.
#[derive(Debug, Clone, PartialEq)]
pub struct DisplayInfo {
    pub interfaces: Vec<GraphicsInterface>,
    /// `GetMultisampleModeCount` / `IsMultisampleModeSupported`: entry `n` is `n + 1` samples.
    pub multisample: Vec<bool>,
    /// `Device +0x98`: the largest anisotropy.
    pub max_anisotropy: i32,
    /// The desktop's resolution (`GetDefaultResolution` reads `EnumDisplaySettings`).
    pub desktop: Option<(i32, i32)>,
}

impl Default for DisplayInfo {
    /// A device that offers only the current window: one interface, adapter and mode.
    fn default() -> Self {
        DisplayInfo {
            interfaces: vec![GraphicsInterface {
                name: "Default".into(),
                adapters: vec![Adapter {
                    name: "Default".into(),
                    modes: vec![DisplayMode {
                        width: 1280,
                        height: 720,
                        refresh: vec![(60, 1)],
                    }],
                }],
            }],
            multisample: vec![true],
            max_anisotropy: 0,
            desktop: None,
        }
    }
}

impl DisplayInfo {
    pub fn modes(&self, interface: i32, adapter: i32) -> &[DisplayMode] {
        self.interfaces
            .get(interface.max(0) as usize)
            .and_then(|i| i.adapters.get(adapter.max(0) as usize))
            .map_or(&[], |a| &a.modes)
    }

    /// `Video::Device::ValidateConfiguration` (`0x1800f9bc0`). An interface or adapter out of
    /// range becomes 0; an unknown resolution becomes the desktop's
    /// (`GetDefaultResolution`, else the first mode); an unknown refresh rate the first with
    /// the same whole hertz, else the first; an unsupported sample count none; a filter above
    /// the largest anisotropy that maximum.
    #[allow(clippy::too_many_arguments)]
    pub fn validate(
        &self,
        interface: &mut i32,
        adapter: &mut i32,
        width: &mut i32,
        height: &mut i32,
        refresh: &mut (i32, i32),
        multisample: &mut i32,
        filter: &mut i32,
    ) {
        if *interface < 0 || *interface as usize >= self.interfaces.len() {
            *interface = 0;
        }
        let adapters = self
            .interfaces
            .get(*interface as usize)
            .map_or(0, |i| i.adapters.len());
        if *adapter < 0 || *adapter as usize >= adapters {
            *adapter = 0;
        }
        let modes = self.modes(*interface, *adapter);
        let find = |w: i32, h: i32| modes.iter().find(|m| m.width == w && m.height == h);
        let mode = find(*width, *height)
            .or_else(|| self.desktop.and_then(|(w, h)| find(w, h)))
            .or(modes.first());
        if let Some(m) = mode {
            *width = m.width;
            *height = m.height;
            if !m.refresh.contains(refresh) {
                let hz = |(n, d): (i32, i32)| n / d.max(1);
                *refresh = m
                    .refresh
                    .iter()
                    .copied()
                    .find(|&r| hz(r) == hz(*refresh))
                    .or(m.refresh.first().copied())
                    .unwrap_or(*refresh);
            }
        }
        let supported = usize::try_from(*multisample)
            .ok()
            .and_then(|i| self.multisample.get(i))
            .copied()
            .unwrap_or(false);
        if !supported {
            *multisample = 0;
        }
        if *filter > self.max_anisotropy {
            *filter = self.max_anisotropy;
        }
    }
}
