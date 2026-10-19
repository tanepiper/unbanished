//! Prints the `ClimateGenerator` resources given on the command line:
//! `climate_info <file.crs> [...]`.

use std::{env, fs, process};

use banished_formats::game::climate_generator;

fn main() {
    let paths: Vec<_> = env::args_os().skip(1).collect();
    if paths.is_empty() {
        eprintln!("usage: climate_info <ClimateGenerator_resource.crs> [...]");
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
        let climate = match climate_generator(&bytes) {
            Ok(climate) => climate,
            Err(error) => {
                eprintln!("{}: {error}", path.to_string_lossy());
                process::exit(1);
            }
        };
        println!(
            "{} enabled={} settings={:?} trailing={:?}",
            climate.name, climate.enabled, climate.settings, climate.trailing_settings
        );
        for (month, values) in climate.monthly_thresholds.iter().enumerate() {
            println!("  Month{month:02}: {values:?}");
        }
    }
}
