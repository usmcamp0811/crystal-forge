fn get_nixos_configurations(&repo_url: String) {
    let flake_show = Command::new("nix")
        .args(["flake", "show", "--json", &repo_url])
        .output()?;
    let flake_json: serde_json::Value = serde_json::from_slice(&flake_show.stdout)?;
    let nixos_configs = flake_json["nixosConfigurations"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();

    println!("nixosConfigurations: {}", nixos_configs);
}
