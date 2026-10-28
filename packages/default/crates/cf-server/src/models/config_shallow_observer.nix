{ configuration
, operation
, path ? [ ]
, childOffset ? 0
, encodeValue ? (_depth: _declaredType: value: value)
}:

let
  options = configuration.options;
  maxItems = 512;
  min = left: right: if left < right then left else right;
  slice = offset: count: values:
    builtins.genList (index: builtins.elemAt values (offset + index)) count;
  optionKey = components: builtins.hashString "sha256" (builtins.toJSON components);
  resolve = builtins.foldl' (node: component: builtins.getAttr component node) options path;

  childrenFor = prefix: node:
    let
      names = builtins.attrNames node;
      remaining = if builtins.length names > childOffset
        then builtins.length names - childOffset
        else 0;
      retained = slice childOffset (min maxItems remaining) names;
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
      retained = slice 0 (min maxItems (builtins.length definitions)) definitions;
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
in
  if operation == "root" then childrenFor [ ] options
  else if operation == "prefix" then childrenFor path resolve
  else if operation == "option" then detailFor resolve
  else if operation == "provenance" then provenanceFor resolve
  else throw "Unsupported shallow Config observation operation"
