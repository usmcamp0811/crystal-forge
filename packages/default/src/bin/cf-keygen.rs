use base64::{Engine as _, engine::general_purpose};
use ed25519_dalek::{SigningKey, Verifier};
use rand::rngs::OsRng;
use std::{
    env, fs,
    io::{self, Write},
    path::PathBuf,
    process,
};

fn print_help(program: &str) {
    eprintln!(
        "\
Usage: {program} [-t ed25519] [-f <output_file>]

Generates an Ed25519 key pair for Crystal Forge agents.

Options:
  -t <type>         Key type (must be 'ed25519'; default)
  -f <path>         File to save the private key (default: /var/lib/crystal_forge/<hostname>.key)
  -h, --help        Show this help message

Example:
  {program} -f /var/lib/crystal_forge/agent.key
"
    );
}

fn confirm_overwrite(path: &PathBuf) {
    eprint!("⚠️  {} already exists. Overwrite? [y/N] ", path.display());
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    if !input.trim().eq_ignore_ascii_case("y") {
        eprintln!("Aborted.");
        process::exit(0);
    }
}

fn get_default_path() -> PathBuf {
    let hostname = hostname::get()
        .unwrap_or_else(|_| "agent".into())
        .to_string_lossy()
        .to_string();
    PathBuf::from(format!("/var/lib/crystal_forge/{}.key", hostname))
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut key_type = "ed25519";
    let mut file: Option<PathBuf> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-t" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("❌ Error: missing argument for -t");
                    print_help(&args[0]);
                    process::exit(1);
                }
                key_type = &args[i];
            }
            "-f" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("❌ Error: missing argument for -f");
                    print_help(&args[0]);
                    process::exit(1);
                }
                file = Some(PathBuf::from(&args[i]));
            }
            "-h" | "--help" => {
                print_help(&args[0]);
                return;
            }
            _ => {
                eprintln!("❌ Unknown option: {}", args[i]);
                print_help(&args[0]);
                process::exit(1);
            }
        }
        i += 1;
    }

    if key_type != "ed25519" {
        eprintln!("❌ Error: only ed25519 is supported");
        process::exit(1);
    }

    let path = file.unwrap_or_else(get_default_path);

    eprint!("📝 Save key to {}? [Y/n] ", path.display());
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    let trimmed = input.trim();
    if !trimmed.is_empty() && !trimmed.eq_ignore_ascii_case("y") {
        eprintln!("Aborted.");
        process::exit(0);
    }

    let signing_key = SigningKey::generate(&mut OsRng);
    let verify_key = signing_key.verifying_key();

    let private_b64 = general_purpose::STANDARD.encode(signing_key.to_bytes());
    let public_b64 = general_purpose::STANDARD.encode(verify_key.to_bytes());

    fs::create_dir_all(path.parent().unwrap()).ok();
    fs::write(&path, private_b64).expect("failed to write private key");

    let pub_path = path.with_extension("pub");
    fs::write(&pub_path, format!("{public_b64}\n")).expect("failed to write public key");

    println!("✅ Private key saved to: {}", path.display());
    println!("✅ Public key saved to:  {}", pub_path.display());
}
