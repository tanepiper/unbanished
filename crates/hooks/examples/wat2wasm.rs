//! Turns a rule-hook plugin written as WebAssembly text into a `.wasm` file, for the tests:
//! `cargo run -p unbanished-hooks --example wat2wasm IN.wat OUT.wasm`.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(input), Some(output)) = (args.first(), args.get(1)) else {
        eprintln!("usage: wat2wasm IN.wat OUT.wasm");
        std::process::exit(2);
    };
    let bytes = wat::parse_file(input).unwrap_or_else(|e| {
        eprintln!("{input}: {e}");
        std::process::exit(1);
    });
    std::fs::write(output, bytes).unwrap_or_else(|e| {
        eprintln!("{output}: {e}");
        std::process::exit(1);
    });
}
