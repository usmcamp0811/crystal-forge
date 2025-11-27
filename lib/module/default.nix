{lib, ...}:
with lib; rec {
  ## Create a NixOS module option.
  ##
  ## ```nix
  ## lib.mkOpt nixpkgs.lib.types.str "My default" "Description of my option."
  ## ```
  ##
  #@ Type -> Any -> String
  mkOpt = type: default: description:
    mkOption {inherit type default description;};

  ## Create a NixOS module option without a description.
  ##
  ## ```nix
  ## lib.mkOpt' nixpkgs.lib.types.str "My default"
  ## ```
  ##
  #@ Type -> Any -> String
  mkOpt' = type: default: mkOpt type default null;

  ## Create a boolean NixOS module option.
  ##
  ## ```nix
  ## lib.mkBoolOpt true "Description of my option."
  ## ```
  ##
  #@ Type -> Any -> String
  mkBoolOpt = mkOpt types.bool;

  ## Create a boolean NixOS module option without a description.
  ##
  ## ```nix
  ## lib.mkBoolOpt true
  ## ```
  ##
  #@ Type -> Any -> String
  mkBoolOpt' = mkOpt' types.bool;

  enabled = {
    ## Quickly enable an option.
    ##
    ## ```nix
    ## services.nginx = enabled;
    ## ```
    ##
    #@ true
    enable = true;
  };

  disabled = {
    ## Quickly disable an option.
    ##
    ## ```nix
    ## services.nginx = enabled;
    ## ```
    ##
    #@ false
    enable = false;
  };

  #
  # Extracts the Vault path and field names from a Vault-client template.
  #
  # This function processes a Vault-client template and extracts:
  # 1. The Vault path specified in the `with secret` block.
  # 2. All field names referenced as `.Data.data.<field>` within the template.
  #
  # @param template (string): The Vault-client template containing `with secret`
  #        and field references.
  #
  # @returns { path: string, fields: list of strings }:
  #          - `path`: The Vault path specified in the template (e.g., `"secret/campground/example"`).
  #          - `fields`: A list of all field names extracted from `.Data.data.<field>` patterns
  #            in the template.
  #
  # Example:
  # ```
  # let
  #   template = ''
  #     {{ with secret "secret/campground/example" }}
  #     {{ .Data.data.field1 }}
  #     {{ .Data.data.field2 }}
  #     {{ end }}
  #   '';
  #   result = extractVaultPathAndFields template;
  # in
  #   result;
  # ```
  # Result:
  # ```
  # {
  #   path = "secret/campground/example";
  #   fields = [ "field1" "field2" ];
  # }
  # ```
  #
  # Notes:
  # - If no `with secret` block is found, `path` will be an empty string.
  # - If no fields are found, `fields` will be an empty list.
  extractVaultPathAndFields = template: let
    extractVaultPath = text: let
      vaultPathRegex = ''.*with secret "([^"]+)".*''; # Regular expression for Vault path
      match = builtins.match vaultPathRegex text;
    in
      if match != null && builtins.length match > 0
      then builtins.elemAt match 0
      else "";

    extractAllFields = text: let
      fieldRegex = ".*[{]{2}[ ]*\\.Data\\.data\\.([^ }]+)[ ]*[}]{2}.*";
      loop = text: acc: let
        match = builtins.match fieldRegex text;
      in
        if match == null || builtins.length match == 0
        then acc # Stop when no matches are found
        else let
          fieldName = builtins.elemAt match 0;
          removeSubstring = text: target:
            builtins.replaceStrings [target] [""] text;
          remainingText = removeSubstring text fieldName;
        in
          loop remainingText (acc ++ [fieldName]);
    in
      loop text [];

    vaultPath = extractVaultPath template; # Call Vault path extraction
    fieldResults = extractAllFields template; # Call field extraction
  in {
    path = vaultPath;
    fields = fieldResults;
  };

  # Finds all instances of `vault-agent` in the given system configuration and retrieves
  # the Vault paths and fields used in their configurations.
  #
  # This function processes a system configuration to locate all `vault-agent` services
  # and extracts:
  # 1. Vault paths specified in file templates and environment variable templates.
  # 2. All field names referenced in the Vault-client templates.
  #
  # @param systemConfig (attribute set): The NixOS system configuration containing
  #        definitions for `vault-agent` services.
  #
  # @returns list of attribute sets:
  #          Each item contains:
  #          - `path`: The Vault path specified in the template.
  #          - `fields`: A list of all field names extracted from `.Data.data.<field>` patterns
  #            in the template.
  #
  # Example:
  # ```
  # let
  #   result = lib.findVaultPathsAndFields outputs.nixosConfigurations.butler.config.campground;
  # in
  #   result;
  # ```
  #
  # Result:
  # ```
  # [
  #   { path = "secret/campground/example"; fields = [ "field1" "field2" ]; }
  #   { path = "secret/another/example"; fields = [ "fieldA" "fieldB" ]; }
  # ]
  # ```
  #
  # Notes:
  # - The function checks both file templates (`service.secrets.file`) and environment
  #   variable templates (`service.secrets.environment`).
  # - If `vault-agent` is not enabled or no templates are found, the function returns an empty list.
  findVaultPathsAndFields = systemConfig: let
    # Constant depth value for recursion
    depth = 3;

    # The `vault-agent` configuration path
    vaultAgentConfig = systemConfig.services."vault-agent";

    # Collect paths and templates for files
    processFileTemplates = service:
      if
        builtins.hasAttr "secrets" service
        && builtins.hasAttr "file" service.secrets
        && builtins.hasAttr "files" service.secrets.file
      then
        builtins.foldl'
        (acc: key: let
          fileConfig = service.secrets.file.files.${key};
        in
          if builtins.hasAttr "text" fileConfig
          then let
            template = fileConfig.text;
          in
            acc ++ [(extractVaultPathAndFields template)]
          else acc) []
        (builtins.attrNames service.secrets.file.files)
      else [];

    # Collect paths and templates for environment variables
    processEnvironmentTemplates = service:
      if
        builtins.hasAttr "secrets" service
        && builtins.hasAttr "environment" service.secrets
        && builtins.hasAttr "templates" service.secrets.environment
      then
        builtins.foldl'
        (acc: key: let
          envConfig = service.secrets.environment.templates.${key};
        in
          if builtins.hasAttr "text" envConfig
          then let
            template = envConfig.text;
          in
            acc ++ [(extractVaultPathAndFields template)]
          else acc) []
        (builtins.attrNames service.secrets.environment.templates)
      else [];

    # Process a single service for both files and environment templates
    processService = service:
      processFileTemplates service ++ processEnvironmentTemplates service;

    # Process all `vault-agent` services
    processVaultAgentServices = services:
      builtins.foldl'
      (acc: serviceName: let
        service = services.${serviceName};
      in
        acc ++ processService service) []
      (builtins.attrNames services);
  in
    if depth <= 0
    then []
    else if
      builtins.hasAttr "enable" vaultAgentConfig
      && vaultAgentConfig.enable or false
    then processVaultAgentServices vaultAgentConfig.services
    else [];

  findVaultPaths = depth: cfg:
    if depth <= 0
    then []
    else let
      isAttrs = x: builtins.isAttrs x && !builtins.isFunction x;
      tryRecurse = x: let
        res = builtins.tryEval (findVaultPaths (depth - 1) x);
      in
        if res.success
        then res.value
        else [];
      getSecretPaths = attr:
        if
          builtins.hasAttr "user-secrets" attr
          && attr.user-secrets.enable
        then let
          baseVaultPath = attr.user-secrets.vault-path or "";
          userNames = builtins.attrNames attr.user-secrets.users or [];
        in
          builtins.map (username: "${baseVaultPath}/${username}") userNames
        else [];
    in
      if isAttrs cfg
      then
        builtins.foldl'
        (acc: key: let
          value = cfg.${key};
          res = builtins.tryEval value;
        in
          if res.success
          then
            if isAttrs res.value
            then acc ++ (tryRecurse res.value)
            else if key == "vault-path" && cfg.enable or false
            then acc ++ [res.value]
            else acc
          else acc)
        (getSecretPaths cfg)
        (builtins.attrNames cfg)
      else [];

  ## Function to make shell Aliases / Functions
  ## Main reason to use this over the `home.shellAliases` is that this can handle
  ## both simple aliases and things that should be functions.. aka things that require
  ## inputs
  convertAlias = aliasAttrs:
    builtins.concatStringsSep "\n" (mapAttrsToList
      (name: value: let
        containsDollar = builtins.elem "$" (lib.splitString "" value);
        containsNewline = builtins.elem "\n" (lib.splitString "" value);
      in
        if containsDollar || containsNewline
        then ''
          function '${name}'() {
            ${value}
          }
        ''
        else let
          # Escape single quotes in the alias value
          escapedValue = builtins.replaceStrings ["'"] ["'\\''"] value;
        in "alias -- '${name}'='${escapedValue}'")
      aliasAttrs);
}
