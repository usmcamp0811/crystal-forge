//! Emit deterministic Crystal Forge export fixtures on stdout.
//!
//! Used by the `nixos-module-generation` flake check and by integration tests,
//! following the same generated-fixture pattern as `xccdf-export-fixture`.

use std::process::ExitCode;

use cf_nixos_module::fixture::{bundle_xccdf_xml, policy_set_json};

const USAGE: &str = "\
USAGE:
    cf-nixos-module-fixture <policy-set|bundle-xccdf>

    policy-set     Emit a policy-set JSON export (urn:crystal-forge:policy-set:1).
    bundle-xccdf   Emit a CF-native XCCDF 1.2 compliance bundle export.
";

fn main() -> ExitCode {
    let Some(kind) = std::env::args().nth(1) else {
        eprint!("{USAGE}");
        return ExitCode::from(2);
    };

    match kind.as_str() {
        "policy-set" => print!("{}", policy_set_json()),
        "bundle-xccdf" => print!("{}", bundle_xccdf_xml()),
        other => {
            eprintln!("unknown fixture: {other}\n");
            eprint!("{USAGE}");
            return ExitCode::from(2);
        }
    }

    ExitCode::SUCCESS
}
