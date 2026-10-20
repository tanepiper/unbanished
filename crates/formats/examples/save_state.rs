//! Write a save's expanded game-state stream to a file (analysis helper).
//! `cargo run -p unbanished-formats --example save_state -- IN.sav OUT.bin`
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let save = unbanished_formats::save::Save::parse(&std::fs::read(&args[1]).unwrap()).unwrap();
    std::fs::write(&args[2], &save.state).unwrap();
}
