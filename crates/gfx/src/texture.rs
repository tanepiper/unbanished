//! Texture upload helpers.

use unbanished_formats::texture::decode_rgba;
use unbanished_formats::types::ImageBuffer;

/// Upload RGBA8 mip levels as an sRGB texture.
pub fn upload_rgba(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    width: u32,
    height: u32,
    mips: &[Vec<u8>],
) -> wgpu::TextureView {
    upload_rgba_as(
        device,
        queue,
        label,
        width,
        height,
        mips,
        wgpu::TextureFormat::Rgba8UnormSrgb,
    )
}

/// Upload 4-byte-per-texel mip levels in `format`.
pub fn upload_rgba_as(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    width: u32,
    height: u32,
    mips: &[Vec<u8>],
    format: wgpu::TextureFormat,
) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: mips.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (level, data) in mips.iter().enumerate() {
        let (w, h) = ((width >> level).max(1), (height >> level).max(1));
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: level as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
    }
    texture.create_view(&Default::default())
}

/// Decode and upload an `ImageBuffer` with all its mip levels (sRGB).
pub fn upload_image(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    img: &ImageBuffer,
) -> Option<wgpu::TextureView> {
    let mips: Option<Vec<Vec<u8>>> = img.levels[..img.mip_count()]
        .iter()
        .enumerate()
        .map(|(i, level)| decode_rgba(img.format, img.width >> i, img.height >> i, level))
        .collect();
    let mips = mips?;
    if mips.is_empty() {
        return None;
    }
    Some(upload_rgba(
        device, queue, label, img.width, img.height, &mips,
    ))
}

/// Decode an `ImageBuffer` into RGBA8 levels: `faces × mips` (face-major, as stored).
pub fn decode_levels(img: &ImageBuffer) -> Option<Vec<Vec<u8>>> {
    let mips = img.mip_count().max(1);
    let mut out = Vec::with_capacity(img.faces() * mips);
    for face in 0..img.faces() {
        for mip in 0..mips {
            let level = img.levels.get(face * mips + mip)?;
            let (w, h) = ((img.width >> mip).max(1), (img.height >> mip).max(1));
            out.push(decode_rgba(img.format, w, h, level)?);
        }
    }
    Some(out)
}

/// Upload a 2D `ImageBuffer` (all mips) as linear RGBA8, the way the D3D9 renderer samples it.
pub fn upload_image_linear(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    img: &ImageBuffer,
) -> Option<wgpu::TextureView> {
    let levels = decode_levels(img)?;
    Some(upload_rgba_as(
        device,
        queue,
        label,
        img.width,
        img.height,
        &levels[..img.mip_count().max(1)],
        wgpu::TextureFormat::Rgba8Unorm,
    ))
}

/// Upload a cube-map `ImageBuffer` (6 faces, D3D order +X −X +Y −Y +Z −Z) as a cube view.
pub fn upload_cube(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    img: &ImageBuffer,
) -> Option<wgpu::TextureView> {
    if img.faces() != 6 {
        return None;
    }
    let levels = decode_levels(img)?;
    let mips = img.mip_count().max(1);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: img.width,
            height: img.height,
            depth_or_array_layers: 6,
        },
        mip_level_count: mips as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for face in 0..6 {
        for mip in 0..mips {
            let (w, h) = ((img.width >> mip).max(1), (img.height >> mip).max(1));
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: mip as u32,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: face as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &levels[face * mips + mip],
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(w * 4),
                    rows_per_image: Some(h),
                },
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
    Some(texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::Cube),
        ..Default::default()
    }))
}
