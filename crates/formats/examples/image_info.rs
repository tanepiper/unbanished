//! Print an ImageBuffer's header and level sizes (analysis helper).
//! `cargo run -p unbanished-formats --example image_info -- path/to/WinData REFERENCE`
use unbanished_formats::pkg::{Archive, Package};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = std::path::Path::new(&args[1]);
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        archive.add(Package::parse(std::fs::read(dir.join(name)).unwrap()).unwrap());
    }
    for r in &args[2..] {
        let data = archive.read_ref(r).unwrap();
        match unbanished_formats::types::image_buffer(&data) {
            Ok(img) => println!(
                "{r}: {}x{} type {} format {:?} usage {} levels {:?}",
                img.width,
                img.height,
                img.image_type,
                img.format,
                img.usage,
                img.levels.iter().map(|l| l.len()).collect::<Vec<_>>()
            ),
            Err(e) => {
                let mi = unbanished_formats::types::material_instance(&data);
                println!("{r}: not an image ({e}); material instance: {mi:?}");
            }
        }
    }
}
