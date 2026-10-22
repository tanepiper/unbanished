use super::*;

#[test]
fn open_map_distances() {
    let n = 32;
    let types = vec![CellType(2); n * n];
    let mut g = SectorGraph::new(n, &types);
    // One node per sector at its centre.
    assert_eq!(g.cell_node[0], g.cell_node[7 * n + 7]);
    assert_ne!(g.cell_node[0], g.cell_node[8]);
    assert_eq!(g.distance(0, 1, true, false, &types), 1);
    let d = g.distance(0, (n * n - 1) as i32, true, true, &types);
    assert!(d > 0);
    assert_eq!(g.distance(0, (n * n - 1) as i32, true, false, &types), d);
}
