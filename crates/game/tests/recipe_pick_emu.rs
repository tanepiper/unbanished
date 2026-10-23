//! A production cycle's recipe against the original (`FUN_14011e940` under the emulator,
//! `tools/emu/recipe_pick.py`): the recipe chosen and the next number, which shows whether one was
//! drawn. Skips when the ground truth has not been made.
use unbanished_game::random::Random;
use unbanished_game::work::produce::pick_recipe;

#[test]
fn original_recipe_pick() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../game-data/derived/emu/recipe_pick/cases.tsv");
    let Ok(data) = std::fs::read_to_string(path) else {
        eprintln!("recipe ground truth missing; skipping");
        return;
    };
    let mut count = 0;
    for line in data.lines().skip(1) {
        let p: Vec<&str> = line.split('\t').collect();
        let weights: Vec<f32> = if p[2].is_empty() {
            Vec::new()
        } else {
            p[2].split(',').map(|v| v.parse().unwrap()).collect()
        };
        let mut rng = Random::new(p[0].parse().unwrap());
        let picked = pick_recipe(p[1].parse().unwrap(), &weights, &mut rng);
        assert_eq!(
            picked as i64,
            p[3].parse::<i64>().unwrap(),
            "case {count}: recipe"
        );
        assert_eq!(
            rng.rand(),
            p[4].parse::<u32>().unwrap(),
            "case {count}: draws"
        );
        count += 1;
    }
    assert!(count > 0);
    eprintln!("{count} recipe picks match the original");
}
