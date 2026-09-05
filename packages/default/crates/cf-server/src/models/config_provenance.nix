{ flake, configuration }:

let
  lib = configuration.pkgs.lib;
  carrier = configuration.config.system.build.toplevel;
  provenanceAdapterVersion = 1;

  unavailable = reasonCode: {
    adapterVersion = provenanceAdapterVersion;
    supported = false;
    inherit reasonCode;
  };

  moduleArgs = configuration._module.args or { };
  moduleType = moduleArgs.moduleType or null;
  payload =
    if moduleType != null && moduleType ? functor
    then moduleType.functor.payload or null
    else null;

  helperNames = [
    "collectModules"
    "pushDownProperties"
    "dischargeProperties"
    "filterOverrides'"
    "sortProperties"
  ];

  moduleTypePayloadAvailable =
    payload != null
    && payload ? modules
    && payload ? specialArgs
    && payload ? class
    && builtins.isList payload.modules;

  helperCapabilitiesAvailable =
    moduleTypePayloadAvailable
    && builtins.all (name: builtins.hasAttr name lib.modules) helperNames;

  priorityOf = definition:
    (lib.modules.filterOverrides' [ definition ]).highestPrio;

  getAt = path: value:
    if path == [ ] then
      { found = true; inherit value; }
    else if builtins.isAttrs value && builtins.hasAttr (builtins.head path) value then
      getAt (builtins.tail path) (builtins.getAttr (builtins.head path) value)
    else
      { found = false; value = null; };

  optionAt = path:
    let result = getAt path configuration.options;
    in result.found && builtins.isAttrs result.value
      && (result.value._type or null) == "option";

  selfTest =
    if !helperCapabilitiesAvailable then false
    else
      let
        selfModules = [
          ({ lib, ... }:
            {
              config = lib.mkIf false {
                crystalForgeProvenanceSelfTest.falseBranch = "must-not-survive";
              };
            })
          {
            config = lib.mkMerge [
              { crystalForgeProvenanceSelfTest.order = lib.mkOrder 200 "late"; }
              { crystalForgeProvenanceSelfTest.order = lib.mkOrder 100 "early"; }
            ];
          }
        ];
        context = ({
          inherit lib;
          options = { };
          specialArgs = payload.specialArgs;
          _class = payload.class;
          _prefix = [ ];
          config = { };
        } // payload.specialArgs);
        collected = lib.modules.collectModules
          (payload.specialArgs.modulesPath or "")
          selfModules
          context;
        normalizedFunction = builtins.any
          (module: builtins.isAttrs module.config
            && (module.config._type or null) == "if"
            && module.config ? content
            && module.config.content ? crystalForgeProvenanceSelfTest)
          collected.modules;
        falseDischarged = lib.modules.dischargeProperties (lib.mkIf false "x") == [ ];
        priority = lib.modules.filterOverrides' [
          { file = "loser"; value = lib.mkOverride 500 "loser"; }
          { file = "winner"; value = lib.mkForce "winner"; }
        ];
        mergeContributions = lib.modules.pushDownProperties
          (builtins.elemAt selfModules 1).config;
        pushDownWorks = builtins.length mergeContributions == 2
          && builtins.all (property: property ? crystalForgeProvenanceSelfTest
            && property.crystalForgeProvenanceSelfTest ? order)
            mergeContributions;
        priorityWorks = priority.highestPrio == priorityOf {
          file = "winner";
          value = lib.mkForce "winner";
        }
          && builtins.length priority.values == 1
          && (builtins.head priority.values).file == "winner";
        order = lib.modules.sortProperties [
          { file = "late"; value = lib.mkOrder 200 "late"; }
          { file = "early"; value = lib.mkOrder 100 "early"; }
        ];
        orderWorks = map (definition: definition.value) order == [ "early" "late" ];
      in
        normalizedFunction && pushDownWorks && falseDischarged
          && priorityWorks && orderWorks;

  canonicalGraph = node: {
    key = node.key;
    file = node.file;
    disabled = node.disabled or false;
    imports = map canonicalGraph node.imports;
  };

  replayGraph =
    lib.modules.collectModules
      (payload.specialArgs.modulesPath or "")
      payload.modules
      ({
        inherit lib;
        options = configuration.options;
        specialArgs = payload.specialArgs;
        _class = payload.class;
        _prefix = [ ];
        config = configuration.config // { _module = configuration._module; };
      } // payload.specialArgs);

  graphMatches =
    payload != null
    && map canonicalGraph configuration.graph == map canonicalGraph replayGraph.graph;

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

  sourceOrigin = sourcePath:
    let
      matches = if sourcePath == null then [ ] else builtins.filter (entry:
        entry.out_path != null
        && (sourcePath == entry.out_path
          || builtins.substring 0 (builtins.stringLength entry.out_path + 1) sourcePath
            == entry.out_path + "/")) origins;
    in if matches == [ ] then { input = null; revision = null; }
       else
         let selected = builtins.foldl' (best: entry:
           if best == null || builtins.stringLength entry.out_path > builtins.stringLength best.out_path
           then entry else best) null matches;
         in { input = selected.name; revision = selected.revision; };

  walk = module: config:
    let
      walkNode = path: node:
        if optionAt path then
          let result = getAt path config;
          in if !result.found then [ ] else map (rawValue: {
            inherit path;
            source_path =
              if module._file or null == null then null else toString module._file;
            source_input = (sourceOrigin (module._file or null)).input;
            source_revision = (sourceOrigin (module._file or null)).revision;
            module_key = module.key or null;
            raw_value = rawValue;
          }) (lib.modules.dischargeProperties result.value)
        else
          let optionNode = getAt path configuration.options;
          in if optionNode.found && builtins.isAttrs optionNode.value && builtins.isAttrs node then
            builtins.concatLists (map (name:
              let child = getAt [ name ] optionNode.value;
              in if child.found then
                walkNode (path ++ [ name ]) (builtins.getAttr name node)
              else [ ]) (builtins.attrNames node))
           else [ ];
    in walkNode [ ] config;

  rawDefinitions = builtins.concatLists (map (module:
    builtins.concatLists (map (config: walk module config)
      (lib.modules.pushDownProperties module.config))
  ) replayGraph.modules);

  addGrouped = grouped: definition:
    let key = builtins.hashString "sha256" (builtins.toJSON definition.path);
    in grouped // {
      "${key}" = (grouped."${key}" or [ ]) ++ [ definition ];
    };

  groupedDefinitions = builtins.foldl' addGrouped { } rawDefinitions;

  findIndex = needle: values:
    if values == [ ] then null
    else if builtins.head values == needle then 0
    else
      let result = findIndex needle (builtins.tail values);
      in if result == null then null else result + 1;

  decorate = key: definitions:
    let
      numbered = builtins.genList (index: (builtins.elemAt definitions index) // {
        ordinal = index;
      }) (builtins.length definitions);
      moduleDefinitions = map (definition: {
        file = definition.source_path;
        value = definition.raw_value;
        ordinal = definition.ordinal;
      }) numbered;
      filtered = lib.modules.filterOverrides' moduleDefinitions;
      survivingOrdinals = map (definition: definition.ordinal) filtered.values;
      ordered = lib.modules.sortProperties filtered.values;
      mergeOrder = map (definition: definition.ordinal) ordered;
      optionPath = (builtins.head numbered).path;
    in {
      option_key = key;
      path = optionPath;
      definitions = map (definition:
        let
          priority = priorityOf {
            file = definition.source_path;
            value = definition.raw_value;
          };
          mergeIndex = findIndex definition.ordinal mergeOrder;
        in {
          source_path = definition.source_path;
          source_input = definition.source_input;
          source_revision = definition.source_revision;
          module_key = definition.module_key;
          ordinal = definition.ordinal;
          inherit priority;
          status = if builtins.elem definition.ordinal survivingOrdinals
            then "active_surviving" else "priority_discarded";
          surviving_merge_order =
            if builtins.elem definition.ordinal survivingOrdinals then mergeIndex else null;
        }) numbered;
    };

  definitionsByOption = map (key: decorate key groupedDefinitions."${key}")
    (builtins.attrNames groupedDefinitions);

  supportedPayload = {
    adapterVersion = provenanceAdapterVersion;
    supported = true;
    targetLibVersion = configuration.pkgs.lib.version or null;
    targetModuleSystemPath = toString (configuration.pkgs.path or "");
    graphReplay = {
      actualNodeCount = builtins.length (map canonicalGraph configuration.graph);
      replayNodeCount = builtins.length (map canonicalGraph replayGraph.graph);
      equal = graphMatches;
    };
    seedEvidence = {
      hiddenPkgsModulePresent = builtins.any (module:
        builtins.isAttrs module
        && (module._file or null) != null
        && builtins.baseNameOf (toString module._file) == "eval-config.nix"
        && (module.key or null) == module._file) payload.modules;
      hiddenModulesModulePresent = builtins.any (module:
        builtins.isAttrs module
        && module ? config
        && module.config ? _module
        && module.config._module ? args
        && builtins.elem "noUserModules" (builtins.attrNames module.config._module.args)) payload.modules;
      modulesLocationSources = lib.unique (map (module: toString module._file) (builtins.filter (module:
        builtins.isAttrs module
        && module ? imports
        && (module._file or null) != null) payload.modules));
    };
    inherit definitionsByOption;
  };

  checked = builtins.tryEval (
    if !moduleTypePayloadAvailable then unavailable "module_type_payload_unavailable"
    else if !helperCapabilitiesAvailable then unavailable "helper_capability_unavailable"
    else if !selfTest then unavailable "capability_self_test_failed"
    else if !graphMatches then unavailable "graph_replay_mismatch"
    else supportedPayload
  );

  result = if checked.success then checked.value
    else unavailable "adapter_evaluation_failed";
in
carrier // {
  meta = {
    crystalForgeProvenance = result;
  };
}
