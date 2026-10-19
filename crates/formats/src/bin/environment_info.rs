//! Prints the `EnvironmentDescription` resources given on the command line:
//! `environment_info <file.crs> [...]`.

use std::{env, fs, process};

use banished_formats::game::environment_description;

fn main() {
    let paths: Vec<_> = env::args_os().skip(1).collect();
    if paths.is_empty() {
        eprintln!("usage: environment_info <EnvironmentDescription_resource.crs> [...]");
        process::exit(2);
    }
    for path in paths {
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                eprintln!("{}: {error}", path.to_string_lossy());
                process::exit(1);
            }
        };
        let environment = match environment_description(&bytes) {
            Ok(environment) => environment,
            Err(error) => {
                eprintln!("{}: {error}", path.to_string_lossy());
                process::exit(1);
            }
        };
        println!("template: {:?}", environment.template);
        for (index, (effect, audio)) in environment.weather.iter().enumerate() {
            println!("weather[{index}]: effect={effect:?} ambient_audio={audio:?}");
        }
    }
}
