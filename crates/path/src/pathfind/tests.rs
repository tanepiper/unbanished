use super::*;

fn grid(rows: &[&str]) -> (usize, Vec<CellType>) {
    // Rows are given top (high y) first; '.' ground, '#' water, '=' road.
    let n = rows.len();
    let mut t = vec![CellType(2); n * n];
    for (r, row) in rows.iter().enumerate() {
        let y = n - 1 - r;
        for (x, ch) in row.chars().enumerate() {
            t[y * n + x] = match ch {
                '#' => CellType(8),
                '=' => CellType(1),
                _ => CellType(2),
            };
        }
    }
    (n, t)
}

#[test]
fn regions_split_by_water() {
    let (n, t) = grid(&["..#..", "..#..", "..#..", "..#..", "..#.."]);
    let r = regions(n, &t);
    assert_eq!(r[0], 1);
    assert_eq!(r[3], 2);
    assert_eq!(r[2], 0);
}

#[test]
fn path_goes_around_water_and_prefers_roads() {
    let (n, t) = grid(&[".....", ".###.", ".....", ".....", "....."]);
    let r = regions(n, &t);
    let mut pf = Pathfinder::new(n, &t);
    pf.set_mode(0);
    let from = (3 * n) as i32; // (0, 3)
    let to = (3 * n + 4) as i32; // (4, 3), across the water row
    let p = request(&mut pf, &t, &r, WALKABLE, from, to).unwrap();
    assert_eq!(*p.first().unwrap(), from);
    assert_eq!(*p.last().unwrap(), to);
    assert!(p.iter().all(|&c| t[c as usize] != CellType(8)));
    // Unreachable across a full water column.
    let (n, t) = grid(&["..#..", "..#..", "..#..", "..#..", "..#.."]);
    let r = regions(n, &t);
    let mut pf = Pathfinder::new(n, &t);
    pf.set_mode(0);
    assert!(request(&mut pf, &t, &r, WALKABLE, 0, 4).is_none());
}
