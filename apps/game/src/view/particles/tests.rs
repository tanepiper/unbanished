#[test]
fn the_emission_direction_turns_the_cone() {
    use glam::Vec3;
    let v = Vec3::new(0.3, 2.0, -0.4);
    // +Y leaves the velocity as it is.
    let same = super::rotate(super::from_direction(Vec3::Y), v);
    assert!((same - v).length() < 1e-6);
    // +Z turns the cone's axis (+Y) up.
    let up = super::rotate(super::from_direction(Vec3::Z), Vec3::Y);
    assert!((up - Vec3::Z).length() < 1e-6, "{up}");
    let side = super::rotate(super::from_direction(Vec3::X), Vec3::Y);
    assert!((side - Vec3::X).length() < 1e-6, "{side}");
}

use super::*;

fn smoke() -> ParticleSystem {
    ParticleSystem {
        instanced: true,
        instances: 3,
        max_particles: 32,
        material: None,
        rate: 5.0,
        rate_change: 0.0,
        rate_scale: 1.0,
        duration: 0,
        direction: [0.0, 1.0, 0.0],
        spawn_size: 0.0,
        spawn_box: [1.0; 3],
        cone: [180.0, 20.0],
        speed: [0.5, 0.0],
        force_scale: 1.0,
        damping: 1.0,
        rotation: [0.0, 180.0],
        spin: [30.0, 5.0],
        spin_change: 0.0,
        spin_damping: 1.0,
        life: [4.5, 1.0],
        force_amount: 1.0,
        force: [15.0, 4.0, 10.0],
        size: [0.15, 0.05],
        growth: 9.0,
        growth_damping: 1.0,
        colors: [0xc0c0_c0c0, 0xc0ff_ffff],
        color_span: [0.0, 1.0],
        fade_in: 100_000,
        fade_out: 3_400_000,
    }
}

#[test]
fn chimney_smoke_rises_grows_and_lives_out() {
    let mut rng = Random::new(1);
    let mut i = Instance::new(Arc::new(System::Standard(smoke())), Vec3::ZERO);
    i.alpha = 1.0;
    for _ in 0..90 {
        i.update(1.0 / 30.0, &mut rng);
    }
    // Five a second for three seconds, none dead yet (lives from 3.5 s).
    assert!(
        (14..=16).contains(&i.particles.len()),
        "{}",
        i.particles.len()
    );
    let oldest = i.particles[0];
    assert!(0.0 < oldest.pos.z && 0.0 < oldest.pos.x, "{:?}", oldest.pos);
    assert!(0.5 < oldest.size, "{}", oldest.size);
    i.emitting = false;
    for _ in 0..200 {
        i.update(1.0 / 30.0, &mut rng);
    }
    assert!(i.is_empty());
}

#[test]
fn the_tornado_funnel_rises_and_widens() {
    let t = TornadoParticle {
        max_particles: 750,
        material: None,
        radius: 20.0,
        rise: 75.0,
        wobble: 26.0,
        wobble_turn: 30.0,
        spiral_turn: 400.0,
        rate: 20.0,
        life: [30.0, 2.0],
        size: [2.5, 15.0],
        size_scale: 30.0,
        colors: [0xffc0_c0c0, 0x8090_9090],
        fade_in: 50_000,
        fade_out: 1_000_000,
    };
    let mut rng = Random::new(1);
    let mut i = Instance::new(Arc::new(System::Tornado(t)), Vec3::ZERO);
    // The slot's prewarm.
    for _ in 0..32 * 30 {
        i.update(1.0 / 30.0, &mut rng);
    }
    // Twenty a second living 28 to 32 s.
    assert!(
        (560..=640).contains(&i.particles.len()),
        "{}",
        i.particles.len()
    );
    let oldest = i.particles[0];
    let newest = *i.particles.last().unwrap();
    assert!(newest.pos.z < oldest.pos.z && oldest.pos.z < 80.0);
    assert!(newest.size < oldest.size && oldest.size <= 15.0 && 2.5 <= newest.size);
    assert!(oldest.alpha < newest.alpha);
    assert!(i
        .particles
        .iter()
        .all(|p| (-180.0..=180.0).contains(&p.rotation)));
}

#[test]
fn slots_fade_in_and_out() {
    let mut p = Particles::default();
    let sys = Some((Arc::new(System::Standard(smoke())), Vec3::ZERO, 1.0, 0.0));
    p.set((1, 0), Want::On, sys.clone());
    p.update(0.5);
    assert!(p.is_running((1, 0)));
    p.update(0.6);
    assert_eq!(p.slots[&(1, 0)].instance.alpha, 1.0);
    // Asking again changes nothing.
    p.set((1, 0), Want::On, sys);
    assert_eq!(p.slots[&(1, 0)].instance.alpha, 1.0);
    p.set((1, 0), Want::Off, None);
    p.update(0.5);
    assert!(p.is_running((1, 0)));
    p.update(0.6);
    assert!(!p.is_running((1, 0)));
}
