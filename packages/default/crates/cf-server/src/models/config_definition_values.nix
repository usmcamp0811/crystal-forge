{ flake, configuration, targetKey, allowedOptionKeys ? null, allowedOptionPaths ? null, provenanceLib, encodeValue }:

let
  lib = configuration.pkgs.lib;
  carrier = configuration.config.system.build.toplevel;
  adapterVersion = 1;
  getAt = path: value:
    if path == [ ] then
      { found = true; inherit value; }
    else if builtins.isAttrs value && builtins.hasAttr (builtins.head path) value then
      getAt (builtins.tail path) (builtins.getAttr (builtins.head path) value)
    else
      { found = false; value = null; };

  optionFor = path:
    let result = getAt path configuration.options;
    in if result.found then result.value
       else throw "definition value option path is missing";

  allowedOptionKeySet = if allowedOptionKeys == null then null else
    builtins.listToAttrs (map (name: { inherit name; value = true; }) allowedOptionKeys);
  allowedOptionPathKeys = if allowedOptionPaths == null then null else
    map (path: builtins.hashString "sha256" (builtins.toJSON path)) allowedOptionPaths;
  allowedSelectionValid = allowedOptionKeys == null && allowedOptionPaths == null
    || allowedOptionKeys != null && allowedOptionPaths != null
      && builtins.length allowedOptionKeys == builtins.length allowedOptionPaths
      && builtins.length allowedOptionKeys == builtins.length (builtins.attrNames allowedOptionKeySet)
      && allowedOptionKeys == allowedOptionPathKeys;
  mergeOptionTrees = trees:
    let count = builtins.length trees;
    in if count == 0 then { }
       else if count == 1 then builtins.head trees
       else
         let midpoint = count / 2;
         in lib.recursiveUpdate
           (mergeOptionTrees (lib.take midpoint trees))
           (mergeOptionTrees (lib.drop midpoint trees));
  selectedOptions = if allowedOptionPaths == null then configuration.options else
    mergeOptionTrees (map (path:
      lib.setAttrByPath path (optionFor path)
    ) allowedOptionPaths);
  provenanceConfiguration = configuration // { options = selectedOptions; };
  provenance = provenanceLib { inherit flake; configuration = provenanceConfiguration; };
  selectedDefinitions = if allowedOptionKeys == null
    then provenance.rawDefinitionsByOption
    else builtins.filter
      (option: builtins.hasAttr option.option_key allowedOptionKeySet)
      provenance.rawDefinitionsByOption;

  normalizeDefinition = option: definition:
    (lib.modules.mergeDefinitions option.path option.type [ {
      file = definition.source_path or "<unknown-definition-source>";
      value = definition.raw_value;
    } ]).mergedValue;

  numberDefinitions = definitions:
    builtins.genList (ordinal:
      (builtins.elemAt definitions ordinal) // { inherit ordinal; }
    ) (builtins.length definitions);

  withPayload = payload: carrier // {
    meta = { crystalForgeDefinitionValues = payload; };
  };

  unsupportedIndex = {
    kind = "definition_index";
    targetKey = targetKey;
    sourceOutPath = flake.outPath;
    adapterVersion = provenance.provenance.adapterVersion or adapterVersion;
    supported = false;
    reasonCode = provenance.provenance.reasonCode or "adapter_unsupported";
    definitionCount = 0;
    definitions = [ ];
  };

  supportedIndex = {
    kind = "definition_index";
    targetKey = targetKey;
    sourceOutPath = flake.outPath;
    adapterVersion = provenance.provenance.adapterVersion;
    supported = true;
    provenanceDigest = provenance.provenance.provenanceDigest;
    definitionCount = builtins.length (builtins.concatLists (map (option:
      option.definitions) selectedDefinitions));
    definitions = builtins.concatLists (map (option:
      map (definition: {
        option_key = option.option_key;
        ordinal = definition.ordinal;
       }) (numberDefinitions option.definitions)
    ) selectedDefinitions);
  };

  indexPayload = if provenance.provenance.supported or false
    then supportedIndex else unsupportedIndex;

  valueJobs = if provenance.provenance.supported or false then
    builtins.concatLists (map (option:
      let selectedOption = optionFor option.path;
      in map (definition:
        let normalized = normalizeDefinition selectedOption definition;
        in {
          name = "def_value_${option.option_key}_${toString definition.ordinal}";
          value = withPayload {
            kind = "definition_value";
            option_key = option.option_key;
            ordinal = definition.ordinal;
             value = encodeValue 0 (selectedOption.type.name or "unknown") normalized;
          };
         }) (numberDefinitions option.definitions)
    ) selectedDefinitions)
  else [ ];

  valueJobNames = map (job: job.name) valueJobs;
  # INVARIANT: Use builtins here because a target configuration can expose a
  # package-set `lib.unique` with a different contract.
  uniqueValueJobNames = builtins.attrNames (builtins.listToAttrs (map (name: {
    inherit name;
    value = true;
  }) valueJobNames));
in
if !allowedSelectionValid then
  throw "Config definition-value allowed option identities are invalid"
else if builtins.length valueJobNames != builtins.length uniqueValueJobNames then
  throw "Config definition-value job identity collision"
else
  builtins.listToAttrs ([
    {
      name = "__crystalForgeDefinitionIndex";
      value = withPayload indexPayload;
    }
  ] ++ valueJobs)
