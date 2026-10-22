//! Walkable regions: flood fills that number the connected walkable areas.

use super::*;

/// Label connected walkable cells in scan order, starting at 1 (0 = none).
///
/// Original: `FUN_1400a1950`.
pub fn regions(size: usize, types: &[CellType]) -> Vec<u16> {
    let mut region = vec![0u16; types.len()];
    let mut id: u16 = 1;
    for i in 0..types.len() {
        if bit(types[i]) & WALKABLE != 0 && region[i] == 0 {
            fill(size, types, &mut region, i, id);
            id = id.wrapping_add(1);
        }
    }
    region
}

/// `FUN_1400a1640`: FIFO scanline fill. A run is widened by one cell for the rows above and
/// below only when that cell is strictly inside the row (`row_start < left - 1`,
/// `right + 1 < row_end`), so runs touch diagonal neighbours except at the map edge.
pub(super) fn fill(size: usize, types: &[CellType], region: &mut [u16], start: usize, id: u16) {
    let n = size as i32;
    let walk = |c: i32| bit(types[c as usize]) & WALKABLE != 0;
    let mut queue = VecDeque::from([start as i32]);
    while let Some(c) = queue.pop_front() {
        let row = c / n;
        let (row_start, row_end) = (n * row, (row + 1) * n - 1);
        if region[c as usize] != 0 || !walk(c) {
            continue;
        }
        let mut left = c;
        while left - 1 >= row_start && walk(left - 1) {
            left -= 1;
        }
        let mut right = c;
        while right + 1 <= row_end && walk(right + 1) {
            right += 1;
        }
        for k in left..=right {
            region[k as usize] = id;
        }
        if row_start < left - 1 {
            left -= 1;
        }
        if right + 1 < row_end {
            right += 1;
        }
        for (ok, step) in [(row > 0, -n), (row < n - 1, n)] {
            if !ok {
                continue;
            }
            for u in left..=right {
                let k = u + step;
                if region[k as usize] == 0 && walk(k) {
                    queue.push_back(k);
                }
            }
        }
    }
}
