//! `cf-nixos-module` — generate standalone NixOS modules from exported
//! Crystal Forge policies and compliance bundles.
//!
//! The binary is fully offline: it never contacts a Crystal Forge server or
//! database, and it never evaluates Nix contained in an export.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::ExitCode;

use cf_nixos_module::generate::{Generated, derive_baseline, generate};
use cf_nixos_module::input::{LoadedInput, load_input};
use cf_nixos_module::select::select_policies;
use cf_nixos_module::write::write_output;

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
        --check            Validate the inputs without writing output.
        --strict           Exit non-zero if any policy cannot be converted.
        --baseline <NAME>  Identifier for the generated baseline. Defaults to
                           the bundle name, or the first input's file stem.
    -h, --help             Print this help.

The artifact contains default.nix, lib.nix, and manifest.json. Import it and
the baseline is enabled by default:

    imports = [ ./generated-compliance ];
    # Optional: importing the artifact enables the baseline by default.
    crystal-forge.compliance.<baseline>.enable = false;

The generated module depends only on standard NixOS module infrastructure and
requires no running Crystal Forge server.
";

#[derive(Debug, Default)]
struct Args {
    inputs: Vec<PathBuf>,
    output: Option<PathBuf>,
    check: bool,
    strict: bool,
    baseline: Option<String>,
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
            "--baseline" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--baseline requires a name".to_string())?;
                args.baseline = Some(value);
            }
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
                } else if let Some(value) = other.strip_prefix("--baseline=") {
                    args.baseline = Some(value.to_string());
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

    let baseline = derive_baseline(&selection, args.baseline.as_deref());

    let generated = generate(&selection, &baseline).map_err(|conflicts| {
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
    println!(
        "\nImport the artifact (the baseline applies by default):\n\n  \
         imports = [ ./{} ];\n  # To disable explicitly:\n  crystal-forge.compliance.{}.enable = false;",
        output
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "generated-compliance".to_string()),
        generated.baseline
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
        let parsed = args(&[
            "-i",
            "a.json",
            "--output=out",
            "--strict",
            "--baseline=custom",
        ])
        .expect("parses");
        assert_eq!(parsed.inputs, vec![PathBuf::from("a.json")]);
        assert_eq!(parsed.output, Some(PathBuf::from("out")));
        assert!(parsed.strict);
        assert_eq!(parsed.baseline, Some("custom".to_string()));
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
