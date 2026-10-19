//! Screenshots (`FUN_14009b270`): key-map actions `0x21` ScreenshotNoUI and `0x22` Screenshot
//! (`FUN_14009b850` sets the game's `+0xc0` / `+0xc1`). The draw (`FUN_14009b4e0`) copies the
//! frame after the scene for the first, after the UI for the second, and writes it to the
//! first free `Screenshots\Screenshot%i.bmp` (`%i` from 0) in the save directory
//! (`FileSystem::GetSaveDirectory`) with `ImageBuffer::SaveImageToFile`; both flags clear.
//!
//! Inferred: the file is a 24-bit bottom-up BMP (the engine's writer is not traced).
//! Deviation: the browser offers the file as a download instead of writing it.

/// The frame (`width × height` RGBA texels, top row first) as a BMP.
pub fn bmp(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let row = (width as usize * 3).next_multiple_of(4);
    let size = 54 + row * height as usize;
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&(size as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(width as i32).to_le_bytes());
    out.extend_from_slice(&(height as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&24u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&((row * height as usize) as u32).to_le_bytes());
    out.extend_from_slice(&2835u32.to_le_bytes());
    out.extend_from_slice(&2835u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    for y in (0..height as usize).rev() {
        let start = out.len();
        for x in 0..width as usize {
            let p = &rgba[(y * width as usize + x) * 4..][..3];
            out.extend_from_slice(&[p[2], p[1], p[0]]);
        }
        out.resize(start + row, 0);
    }
    out
}

/// Write the frame to the first free `Screenshots/Screenshot%i.bmp`.
#[cfg(not(target_arch = "wasm32"))]
pub fn save(rgba: &[u8], width: u32, height: u32) {
    let Some(dir) = crate::platform::paths::user_dir().map(|d| d.join("Screenshots")) else {
        return;
    };
    let _ = std::fs::create_dir_all(&dir);
    let path = (0..)
        .map(|i| dir.join(format!("Screenshot{i}.bmp")))
        .find(|p| !p.exists())
        .unwrap();
    match std::fs::write(&path, bmp(rgba, width, height)) {
        Ok(()) => log::info!("screenshot {}", path.display()),
        Err(e) => log::error!("{}: {e}", path.display()),
    }
}

/// Offer the frame as `Screenshot%i.bmp`, numbered from 0 in this page.
#[cfg(target_arch = "wasm32")]
pub fn save(rgba: &[u8], width: u32, height: u32) {
    use std::sync::atomic::{AtomicU32, Ordering};
    use wasm_bindgen::JsCast;
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let data = bmp(rgba, width, height);
    let name = format!("Screenshot{}.bmp", NEXT.fetch_add(1, Ordering::Relaxed));
    let result = (|| {
        let array = js_sys::Array::of1(&js_sys::Uint8Array::from(data.as_slice()));
        let options = web_sys::BlobPropertyBag::new();
        options.set_type("image/bmp");
        let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&array, &options)?;
        let url = web_sys::Url::create_object_url_with_blob(&blob)?;
        let document = web_sys::window()
            .and_then(|w| w.document())
            .ok_or("no document")?;
        let a: web_sys::HtmlAnchorElement = document.create_element("a")?.dyn_into()?;
        a.set_href(&url);
        a.set_download(&name);
        a.click();
        web_sys::Url::revoke_object_url(&url)?;
        Ok::<_, wasm_bindgen::JsValue>(())
    })();
    if let Err(e) = result {
        log::error!("screenshot: {e:?}");
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_bmp_is_bottom_up_bgr_with_padded_rows() {
        // 1 × 2: red on top, blue below.
        let b = super::bmp(&[255, 0, 0, 255, 0, 0, 255, 255], 1, 2);
        assert_eq!(&b[..2], b"BM");
        assert_eq!(b.len(), 54 + 4 * 2);
        assert_eq!(&b[54..58], &[255, 0, 0, 0]);
        assert_eq!(&b[58..62], &[0, 0, 255, 0]);
    }
}
