//! Scan a clip's sampled poses for discontinuities (analysis helper).
//! `cargo run -p unbanished-game --example pose_scan -- WinData Models/Citizen/CitizenAnims.rsc:resource Idle`
use unbanished_formats::pkg::{Archive, Package};
use unbanished_game::behaviour::animation::sample_pose;
use unbanished_game::scheduler::ticks;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = std::path::Path::new(&args[1]);
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        archive.add(Package::parse(std::fs::read(dir.join(name)).unwrap()).unwrap());
    }
    let g =
        unbanished_formats::types::animation_group(&archive.read_ref(&args[2]).unwrap()).unwrap();
    let clip = g.clips.iter().find(|c| c.name == args[3]).unwrap();
    println!(
        "frames {} rate {} interval {} duration_ticks {} loop {} start {} end {} loop_start {}",
        clip.frames.len(),
        clip.sample_rate,
        clip.sample_interval,
        clip.duration_ticks,
        clip.looping,
        clip.start_frame,
        clip.end_frame,
        clip.loop_start_frame
    );
    let mut prev: Option<Vec<unbanished_formats::skeleton::Transform>> = None;
    for i in 0..480 {
        let t = i as f32 / 120.0;
        let pose = sample_pose(clip, ticks(t), 0);
        if let Some(p) = &prev {
            let worst = pose
                .iter()
                .zip(p)
                .skip(1)
                .map(|(a, b)| {
                    let d: f32 = (0..4).map(|k| a.rotation[k] * b.rotation[k]).sum();
                    1.0 - d.abs()
                })
                .fold(0.0f32, f32::max);
            if worst > 0.01 {
                println!("jump at {t:.3}s: {worst}");
            }
        }
        prev = Some(pose);
    }
}
