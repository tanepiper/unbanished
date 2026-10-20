//! Full shipped picking corpus; source data is optional and never embedded.
use unbanished_formats::{
    crs, picking,
    pkg::{Archive, Package},
};

#[test]
fn shipped_picking_geometry() {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../game-data/original/WinData");
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        let Ok(b) = std::fs::read(dir.join(name)) else {
            eprintln!("game data missing; skipping");
            return;
        };
        archive.add(Package::parse(b).unwrap());
    }
    let mut counts = [0; 4];
    for name in archive.names() {
        let path = name.replace('\\', "/");
        if !path.starts_with("Models/") && !path.starts_with("Template/") {
            continue;
        }
        let data = archive.read(name).unwrap();
        let Ok(h) = crs::header(&data) else { continue };
        match h.type_id {
            0x9c23_01f9 => {
                let mesh = picking::picking_mesh(&data).unwrap_or_else(|e| panic!("{name}: {e}"));
                for m in &mesh.meshes {
                    for t in &m.triangles {
                        assert!(t.vertices.iter().all(|&i| (i as usize) < m.vertices.len()));
                        assert!(t.normals.iter().all(|&i| (i as usize) < m.normals.len()));
                    }
                }
                counts[0] += 1;
            }
            0x541c_c42e => {
                picking::picking_description(&data).unwrap_or_else(|e| panic!("{name}: {e}"));
                counts[1] += 1;
            }
            0x78e6_8d30 => {
                picking::edge_mesh(&data).unwrap_or_else(|e| panic!("{name}: {e}"));
                counts[2] += 1;
            }
            0x4a96_367e => {
                picking::highlight_description(&data).unwrap_or_else(|e| panic!("{name}: {e}"));
                counts[3] += 1;
            }
            _ => {}
        }
    }
    assert!(
        counts[0] >= 161 && counts[1] >= 47 && counts[2] >= 150 && counts[3] >= 54,
        "{counts:?}"
    );
}
