//! `cf-nixos-module` — generate standalone NixOS modules from exported
//! Crystal Forge policies and compliance bundles.
//!
//! The binary is fully offline: it never contacts a Crystal Forge server or
//! database, and it never evaluates Nix contained in an export.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

use cf_nixos_module::generate::{Generated, Layout, generate};
use cf_nixos_module::input::{LoadedInput, load_input};
use cf_nixos_module::select::select_policies;

const USAGE: &str = "\
cf-nixos-module — generate standalone NixOS modules from exported Crystal Forge content

USAGE:
    cf-nixos-module --input <FILE> [--input <FILE>...] --output <DIR> [OPTIONS]

REQUIRED:
    -i, --input <FILE>     Exported policy document (.json/.toml) or CF-native
                           XCCDF bundle export (.xml/.zip). Repeatable.
    -o, --output <DIR>     Directory to write the generated module into.
                           Not required with --check.

OPTIONS:
        --check            Validate the inputs without writing any output.
        --strict           Exit non-zero if any policy cannot be converted.
        --single-file      Emit one combined default.nix instead of a directory
                           of per-policy modules.
    -h, --help             Print this help.

The generated module depends only on standard NixOS module infrastructure and
requires no running Crystal Forge server.
";

#[derive(Debug, Default)]
struct Args {
    inputs: Vec<PathBuf>,
    output: Option<PathBuf>,
    check: bool,
    strict: bool,
    single_file: bool,
    help: bool,
}

fn parse_args(raw: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut args = Args::default();
    let mut iter = raw.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => args.help = true,
            "--check" => args.check = true,
            "--strict" => args.strict = true,
            "--single-file" => args.single_file = true,
            "-i" | "--input" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--input requires a file path".to_string())?;
                args.inputs.push(PathBuf::from(value));
            }
            "-o" | "--output" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--output requires a directory path".to_string())?;
                args.output = Some(PathBuf::from(value));
            }
            other => {
                if let Some(value) = other.strip_prefix("--input=") {
                    args.inputs.push(PathBuf::from(value));
                } else if let Some(value) = other.strip_prefix("--output=") {
                    args.output = Some(PathBuf::from(value));
                } else {
                    return Err(format!("unrecognized argument: {other}"));
                }
            }
        }
    }

    Ok(args)
}

