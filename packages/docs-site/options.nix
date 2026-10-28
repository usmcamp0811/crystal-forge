{ pkgs, inputs, repositoryUrl ? "https://gitlab.com/crystal-forge/crystal-forge/-/blob/dev", ... }:
let
  self = inputs.self;
  moduleInputs = self.inputs // { inherit self; };
  snowfall = moduleInputs.snowfall-lib.mkLib {
    inputs = moduleInputs;
    src = self.outPath;
    snowfall = {
      namespace = "crystal-forge";
      meta = { name = "crystal-forge"; title = "Crystal Forge"; };
    };
  };
  # Match Snowfall's module namespace while retaining its project extensions.
  lib = snowfall // {
    crystal-forge = snowfall;
    # Legacy module descriptions already contain Markdown.
    mdDoc = text: text;
  };
  exported = self.nixosModules;
  moduleNames = builtins.attrNames exported;
  evaluated = import (pkgs.path + "/nixos/lib/eval-config.nix") {
    inherit pkgs lib;
    inherit (pkgs.stdenv.hostPlatform) system;
    specialArgs = {
      inherit lib;
      inputs = moduleInputs;
      namespace = "crystal-forge";
      host = "options-reference";
      format = "";
    };
    # Only the options tree is consumed. No system, activation script, assertion
    # result, database, or service is built. STIG modules retain their real
    # enable defaults because tracking declarations depend on those defaults.
    # Snowfall wrappers re-root source paths in a separate cleaned store tree.
    # Import every exported name from its source with the same project library
    # so declaration provenance remains relative to this repository root.
    modules = map (name: self.outPath + "/modules/nixos/${name}/default.nix") moduleNames ++ [ {
      _module.check = false;
    } ];
  };
  sourcePrefix = "${self.outPath}/";
  isProject = path: lib.hasPrefix "${sourcePrefix}modules/nixos/" (toString path);
  docs = pkgs.nixosOptionsDoc {
    inherit (evaluated) options;
    warningsAreErrors = false;
    transformOptions = option:
      # Remove lazy values BEFORE serialization. Package defaults and examples
      # can require host configuration or force the application build closure.
      (builtins.removeAttrs option [ "default" "example" "relatedPackages" ]) // {
        visible = option.visible && builtins.any isProject option.declarations;
        declarations = map (path: lib.removePrefix sourcePrefix (toString path))
          (builtins.filter isProject option.declarations);
      };
  };
  manifest = pkgs.writeText "docs-site-modules.json" (builtins.toJSON {
    modules = map (name: {
      inherit name;
      declaration = "modules/nixos/${name}/default.nix";
    }) moduleNames;
  });
  python = pkgs.python3.withPackages (p: [ p.pyyaml ]);
in
pkgs.runCommand "docs-site-options" {
  nativeBuildInputs = [ python ];
  passthru = { inherit manifest; optionsJSON = docs.optionsJSON; };
} ''
  python ${./options.py} \
    ${docs.optionsJSON}/share/doc/nixos/options.json "$out" \
    --manifest ${manifest} --repository-url ${lib.escapeShellArg repositoryUrl}
''
