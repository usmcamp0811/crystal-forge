lib:
  let
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
      else if builtins.isAttrs value && lib.isDerivation value then {
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
  in encodeValue
