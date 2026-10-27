use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use gameframe_frame::{Origin, Session, Settings};

use super::*;
use crate::frame::manifest::UNBANISHED;

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("unbanished-kernel-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let w = dir.join("Banished/WinData");
    std::fs::create_dir_all(&w).unwrap();
    std::fs::write(w.join("data0.pkg"), b"").unwrap();
    dir
}

fn found(dir: &Path) -> Vec<steam::Install> {
    vec![steam::Install {
        label: "Steam".into(),
        win_data: dir.join("Banished/WinData"),
    }]
}

fn start(k: &mut Kernel, settings: Value) -> (Option<Launch>, Vec<Event>) {
    let mut out = Outbox::default();
    let data = json!({ "renderer": "wgpu", "settings": settings, "args": [] });
    k.on_event(
        &Event::new(topics::GAME_START, Origin::Frame, data),
        &mut out,
    );
    (k.take_launch(), out.take())
}

fn failure(events: &[Event]) -> String {
    events[0].data["detail"].as_str().unwrap().to_string()
}

#[test]
fn the_manifest_is_one_the_frame_takes() {
    let m = manifest::manifest();
    m.check().unwrap();
    assert!(m.setting(INSTALL).is_some());
}

#[test]
fn the_frame_lists_the_copies_found_and_starts_the_first() {
    let dir = temp("found");
    let mut s = Session::new(Box::new(Kernel::new(found(&dir))), Settings::new()).unwrap();
    let states = Rc::new(RefCell::new(Vec::new()));
    let keep = states.clone();
    s.on(topics::GAME_STATUS, move |e| {
        keep.borrow_mut()
            .push(e.data["state"].as_str().unwrap_or("").to_string())
    });
    s.ready();
    s.tick();
    s.start(Vec::new());
    s.tick();
    assert_eq!(*states.borrow(), ["loading"]);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn each_machine_and_folder_makes_its_launch() {
    let dir = temp("machines");
    let mut k = Kernel::new(found(&dir));
    let win_data = dir.join("Banished/WinData");
    let (l, _) = start(&mut k, json!({ MACHINE: BANISHED, MUTE: true }));
    let l = l.unwrap();
    assert!(l.vanilla && l.mute);
    assert_eq!(l.data, win_data);
    let chosen = dir.join("Banished").to_string_lossy().into_owned();
    let (l, _) = start(
        &mut k,
        json!({ INSTALL: FOLDER, FOLDER_PATH: chosen, ARGS: "--ui-scale 2" }),
    );
    let l = l.unwrap();
    assert_eq!(l.data, win_data);
    assert_eq!(l.args, ["--ui-scale", "2"]);
    assert_eq!(l.vanilla, !manifest::has_extensions());
    if manifest::has_extensions() {
        let (l, _) = start(&mut k, json!({ MACHINE: UNBANISHED }));
        assert!(!l.unwrap().vanilla);
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_folder_without_the_game_fails_with_the_reason() {
    let dir = temp("missing");
    let mut k = Kernel::new(Vec::new());
    let (l, out) = start(&mut k, json!({}));
    assert!(l.is_none());
    assert!(failure(&out).contains("not found in Steam"));
    let empty = dir.to_string_lossy().into_owned();
    let (l, out) = start(&mut k, json!({ INSTALL: FOLDER, FOLDER_PATH: empty }));
    assert!(l.is_none());
    assert!(failure(&out).contains("no data0.pkg"));
    let (_, out) = start(&mut k, json!({ MACHINE: "doom" }));
    assert!(failure(&out).contains("cannot run"));
    std::fs::remove_dir_all(&dir).unwrap();
}
