use super::*;

#[test]
fn round_trip_keeps_the_original_names() {
    let mut s = Settings::default();
    s.game.ui_scale = 125;
    s.audio.music = 30;
    s.video.shadow_scalar = 2;
    s.video.save(&mut s.persistence);
    s.audio.save(&mut s.persistence);
    s.game.save(&mut s.persistence);
    let text = s.persistence.serialize();
    assert!(text.contains("GameUIScale=125"));
    assert!(text.contains("AudioMusicVolume=30"));
    assert!(text.contains("VideoShadowScalar=2"));
    let mut p = Persistence::default();
    p.parse(&text);
    assert_eq!(GameOptions::load(&p).ui_scale, 125);
    assert_eq!(AudioConfig::load(&p).music, 30);
    assert_eq!(VideoConfig::load(&p).shadow_scalar, 2);
    assert_eq!(
        GameOptions::load(&Persistence::default()),
        GameOptions::default()
    );
}
