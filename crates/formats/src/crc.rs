//! Standard CRC-32 (IEEE), as used by `System::CRC` and for type ids.

const fn table() -> [u32; 256] {
    let mut t = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
}

static TABLE: [u32; 256] = table();

/// The CRC-32 (IEEE) of bytes.
pub fn crc32(bytes: impl IntoIterator<Item = u8>) -> u32 {
    !bytes.into_iter().fold(!0u32, |c, b| {
        TABLE[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8)
    })
}

/// The table loop alone, from `c` and without the final complement (the engine's
/// `System::CRC` steps; [`crc32`] is `!crc32_raw(!0, ..)`).
pub fn crc32_raw(c: u32, bytes: &[u8]) -> u32 {
    bytes.iter().fold(c, |c, &b| {
        TABLE[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8)
    })
}

/// Resource type id: CRC-32 over the class name as UTF-16LE (`Object::Factory::Startup`).
pub fn type_id(class_name: &str) -> u32 {
    crc32(class_name.encode_utf16().flat_map(u16::to_le_bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vectors() {
        assert_eq!(crc32(*b"123456789"), 0xCBF4_3926);
        assert_eq!(type_id("ImageBuffer"), 0x84E6_F2DD);
        assert_eq!(type_id("GraphicsMesh"), 0xE61C_2FFE);
        assert_eq!(type_id("MaterialInstance"), 0x2EBE_ED54);
    }
}
