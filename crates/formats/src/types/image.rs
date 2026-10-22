//! Image buffers: the pixel formats, the faces and the mip levels of a texture.

use super::*;

/// `Video::ImageBuffer::Format`, mapped to DXGI by the VideoDX11 texture-creation table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    /// BC1 (DXT1) blocks.
    Bc1,
    /// BC3 (DXT5) blocks.
    Bc3,
    /// One byte of alpha per pixel.
    A8,
    /// One byte of red per pixel.
    R8,
    /// Four bytes per pixel: blue, green, red, alpha.
    Bgra8,
    /// One 32-bit float per pixel.
    R32F,
    /// 24-bit depth and 8-bit stencil per pixel (*inferred*).
    R24G8,
}

/// A texture: its size, format and levels (mip maps, and the six faces of a cube map).
#[derive(Debug, Clone)]
pub struct ImageBuffer {
    /// The width in pixels.
    pub width: u32,
    /// The height in pixels.
    pub height: u32,
    /// 0 = 2D; otherwise a cube map (levels are 6 faces x mips).
    pub image_type: u32,
    /// How the engine uses the texture.
    pub usage: u32,
    /// The pixel format.
    pub format: ImageFormat,
    /// Whether the engine makes the mip levels itself.
    pub generate_mips: bool,
    /// The address (wrap) mode on each axis.
    pub address: [u32; 3],
    /// The filter mode.
    pub filter: u32,
    /// The original image file the texture was made from.
    pub source: String,
    /// The data of each level: faces × mips, face by face.
    pub levels: Vec<Vec<u8>>,
}

impl ImageFormat {
    pub(super) fn from_raw(v: u32) -> Result<Self> {
        Ok(match v {
            0 => Self::Bc1,
            1 | 2 => Self::Bc3,
            3 => Self::A8,
            4 => Self::R8,
            5 => Self::Bgra8,
            6 => Self::R32F,
            7 => Self::R24G8,
            _ => return Err(Error::Unsupported(format!("image format {v}"))),
        })
    }

    /// Byte size of one level.
    pub fn level_size(self, w: u32, h: u32) -> usize {
        let (w, h) = (w.max(1) as usize, h.max(1) as usize);
        let blocks = w.div_ceil(4) * h.div_ceil(4);
        match self {
            Self::Bc1 => blocks * 8,
            Self::Bc3 => blocks * 16,
            Self::A8 | Self::R8 => w * h,
            Self::Bgra8 | Self::R32F | Self::R24G8 => w * h * 4,
        }
    }
}

impl ImageBuffer {
    /// The resource class name.
    pub const CLASS: &'static str = "ImageBuffer";

    /// `Video::ImageBuffer::Serialize` + `PlatformSerialize`.
    ///
    /// Original: Runtime `0x1800f4f40`, Runtime `0x1800f5980`.
    pub fn read(r: &mut Reader) -> Result<Self> {
        r.block(|r, _| {
            let width = r.u32()?;
            let height = r.u32()?;
            let image_type = r.u32()?;
            let usage = r.u32()?;
            let format = ImageFormat::from_raw(r.u32()?)?;
            let generate_mips = r.u32()? != 0;
            let address = [r.u32()?, r.u32()?, r.u32()?];
            let filter = r.u32()?;
            let source = r.string()?;
            let mut levels = Vec::new();
            if !matches!(usage, 4 | 5) {
                for _ in 0..r.count()? {
                    let n = r.count()?;
                    levels.push(r.bytes(n)?.to_vec());
                }
            }
            Ok(ImageBuffer {
                width,
                height,
                image_type,
                usage,
                format,
                generate_mips,
                address,
                filter,
                source,
                levels,
            })
        })
    }

    /// The number of faces: 6 for a cube map, else 1.
    pub fn faces(&self) -> usize {
        if self.image_type == 0 {
            1
        } else {
            6
        }
    }

    /// The number of mip levels of each face.
    pub fn mip_count(&self) -> usize {
        self.levels.len() / self.faces()
    }
}
