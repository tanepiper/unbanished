use super::*;

fn material(size: i32, flags: u32) -> RawMaterial {
    RawMaterial {
        reference: String::new(),
        size,
        flags,
        ..Default::default()
    }
}

#[test]
fn slots() {
    assert_eq!(stock_slot(0x400), Some(0)); // wood
    assert_eq!(stock_slot(0x800), Some(1)); // stone
    assert_eq!(stock_slot(0x1000), Some(2)); // iron
    assert_eq!(stock_slot(0xc0), Some(3)); // firewood
    assert_eq!(stock_slot(0x200), Some(4)); // tools
    assert_eq!(stock_slot(0x22), Some(5)); // potato
    assert_eq!(stock_slot(0x4000), Some(7)); // leather coat
}

#[test]
fn round_robin_respects_capacity_and_flags() {
    let materials = vec![material(11, 0x400), material(1, 0x22)];
    let mut storages = vec![
        Storage {
            capacity: 6000,
            accepts: 0x16220,
            items: vec![],
        },
        Storage {
            capacity: 50,
            accepts: 0x1c40,
            items: vec![],
        },
    ];
    distribute(&mut storages, &materials, &[0, 1], 0x400, 120);
    distribute(&mut storages, &materials, &[0, 1], 0x20, 1800);
    let table = stock_table(&storages, &materials);
    assert_eq!(table[0], 4); // 4 logs of 11 fit in 50
    assert_eq!(table[5], 1800);
}
