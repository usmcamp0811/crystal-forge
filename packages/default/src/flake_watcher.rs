
let output = Command::new("nix")
    .args(["flake", "show", "--json", repo_url])
    .output()?;

let json: serde_json::Value = serde_json::from_slice(&output.stdout)?;
let configs = json["nixosConfigurations"]
    .as_object()
    .unwrap()
    .keys()
    .cloned()
    .collect::<Vec<_>>();
