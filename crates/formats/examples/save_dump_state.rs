//! Write a save's decoded game-state stream to a file (analysis helper; output stays local).
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let save = unbanished_formats::save::Save::parse(&std::fs::read(&args[1]).unwrap()).unwrap();
    std::fs::write(&args[2], &save.state).unwrap();
}
