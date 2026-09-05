{ flakeRef, configurationName }:

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

  encodeValue = depth: declaredType: value:
    if depth > 16 then {
      kind = "opaque";
      value = { type_name = "over_depth"; };
    }
    else if builtins.isNull value
      || builtins.isBool value
      || builtins.isInt value
      || builtins.isFloat value
      || builtins.isString value then {
        kind = "scalar";
        inherit value;
      }
    else if lib.isDerivation value then {
      kind = "package";
      value = {
        name = value.name or null;
        pname = value.pname or null;
        version = value.version or null;
        output_path = if value ? outPath then builtins.toString value.outPath else null;
      };
    }
    else if builtins.isFunction value then {
      kind = "opaque";
      value = { type_name = "lambda"; };
    }
    else if builtins.isList value then
      if builtins.length value > 256 then {
        kind = "opaque";
        value = { type_name = "list_over_limit"; };
      }
      else {
        kind = "list";
        value = map (item: encodeValue (depth + 1) declaredType item) value;
      }
    else if builtins.isAttrs value then
      let names = builtins.attrNames value; in
      if builtins.length names > 256 then {
        kind = "opaque";
        value = { type_name = "attribute_set_over_limit"; };
      }
      else {
        kind = if lib.hasInfix "submodule" declaredType then "submodule" else "attribute_set";
        value = builtins.mapAttrs (_: item: encodeValue (depth + 1) declaredType item) value;
      }
    else {
      kind = "opaque";
      value = { type_name = builtins.typeOf value; };
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
