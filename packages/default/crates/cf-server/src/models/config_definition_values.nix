{ flake, configuration, targetKey, provenanceLib, encodeValue }:

let
  lib = configuration.pkgs.lib;
  carrier = configuration.config.system.build.toplevel;
  adapterVersion = 1;
  provenance = provenanceLib { inherit flake configuration; };

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
    adapterVersion = provenance.provenance.adapterVersion or adapterVersion;
    supported = false;
    reasonCode = provenance.provenance.reasonCode or "adapter_unsupported";
    definitionCount = 0;
    definitions = [ ];
  };

  supportedIndex = {
    kind = "definition_index";
    targetKey = targetKey;
    adapterVersion = provenance.provenance.adapterVersion;
    supported = true;
    provenanceDigest = provenance.provenance.provenanceDigest;
    definitionCount = builtins.length (builtins.concatLists (map (option:
      option.definitions) provenance.rawDefinitionsByOption));
    definitions = builtins.concatLists (map (option:
      map (definition: {
        option_key = option.option_key;
        ordinal = definition.ordinal;
       }) (numberDefinitions option.definitions)
    ) provenance.rawDefinitionsByOption);
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
    ) provenance.rawDefinitionsByOption)
  else [ ];

  valueJobNames = map (job: job.name) valueJobs;
in
if builtins.length valueJobNames != builtins.length (lib.unique valueJobNames) then
  throw "Config definition-value job identity collision"
else
  builtins.listToAttrs ([
    {
      name = "__crystalForgeDefinitionIndex";
      value = withPayload indexPayload;
    }
  ] ++ valueJobs)
