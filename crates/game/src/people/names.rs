//! Names for citizens and towns, made from the name databases (`Game/Names/*`). See
//! docs/TOWN-NAME-DATABASE.md.
//!
//! Original: `FUN_1400a9d50` (one candidate), `FUN_1400aa000` (try again
//! while the candidate is a name the game ships), `FUN_1400aa130` (citizens), `FUN_14009ef90`
//! (towns).

use unbanished_formats::names::NameDatabase;

use crate::random::Random;

/// Bit 26 of a transition mask: the name may end here.
const END: u32 = 26;

fn key(window: [u8; 4]) -> u32 {
    // Bytes are sign-extended (`movsbl`); names are ASCII, so this is a plain big-endian key.
    let b = |c: u8| c as i8 as i32 as u32;
    (((b(window[0]) << 8 | b(window[1])) << 8 | b(window[2])) << 8) | b(window[3])
}

/// Binary search of the transition table as the original does it (unsigned keys).
fn find(db: &NameDatabase, k: u32) -> Option<usize> {
    let n = db.transitions.len() as i32;
    let (mut lo, mut hi) = (0i32, n);
    loop {
        let mid = (hi + lo) >> 1;
        if mid < 0 || n <= mid {
            return None;
        }
        let rec = key(db.transitions[mid as usize].prefix);
        if k == rec {
            return Some(mid as usize);
        }
        if k <= rec {
            hi = mid - 1;
        } else {
            lo = mid + 1;
        }
        if hi < lo {
            return None;
        }
    }
}

/// Compose one candidate of roughly `min..max` letters.
///
/// Original: `FUN_1400a9d50`.
pub fn candidate(db: &NameDatabase, rng: &mut Random, min: i32, max: i32) -> String {
    let mut out = [0u8; 0x100];
    let start = rng.rand_below(db.starting_transitions.len() as u32) as usize;
    let first = db.transitions[db.starting_transitions[start] as usize];
    out[..4].copy_from_slice(&first.prefix);
    out[0] = out[0].wrapping_sub(0x20);
    let mut window = first.prefix;
    let limit = rng.rand_range(min, max - 4) as i64;
    let steps = (max * 2 - 3) as i64;
    let mut written = 0usize; // r14
    let mut last_boundary: i32 = -1; // r15
    let mut pos: i64 = 0; // rbp
    let finish = |out: &[u8; 0x100]| {
        let len = out.iter().position(|&c| c == 0).unwrap_or(out.len());
        String::from_utf8_lossy(&out[..len]).into_owned()
    };
    if steps <= 0 {
        return finish(&out);
    }
    loop {
        let Some(i) = find(db, key(window)) else {
            return finish(&out);
        };
        let rec = db.transitions[i];
        window = rec.prefix;
        let mask = rec.next_characters;
        if mask == 0 {
            return finish(&out);
        }
        if pos >= max as i64 && rec.boundary != 0 {
            return finish(&out);
        }
        if rec.boundary != 0 {
            last_boundary = written as i32 + 4;
        }
        let total = (mask & 0x7ff_ffff).count_ones();
        let pick = rng.rand_below(total);
        let mut chosen = total;
        let mut seen = 0;
        for bit in 0..=END {
            if mask >> bit & 1 != 0 {
                if seen == pick {
                    chosen = bit;
                    break;
                }
                seen += 1;
            }
        }
        if (pos >= limit && mask >> END & 1 != 0) || chosen == END {
            out[written + 4] = 0;
            return finish(&out);
        }
        let c = chosen as u8 + b'a';
        pos += 1;
        written += 1;
        out[3 + pos as usize] = c;
        window = [window[1], window[2], window[3], c];
        if pos >= steps {
            break;
        }
    }
    if last_boundary > 0 {
        out[last_boundary as usize] = 0;
    }
    finish(&out)
}

/// Up to 50 candidates, keeping the first of at least `min` letters that is not a shipped name (or
/// the last one tried).
///
/// Original: `FUN_1400aa000`.
pub fn generate(db: &NameDatabase, rng: &mut Random, min: i32, max: i32) -> String {
    let mut name = String::new();
    for _ in 0..50 {
        name = candidate(db, rng, min, max);
        if name.len() as i32 >= min && !is_shipped(db, &name) {
            break;
        }
    }
    name
}

/// Binary search of the sorted accepted-name pool (`String::StrCompare`).
fn is_shipped(db: &NameDatabase, name: &str) -> bool {
    let n = db.names.len() as i32;
    let (mut lo, mut hi) = (0i32, n);
    loop {
        let mid = (hi + lo) >> 1;
        if mid < 0 || n <= mid {
            return false;
        }
        match name.as_bytes().cmp(db.names[mid as usize].as_bytes()) {
            std::cmp::Ordering::Equal => return true,
            std::cmp::Ordering::Less => hi = mid - 1,
            std::cmp::Ordering::Greater => lo = mid + 1,
        }
        if hi < lo {
            return false;
        }
    }
}

/// A citizen's first name, 3 to 10 letters.
///
/// Original: `FUN_1400aa130`.
pub fn citizen_name(db: &NameDatabase, rng: &mut Random) -> String {
    generate(db, rng, 3, 10)
}

/// A town name of at least five letters; `…vill` becomes `…ville`.
///
/// Original: `FUN_14009ef90`.
pub fn town_name(db: &NameDatabase, rng: &mut Random, max: i32) -> String {
    let mut name = generate(db, rng, 5, max);
    if name.len() > 4 && name.ends_with("vill") {
        name.push('e');
    }
    name
}
