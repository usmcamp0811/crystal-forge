{ flakeRef, configurationName, encodeValue }:

let
  flake = builtins.getFlake flakeRef;
  configuration = builtins.getAttr configurationName flake.nixosConfigurations;
  lib = configuration.pkgs.lib;
  carrier = configuration.config.system.build.toplevel;

  optionKey = path: builtins.hashString "sha256" (builtins.toJSON path);

  isOption = value:
    builtins.isAttrs value && (value._type or null) == "option";

  walkOptions = prefix: node:
    builtins.concatLists (map (name:
      let
        path = prefix ++ [ name ];
        value = builtins.getAttr name node;
      in
        if isOption value then [ path ]
        else if builtins.isAttrs value then walkOptions path value
        else [ ]
    ) (builtins.attrNames node));

  optionPaths = walkOptions [ ] configuration.options;
  optionEntries = map (path: {
    key = optionKey path;
    inherit path;
  }) optionPaths;
  optionKeys = map (entry: entry.key) optionEntries;

  origin = name: input:
    let
      sourceInfo = input.sourceInfo or { };
    in {
      inherit name;
      out_path = input.outPath or sourceInfo.outPath or null;
      revision = input.rev or sourceInfo.rev or null;
    };

  origins = [ (origin "self" flake) ]
    ++ lib.mapAttrsToList origin flake.inputs;

  getOption = path:
    builtins.foldl' (value: name: builtins.getAttr name value)
      configuration.options path;

  metadataFor = entry:
    let
      option = getOption entry.path;
    in {
      kind = "metadata";
      key = entry.key;
      metadata = {
        path = entry.path;
        option_type = option._type or null;
        loc = option.loc or [ ];
        declared_type = option.type.name or null;
        declarations = option.declarations or [ ];
        declaration_positions = option.declarationPositions or [ ];
        highest_prio = option.highestPrio or null;
        is_defined = option.isDefined or false;
        surviving_definition_sources = map (definition: {
          source_path = definition.file;
          priority = definition.priority or null;
        }) option.definitionsWithLocations;
      };
    };

  valueFor = entry:
    let
      option = getOption entry.path;
    in {
      kind = "value";
      key = entry.key;
      value = encodeValue 0 (option.type.name or "unknown") option.value;
    };

  indexPayload = {
    kind = "index";
    options = optionEntries;
    inherit origins;
  };

  withPayload = payload: carrier // {
    # Keep the system derivation identity and expose only inspector metadata.
    meta = { crystalForgeInspector = payload; };
  };

  jobs = [
    {
      name = "__crystalForgeConfigIndex";
      value = withPayload indexPayload;
    }
  ]
  ++ map (entry: {
    name = "meta_${entry.key}";
    value = withPayload (metadataFor entry);
  }) optionEntries
  ++ map (entry: {
    name = "value_${entry.key}";
    value = withPayload (valueFor entry);
  }) optionEntries;
in
  if builtins.length optionKeys != builtins.length (lib.unique optionKeys) then
    throw "Config inspector option-key collision"
  else
    builtins.listToAttrs jobs
