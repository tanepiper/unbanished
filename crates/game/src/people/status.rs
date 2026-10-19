//! Read-only StatusBarUI calculations (FUN_14007eda0 / FUN_14006b830).

#[derive(Debug, Clone, Copy, Default)]
pub struct CitizenValue {
    pub age: f32,
    pub adult_age: f32,
    pub school: bool,
    pub health: f32,
    pub happiness: f32,
    /// StatusIconComponent +0xf0, not Citizen +0x88.
    pub status: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TownStatus {
    pub adults: u32,
    pub students: u32,
    pub children: u32,
    pub counts: [u32; 6],
    pub health: f32,
    pub happiness: f32,
}

/// Port of FUN_14007eda0's citizen loop. Preserve creation-list float summation order.
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

/// Port of FUN_14006b830: each group has a `whole` and `half` image. Keep the container
/// visible so all five resource-defined slots occupy their original geometry.
pub fn meter(value: f32, maximum: f32, icons: usize) -> Vec<(bool, bool)> {
    let halves = ((value / maximum) * (icons as i32 * 2) as f32 + 0.5) as i32;
    (0..icons)
        .map(|i| {
            let boundary = i as i32 * 2 + 1;
            (boundary < halves, boundary == halves)
        })
        .collect()
}

/// Port of FUN_1401318c0's mask mutation. Deleting an icon is permitted on disabled owners.
pub fn set_icon(mask: &mut u32, bit: u8, on: bool, owner_disabled: bool) {
    if !on {
        *mask &= !(1 << (bit & 31));
    } else if !owner_disabled {
        *mask |= 1 << (bit & 31);
    }
}
