//! Read-only StatusBarUI calculations.
//!
//! Original: `FUN_14007eda0`, `FUN_14006b830`.

/// What the status bar reads of one citizen.
#[derive(Debug, Clone, Copy, Default)]
pub struct CitizenValue {
    /// The age.
    pub age: f32,
    /// The age at which a citizen is an adult.
    pub adult_age: f32,
    /// Whether it attends school.
    pub school: bool,
    /// The health.
    pub health: f32,
    /// The happiness.
    pub happiness: f32,
    // +0xf0; +0x88
    /// StatusIconComponent, not Citizen.
    pub status: u32,
}

/// The status bar's counts: adults, students, children and the other values.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TownStatus {
    /// The adults.
    pub adults: u32,
    /// The students.
    pub students: u32,
    /// The children.
    pub children: u32,
    /// The citizens with each of the first six status icons.
    pub counts: [u32; 6],
    /// The mean health.
    pub health: f32,
    /// The mean happiness.
    pub happiness: f32,
}

/// Preserve creation-list float summation order.
///
/// Original: `FUN_14007eda0` (citizen loop).
pub fn aggregate(citizens: impl IntoIterator<Item = CitizenValue>) -> TownStatus {
    let mut result = TownStatus::default();
    let mut count = 0;
    for c in citizens {
        if c.school {
            result.students += 1;
        } else if c.age < c.adult_age {
            result.children += 1;
        } else {
            result.adults += 1;
        }
        for (i, n) in result.counts.iter_mut().enumerate() {
            *n += (c.status >> i) & 1;
        }
        result.health += c.health;
        result.happiness += c.happiness;
        count += 1;
    }
    if count != 0 {
        result.health /= count as f32;
        result.happiness /= count as f32;
    }
    result
}

/// Each group has a `whole` and `half` image. Keep the container visible so all five
/// resource-defined slots occupy their original geometry.
///
/// Original: `FUN_14006b830`.
pub fn meter(value: f32, maximum: f32, icons: usize) -> Vec<(bool, bool)> {
    let halves = ((value / maximum) * (icons as i32 * 2) as f32 + 0.5) as i32;
    (0..icons)
        .map(|i| {
            let boundary = i as i32 * 2 + 1;
            (boundary < halves, boundary == halves)
        })
        .collect()
}

/// Deleting an icon is permitted on disabled owners.
///
/// Original: `FUN_1401318c0` (mask mutation).
pub fn set_icon(mask: &mut u32, bit: u8, on: bool, owner_disabled: bool) {
    if !on {
        *mask &= !(1 << (bit & 31));
    } else if !owner_disabled {
        *mask |= 1 << (bit & 31);
    }
}
