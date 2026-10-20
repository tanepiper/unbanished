//! Mod preview images from the mod sites: PNG or JPEG (by their first bytes, since some sites
//! name JPEG files `.png`), decoded to RGBA for the game's and the mod manager's lists.

/// The largest preview decoded, in pixels on each side; larger images are refused.
pub const MAX_SIDE: usize = 2048;

/// A decoded image: width, height and RGBA pixels, row by row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// The width in pixels.
    pub width: u32,
    /// The height in pixels.
    pub height: u32,
    /// Four bytes per pixel: red, green, blue, alpha.
    pub rgba: Vec<u8>,
}

/// Decode a PNG or JPEG image, or `None` for anything else or a damaged file.
pub fn decode(bytes: &[u8]) -> Option<Image> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        png(bytes)
    } else if bytes.starts_with(&[0xff, 0xd8]) {
        jpeg(bytes)
    } else {
        None
    }
}

fn png(bytes: &[u8]) -> Option<Image> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let (w, h) = (reader.info().width as usize, reader.info().height as usize);
    if w > MAX_SIDE || h > MAX_SIDE {
        return None;
    }
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => buf[..w * h * 4].to_vec(),
        png::ColorType::Rgb => buf[..w * h * 3]
            .chunks(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => buf[..w * h * 2]
            .chunks(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => buf[..w * h].iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return None,
    };
    Some(Image {
        width: w as u32,
        height: h as u32,
        rgba,
    })
}

fn jpeg(bytes: &[u8]) -> Option<Image> {
    use zune_core::{bytestream::ZCursor, colorspace::ColorSpace, options::DecoderOptions};
    let options = DecoderOptions::default()
        .jpeg_set_out_colorspace(ColorSpace::RGBA)
        .set_max_width(MAX_SIDE)
        .set_max_height(MAX_SIDE);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(bytes), options);
    let rgba = decoder.decode().ok()?;
    let info = decoder.info()?;
    let (w, h) = (info.width as u32, info.height as u32);
    (rgba.len() == (w * h * 4) as usize).then_some(Image {
        width: w,
        height: h,
        rgba,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn refuses_what_it_does_not_know() {
        assert_eq!(super::decode(b"GIF89a"), None);
        assert_eq!(super::decode(b"\x89PNG\r\n\x1a\nbroken"), None);
        assert_eq!(super::decode(&[0xff, 0xd8, 0xff, 0x00]), None);
    }
}
