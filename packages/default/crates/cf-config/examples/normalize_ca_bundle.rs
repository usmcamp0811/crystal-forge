//! Runs the production CA normalizer on public sandbox regression fixtures.
//!
//! Writes certificate output to a file; diagnostics never contain input bytes.

use std::{fs, io::Read, process::ExitCode};

fn run() -> Result<(), &'static str> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("expected input and output paths");
    }
    let file = fs::File::open(&args[0]).map_err(|_| "input open failed")?;
    let mut input = String::new();
    file.take(4 * 1024 * 1024 + 1)
        .read_to_string(&mut input)
        .map_err(|_| "input read failed")?;
    let output = cf_config::ca_bundle::normalize_ca_bundle(&input).map_err(|e| e.code())?;
    fs::write(&args[1], output).map_err(|_| "output write failed")
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}
