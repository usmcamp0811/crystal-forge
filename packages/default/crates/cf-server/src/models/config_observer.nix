{ flake, configuration, targetKey, operation, path, childOffset ? 0, encodeValue }:

let
  lib = configuration.pkgs.lib;
  carrier = configuration.config.system.build.toplevel;
  options = configuration.options;
  optionKey = components: builtins.hashString "sha256" (builtins.toJSON components);
  withPayload = payload: carrier // { meta.crystalForgeConfigObservation = payload; };
  resolve = builtins.foldl' (node: component: builtins.getAttr component node) options path;
  maxItems = 512;

  childrenFor = prefix: node:
    let
      names = builtins.attrNames node;
      remaining = lib.max 0 (builtins.length names - childOffset);
      retained = lib.sublist childOffset (lib.min maxItems remaining) names;
      childFor = name:
        let
          childPath = prefix ++ [ name ];
          childAttempt = builtins.tryEval
            (let child = builtins.getAttr name node; in builtins.seq child child);
          child = childAttempt.value or null;
          typeAttempt = if childAttempt.success && builtins.isAttrs child
            then builtins.tryEval (child._type or null)
            else { success = false; value = null; };
        in if !childAttempt.success || !builtins.isAttrs child || !typeAttempt.success then {
          path_components = childPath;
          key = optionKey childPath;
          kind = "unavailable";
        } else {
          path_components = childPath;
          key = optionKey childPath;
          kind = if typeAttempt.value == "option" then "option" else "prefix";
        };
    in {
      kind = operation;
      path_components = prefix;
      child_offset = childOffset;
      children = map childFor retained;
      children_truncated = builtins.length names > childOffset + builtins.length retained;
      total_children = builtins.length names;
    };

  detailFor = option:
    let
      typeAttempt = builtins.tryEval (option.type.name or null);
      declaredType = if typeAttempt.success then typeAttempt.value else null;
      valueAttempt = builtins.tryEval (
        let encoded = encodeValue 0
          (if declaredType != null then declaredType else "unknown")
          option.value;
        in builtins.deepSeq encoded encoded
      );
    in {
      kind = "option";
      path_components = path;
      key = optionKey path;
      declared_type = declaredType;
      is_defined = option.isDefined or false;
      highest_prio = option.highestPrio or null;
      value = if valueAttempt.success then valueAttempt.value else {
        kind = "failed";
        value = {
          code = "value_unavailable";
          message = "Option value is unavailable";
        };
      };
    };

  provenanceFor = option:
    let
      definitions = option.definitionsWithLocations or [ ];
      retained = lib.sublist 0 (lib.min maxItems (builtins.length definitions)) definitions;
    in {
      kind = "provenance";
      path_components = path;
      key = optionKey path;
      definitions = map (definition: {
        source_path = definition.file or null;
        priority = definition.priority or null;
      }) retained;
      definitions_truncated = builtins.length definitions > builtins.length retained;
      total_definitions = builtins.length definitions;
    };

  emptyTraversal = { entries = [ ]; diagnostics = [ ]; };
  mergeTraversal = left: right: {
    entries = left.entries ++ right.entries;
    diagnostics = left.diagnostics ++ right.diagnostics;
  };
  walkChild = depth: prefix: node: name:
    let
      childPath = prefix ++ [ name ];
      attempt = builtins.tryEval
        (let child = builtins.getAttr name node; in builtins.seq child child);
      child = attempt.value or null;
      typeAttempt = if attempt.success && builtins.isAttrs child
        then builtins.tryEval (child._type or null)
        else { success = false; value = null; };
    in if attempt.success && typeAttempt.success && builtins.isAttrs child
      && typeAttempt.value == "option" then {
        entries = [ { path = childPath; option = child; key = optionKey childPath; } ];
        diagnostics = [ ];
      } else if !attempt.success || !builtins.isAttrs child || !typeAttempt.success then {
        entries = [ ];
        diagnostics = [ { path_components = childPath; code = "unreadable_option_subtree"; message = "Option subtree could not be inspected"; } ];
      } else walkOptions (depth + 1) childPath child;
  walkNames = depth: prefix: node: names:
    let count = builtins.length names;
    in if count == 0 then emptyTraversal
    else if count == 1 then walkChild depth prefix node (builtins.head names)
    else let midpoint = builtins.div count 2; in mergeTraversal
      (walkNames depth prefix node (lib.sublist 0 midpoint names))
      (walkNames depth prefix node (lib.sublist midpoint (count - midpoint) names));
  walkOptions = depth: prefix: node:
    let namesAttempt = builtins.tryEval (builtins.attrNames node); in
    if !namesAttempt.success then {
      entries = [ ];
      diagnostics = [ { path_components = prefix; code = "unreadable_option_subtree"; message = "Option subtree could not be inspected"; } ];
    } else if depth >= 16 then {
      entries = [ ];
      diagnostics = if namesAttempt.value == [ ] then [ ] else [ {
        path_components = prefix; code = "option_subtree_depth_exceeded"; message = "Option subtree exceeds the traversal depth limit";
      } ];
    } else walkNames depth prefix node namesAttempt.value;

  traversal = walkOptions 0 [ ] options;
  defaultPriority = (lib.mkOptionDefault null).priority;
  classifierFor = entry:
    let
      option = entry.option;
      highestPrio = option.highestPrio or null;
      survivorCount = builtins.length (option.definitions or [ ]);
      configured = (option.isDefined or false) && (
        !(option ? default)
        || (highestPrio != null && highestPrio < defaultPriority)
        || (highestPrio == defaultPriority && survivorCount > 1)
      );
    in withPayload {
      kind = "configured_classifier";
      path_components = entry.path;
      key = entry.key;
      inherit configured;
    };
  configuredJobs = [ {
    name = "__crystalForgeConfiguredIndex";
    value = withPayload {
      kind = "configured_index";
      path_components = [ ];
      total_traversed = builtins.length traversal.entries;
      diagnostics = lib.sublist 0 128 traversal.diagnostics;
      diagnostics_truncated = builtins.length traversal.diagnostics > 128;
    };
  } ] ++ map (entry: {
    name = "configured_${entry.key}";
    value = classifierFor entry;
  }) traversal.entries;

  jobs = if operation == "root" then [ {
    name = "observation";
    value = withPayload (childrenFor [ ] options);
  } ] else if operation == "prefix" then [ {
    name = "observation";
    value = withPayload (childrenFor path resolve);
  } ] else if operation == "option" then [ {
    name = "observation";
    value = withPayload (detailFor resolve);
  } ] else if operation == "provenance" then [ {
    name = "observation";
    value = withPayload (provenanceFor resolve);
  } ] else if operation == "configured_index" then configuredJobs
  else throw "Unsupported Config observation operation";
in
  builtins.listToAttrs jobs
