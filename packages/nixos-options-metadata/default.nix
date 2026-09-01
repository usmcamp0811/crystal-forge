{ lib, pkgs, ... }:
let
  evaluated = import (pkgs.path + "/nixos/lib/eval-config.nix") {
    inherit (pkgs.stdenv.hostPlatform) system;
    modules = [ ];
  };

  # `unique` changes merge behavior, not the accepted value domain. Other
  # wrappers (notably nullOr and coercedTo) add values and must remain unknown.
  unwrapTransparent = type:
    if type.functor.name == "unique" && type.nestedTypes ? elemType
    then unwrapTransparent type.nestedTypes.elemType
    else type;

  classify = rawType:
    let
      type = unwrapTransparent rawType;
      functorName = type.functor.name;
      enumValues = type.functor.payload.values or [ ];
      # Phase 3's enum editor persists semantic JSON strings. Enum domains that
      # contain non-string values cannot be represented faithfully and remain
      # unknown rather than being coerced or lied about.
      scalarEnum = builtins.all builtins.isString enumValues;
    in
    if functorName == "bool" || functorName == "boolByOr" then {
      value_type = "boolean";
    } else if functorName == "enum" && scalarEnum then {
      value_type = "enum";
      enum_values = enumValues;
    } else if functorName == "int" then {
      value_type = "integer";
    } else if functorName == "separatedString" then {
      value_type = if type.functor.payload.sep == "\n" then "lines" else "string";
    } else if builtins.elem functorName [ "str" "nonEmptyStr" "singleLineStr" "strMatching" ] then {
      value_type = "string";
    } else {
      value_type = "unknown";
    };

  optionToEntry = option:
    {
      path = lib.showOption option.loc;
    }
    // classify option.type
    // lib.optionalAttrs (builtins.isString (option.description or null)) {
      inherit (option) description;
    };

  # Keep visibility and submodule traversal in lockstep with
  # lib.optionAttrSetToDocList, while retaining each option's real type object.
  collectOptions = options:
    lib.concatMap
      (option:
        let
          visible = option.visible or true;
          includeOption = (if builtins.isBool visible then visible else visible == "shallow")
            && !(option.internal or false);
          includeSubOptions = if builtins.isBool visible then visible else visible == "transparent";
          subOptions = option.type.getSubOptions option.loc;
        in
        lib.optional includeOption (optionToEntry option)
        ++ lib.optionals (includeSubOptions && subOptions != { }) (collectOptions subOptions))
      (lib.collect lib.options.isOption options);

  entriesByPath = builtins.listToAttrs (map (entry: {
    name = entry.path;
    value = entry;
  }) (collectOptions evaluated.options));
  metadata = map (path: entriesByPath.${path})
    (lib.sort builtins.lessThan (builtins.attrNames entriesByPath));
in
pkgs.writeTextFile {
  name = "crystal-forge-nixos-options-metadata";
  destination = "/share/crystal-forge/nixos-options.json";
  text = builtins.toJSON metadata;
  passthru = { inherit metadata; };
  meta = {
    description = "Compact NixOS option metadata for Crystal Forge";
    platforms = lib.platforms.all;
  };
}
