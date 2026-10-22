use super::*;

fn key(x: f32, at: f32) -> Keyframe {
    Keyframe {
        eye: Vec3::new(x, 0.0, 10.0),
        target: Vec3::new(x, 10.0, 0.0),
        at,
    }
}

#[test]
fn parses_the_options() {
    assert_eq!(
        Keyframe::parse("1,2,3>4,5,6@7.5"),
        Some(Keyframe {
            eye: Vec3::new(1.0, 2.0, 3.0),
            target: Vec3::new(4.0, 5.0, 6.0),
            at: 7.5,
        })
    );
    assert_eq!(Keyframe::parse("1,2>4,5,6@7"), None);
    assert_eq!(
        Subject::parse("profession Fish 2"),
        Some(Subject::Profession("fish".into(), 2))
    );
    assert_eq!(
        Subject::parse("animal deer"),
        Some(Subject::Animal(Some("deer".into()), 0))
    );
    assert_eq!(Subject::parse("animal 3"), Some(Subject::Animal(None, 3)));
    assert_eq!(Subject::parse("boat 1"), Some(Subject::Boat(1)));
    let f = Follow::parse("boat 0@12-30").unwrap();
    assert_eq!((f.start, f.end), (12.0, 30.0));
    assert_eq!(Follow::parse("citizen 4@3").unwrap().end, f32::INFINITY);
    let rig = Rig::parse("10,6").unwrap();
    assert_eq!(
        (rig.back, rig.up, rig.side),
        (10.0, 6.0, Rig::default().side)
    );
    assert!(Rig::parse("1,2,3,4,5,6").is_none());
}

#[test]
fn a_path_passes_through_its_keyframes_and_rests_at_its_ends() {
    let keys = [key(0.0, 1.0), key(10.0, 2.0), key(30.0, 4.0)];
    assert_eq!(path(&keys, 0.5), None);
    for k in &keys {
        let (eye, target) = path(&keys, k.at).unwrap();
        assert!((eye - k.eye).length() < 1e-4 && (target - k.target).length() < 1e-4);
    }
    assert_eq!(path(&keys, 9.0).unwrap().0, keys[2].eye);
    // Slow at the start and the end, moving at the keyframe between.
    let x = |t: f32| path(&keys, t).unwrap().0.x;
    assert!(x(1.01) - x(1.0) < 0.01);
    assert!(x(4.0) - x(3.99) < 0.02);
    assert!(x(2.01) - x(1.99) > 0.15);
    // Never turns back.
    let mut previous = x(1.0);
    for i in 1..=300 {
        let now = x(1.0 + i as f32 * 0.01);
        assert!(now >= previous - 1e-4, "at {i}");
        previous = now;
    }
}

#[test]
fn the_camera_rises_over_what_hides_the_subject() {
    fn boxes(all: &[Obstacle]) -> impl FnMut(Vec3, Vec3) -> bool + '_ {
        move |a, e| all.iter().any(|b| box_blocks(b, a, e))
    }
    let at = |lo: Vec3, hi: Vec3| Obstacle {
        placed: 0,
        to_local: Mat4::IDENTITY,
        lo,
        hi,
    };
    let roof = at(Vec3::new(-2.0, -6.0, 0.0), Vec3::new(2.0, -2.0, 3.0));
    let subject = Vec3::new(0.0, 0.0, 1.0);
    assert!(roof.crosses(subject, Vec3::new(0.0, -8.0, 3.0)));
    assert!(!roof.crosses(subject, Vec3::new(0.0, 8.0, 3.0)));
    // From 8 behind at 3 up, the line clears the roof's edge nearest the subject (y −2, z 3)
    // from z 9.
    let l = lift(boxes(&[roof]), subject, Vec3::new(0.0, -8.0, 3.0)).unwrap();
    assert!((l - 6.0).abs() <= LIFT_STEP, "{l}");
    // Too high a wall to rise over: the camera stays.
    let wall = at(Vec3::new(-2.0, -6.0, 0.0), Vec3::new(2.0, -2.0, 30.0));
    assert_eq!(
        lift(boxes(&[wall]), subject, Vec3::new(0.0, -8.0, 3.0)),
        None
    );
    // A large box holding the subject (a market) is open ground; a small one (a house it
    // brushes past) keeps the eye out.
    let market = at(Vec3::new(-6.0, -6.0, 0.0), Vec3::new(6.0, 6.0, 7.0));
    let eye = Vec3::new(0.0, -4.0, 3.0);
    assert_eq!(lift(boxes(&[market]), subject, eye), Some(0.0));
    let house = at(Vec3::new(-3.0, -5.0, 0.0), Vec3::new(3.0, 1.0, 4.0));
    assert_eq!(lift(boxes(&[house]), subject, eye), Some(1.0 + LIFT_STEP));
    // A turned box is tested in its own frame: a long bridge at 45° misses a point its
    // axis-aligned bounds would hold.
    let bridge = Obstacle {
        placed: 0,
        to_local: Mat4::from_rotation_z(-std::f32::consts::FRAC_PI_4),
        lo: Vec3::new(-10.0, -1.0, 0.0),
        hi: Vec3::new(10.0, 1.0, 2.0),
    };
    assert!(bridge.holds(Vec3::new(5.0, 5.0, 1.0)));
    assert!(!bridge.holds(Vec3::new(5.0, -5.0, 1.0)));
}

#[test]
fn the_rig_sits_behind_and_above_looking_ahead() {
    let rig = Rig {
        back: 4.0,
        up: 2.0,
        side: 1.0,
        ahead: 8.0,
        look_up: 1.0,
    };
    // Heading 0 faces +y; heading 90 faces −x.
    let (eye, target) = rig.place(Vec3::ZERO, 0.0);
    assert!((eye - Vec3::new(1.0, -4.0, 2.0)).length() < 1e-5);
    assert!((target - Vec3::new(0.0, 8.0, 1.0)).length() < 1e-5);
    let (eye, target) = rig.place(Vec3::ZERO, 90.0);
    assert!((eye - Vec3::new(4.0, 1.0, 2.0)).length() < 1e-5);
    assert!((target - Vec3::new(-8.0, 0.0, 1.0)).length() < 1e-5);
}
