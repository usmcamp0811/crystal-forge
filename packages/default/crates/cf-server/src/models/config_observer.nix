{ flake, configuration, targetKey, operation, path, childOffset ? 0, encodeValue, shallowObserver }:

let
  lib = configuration.pkgs.lib;
  carrier = configuration.config.system.build.toplevel;
  options = configuration.options;
  optionKey = components: builtins.hashString "sha256" (builtins.toJSON components);
  withPayload = payload: carrier // { meta.crystalForgeConfigObservation = payload; };
  resolve = builtins.foldl' (node: component: builtins.getAttr component node) options path;
  maxItems = 512;

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
    value = withPayload (shallowObserver { inherit configuration operation path childOffset encodeValue; });
  } ] else if operation == "prefix" then [ {
    name = "observation";
    value = withPayload (shallowObserver { inherit configuration operation path childOffset encodeValue; });
  } ] else if operation == "option" then [ {
    name = "observation";
    value = withPayload (shallowObserver { inherit configuration operation path childOffset encodeValue; });
  } ] else if operation == "provenance" then [ {
    name = "observation";
    value = withPayload (shallowObserver { inherit configuration operation path childOffset encodeValue; });
  } ] else if operation == "configured_index" then configuredJobs
  else throw "Unsupported Config observation operation";
in
  builtins.listToAttrs jobs
