{ flake, configuration, targetKey, provenanceLib, encodeValue }:

let
  lib = configuration.pkgs.lib;
  carrier = configuration.config.system.build.toplevel;

  optionKey = path: builtins.hashString "sha256" (builtins.toJSON path);

  optionTraversalDepthLimit = 16;
  optionInventoryDiagnosticLimit = 128;

  mergeTraversalResults = left: right:
    let
      remaining = optionInventoryDiagnosticLimit - builtins.length left.diagnostics;
      retained = lib.sublist 0 (lib.min remaining (builtins.length right.diagnostics))
        right.diagnostics;
    in {
      options = left.options ++ right.options;
      diagnostics = left.diagnostics ++ retained;
      diagnosticsTruncated = left.diagnosticsTruncated
        || right.diagnosticsTruncated
        || builtins.length right.diagnostics > remaining;
    };

  oneDiagnostic = diagnostic: {
    options = [ ];
    diagnostics = [ diagnostic ];
    diagnosticsTruncated = false;
  };

  emptyTraversal = {
    options = [ ];
    diagnostics = [ ];
    diagnosticsTruncated = false;
  };

  # Balanced merges avoid repeatedly copying a growing option list for wide
  # option trees while preserving the canonical attr-name traversal order.
  walkNames = depth: prefix: node: names:
    let count = builtins.length names;
    in if count == 0 then emptyTraversal
    else if count == 1 then walkChild depth prefix node (builtins.head names)
    else
      let midpoint = builtins.div count 2;
      in mergeTraversalResults
        (walkNames depth prefix node (lib.sublist 0 midpoint names))
        (walkNames depth prefix node (lib.sublist midpoint (count - midpoint) names));

  walkChild = depth: prefix: node: name:
    let
      path = prefix ++ [ name ];
      childAttempt = builtins.tryEval
        (let child = builtins.getAttr name node; in builtins.seq child child);
      child = childAttempt.value or null;
      typeAttempt = if childAttempt.success && builtins.isAttrs child
        then builtins.tryEval (child._type or null)
        else { success = false; value = null; };
    in if childAttempt.success && typeAttempt.success
      && builtins.isAttrs child && typeAttempt.value == "option" then {
        options = [ { inherit path; option = child; } ];
        diagnostics = [ ];
        diagnosticsTruncated = false;
      }
    else if !childAttempt.success || !builtins.isAttrs child || !typeAttempt.success then
      oneDiagnostic {
        inherit path;
        code = "unreadable_option_subtree";
        message = "Option subtree could not be inspected";
      }
    else
      walkOptions (depth + 1) path child;

  walkOptions = depth: prefix: node:
    let namesAttempt = builtins.tryEval (builtins.attrNames node);
    in if !namesAttempt.success then oneDiagnostic {
        path = prefix;
        code = "unreadable_option_subtree";
        message = "Option subtree could not be inspected";
      }
    else if depth >= optionTraversalDepthLimit then
      if namesAttempt.value == [ ] then {
        options = [ ]; diagnostics = [ ]; diagnosticsTruncated = false;
      } else oneDiagnostic {
        path = prefix;
        code = "option_subtree_depth_exceeded";
        message = "Option subtree exceeds the traversal depth limit";
      }
    else walkNames depth prefix node namesAttempt.value;

  traversal = walkOptions 0 [ ] configuration.options;
  rootUnreadable = builtins.any (diagnostic: diagnostic.path == [ ])
    traversal.diagnostics;
  sortedDiagnostics = builtins.sort
    (left: right: builtins.toJSON left.path < builtins.toJSON right.path)
    traversal.diagnostics;
  optionEntries = map (entry: entry // { key = optionKey entry.path; }) traversal.options;
  optionKeys = map (entry: entry.key) optionEntries;
  optionKeySet = builtins.listToAttrs
    (map (key: { name = key; value = true; }) optionKeys);
  inventoryComplete = sortedDiagnostics == [ ];

  mergeOptionTrees = trees:
    let count = builtins.length trees;
    in if count == 0 then { }
       else if count == 1 then builtins.head trees
       else
         let midpoint = builtins.div count 2;
         in lib.recursiveUpdate
           (mergeOptionTrees (lib.sublist 0 midpoint trees))
           (mergeOptionTrees (lib.sublist midpoint (count - midpoint) trees));
  observedOptions = mergeOptionTrees (map (entry:
    lib.setAttrByPath entry.path entry.option
  ) optionEntries);
  provenanceConfiguration = if inventoryComplete then configuration else
    configuration // { options = observedOptions; };
  provenance = provenanceLib {
    inherit flake;
    configuration = provenanceConfiguration;
  };

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

  metadataFor = entry:
    let option = entry.option; in {
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
    let option = entry.option; in {
      kind = "value";
      key = entry.key;
      value = encodeValue 0 (option.type.name or "unknown") option.value;
    };

  indexPayload = if rootUnreadable then
    throw "Config inspector option inventory root is unavailable"
  else {
    kind = "index";
    targetKey = targetKey;
    sourceOutPath = flake.outPath;
    options = map (entry: {
      inherit (entry) key path;
    }) optionEntries;
    optionInventoryComplete = inventoryComplete;
    optionInventoryDiagnostics = sortedDiagnostics;
    optionInventoryDiagnosticsTruncated = traversal.diagnosticsTruncated;
    inherit origins;
  };

  withPayload = payload: carrier // {
    # Keep the system derivation identity and expose only inspector metadata.
    meta = { crystalForgeInspector = payload; };
  };

  provenanceJob = carrier // {
    meta = { crystalForgeProvenance = provenance.provenance; };
  };

  jobs = [
    {
      name = "__crystalForgeConfigIndex";
      value = withPayload indexPayload;
    }
    {
      name = "__crystalForgeProvenance";
      value = provenanceJob;
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
  if builtins.length optionKeys != builtins.length (builtins.attrNames optionKeySet) then
    throw "Config inspector option-key collision"
  else
    builtins.listToAttrs jobs