fn main() -> ExitCode {
    let args = match parse_args(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("error: {message}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };

    if args.help {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }

    match run(&args) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &Args) -> Result<ExitCode, String> {
    if args.inputs.is_empty() {
        return Err(format!("at least one --input is required\n\n{USAGE}"));
    }
    if args.output.is_none() && !args.check {
        return Err(format!(
            "--output is required unless --check is used\n\n{USAGE}"
        ));
    }

    // Reject duplicate --input paths so a file cannot silently be counted twice.
    let mut seen = BTreeSet::new();
    for path in &args.inputs {
        if !seen.insert(path.clone()) {
            return Err(format!(
                "input {} was supplied more than once",
                path.display()
            ));
        }
    }

    let mut loaded: Vec<LoadedInput> = Vec::with_capacity(args.inputs.len());
    for path in &args.inputs {
        let bytes = std::fs::read(path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        let label = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        let input = load_input(&bytes, &label).map_err(|error| error.to_string())?;
        loaded.push(input);
    }

    let selection = select_policies(&loaded).map_err(|conflicts| {
        let mut message =
            String::from("inputs disagree about an immutable policy version identity:\n");
        for conflict in conflicts {
            message.push_str(&format!("  {conflict}\n"));
        }
        message
    })?;

    let layout = if args.single_file {
        Layout::SingleFile
    } else {
        Layout::Directory
    };

    let generated = generate(&selection, layout).map_err(|conflicts| {
        let mut message = String::from("conflicting NixOS implementations:\n");
        for conflict in conflicts {
            message.push_str(&format!("  {conflict}\n"));
        }
        message.push_str(
            "\nResolve the conflict in Crystal Forge, or generate from a policy set that does not \
             contain both policies. This tool never picks a winner automatically.\n",
        );
        message
    })?;

    report(&generated, &selection.deduplicated);

    if args.strict && !generated.skipped.is_empty() {
        return Err(format!(
            "--strict: {} policy/policies could not be converted",
            generated.skipped.len()
        ));
    }

    if args.check {
        println!(
            "check: {} policy/policies convertible, {} skipped; no output written",
            generated.implemented.len(),
            generated.skipped.len()
        );
        return Ok(ExitCode::SUCCESS);
    }

    let output = args.output.as_ref().ok_or("--output is required")?;
    write_output(&generated, output)?;

    println!(
        "wrote {} file(s) to {}",
        generated.files.len(),
        output.display()
    );

    Ok(ExitCode::SUCCESS)
}

/// Print per-policy diagnostics.
///
/// Skipped policies go to stderr so they remain visible when stdout is piped,
/// and so they cannot be mistaken for generated content.
fn report(generated: &Generated, deduplicated: &[String]) {
    for note in deduplicated {
        eprintln!("note: duplicate definition collapsed: {note}");
    }

    if generated.skipped.is_empty() {
        return;
    }

    eprintln!("\nThe following policies have no NixOS implementation and were NOT generated:");
    let width = generated
        .skipped
        .iter()
        .map(|entry| entry.policy.name.chars().count())
        .max()
        .unwrap_or(0)
        .min(60);
    for entry in &generated.skipped {
        eprintln!(
            "  {:<width$}  Reason: {}",
            entry.policy.name,
            entry.reason,
            width = width
        );
    }
    eprintln!();
}

/// Write every generated file beneath `output`.
///
/// Each path is re-validated before use so nothing can be written outside the
/// requested directory even if an upstream layer produced an unexpected path.
fn write_output(generated: &Generated, output: &Path) -> Result<(), String> {
    for file in &generated.files {
        let relative = Path::new(&file.path);
        if relative.is_absolute()
            || relative
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(format!("refusing to write unsafe path: {}", file.path));
        }

        let destination = output.join(relative);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
        }
        std::fs::write(&destination, &file.contents)
            .map_err(|error| format!("could not write {}: {error}", destination.display()))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> Result<Args, String> {
        parse_args(items.iter().map(|s| s.to_string()))
    }

    #[test]
    fn parses_repeated_inputs_and_output() {
        let parsed =
            args(&["--input", "a.json", "--input", "b.xml", "--output", "out"]).expect("parses");
        assert_eq!(parsed.inputs.len(), 2);
        assert_eq!(parsed.output, Some(PathBuf::from("out")));
        assert!(!parsed.check);
        assert!(!parsed.strict);
    }

    #[test]
    fn parses_equals_form_and_short_flags() {
        let parsed =
            args(&["-i", "a.json", "--output=out", "--strict", "--single-file"]).expect("parses");
        assert_eq!(parsed.inputs, vec![PathBuf::from("a.json")]);
        assert_eq!(parsed.output, Some(PathBuf::from("out")));
        assert!(parsed.strict);
        assert!(parsed.single_file);
    }

    #[test]
    fn rejects_unknown_arguments() {
        assert!(args(&["--nope"]).is_err());
    }

    #[test]
    fn rejects_flags_missing_values() {
        assert!(args(&["--input"]).is_err());
        assert!(args(&["--output"]).is_err());
    }

    #[test]
    fn requires_at_least_one_input() {
        let parsed = args(&["--output", "out"]).expect("parses");
        assert!(run(&parsed).is_err());
    }

    #[test]
    fn requires_output_unless_checking() {
        let parsed = args(&["--input", "a.json"]).expect("parses");
        let error = run(&parsed).expect_err("must require --output");
        assert!(error.contains("--output is required"), "{error}");
    }

    #[test]
    fn rejects_duplicate_input_paths() {
        let parsed = args(&["--input", "a.json", "--input", "a.json", "--check"]).expect("parses");
        let error = run(&parsed).expect_err("must reject duplicates");
        assert!(error.contains("more than once"), "{error}");
    }
}
