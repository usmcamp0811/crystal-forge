{ lib, pkgs, ... }:
let
  package = pkgs.crystal-forge.nixos-options-metadata;
  metadata = package.metadata;
  paths = map (entry: entry.path) metadata;
  get = path: builtins.head (builtins.filter (entry: entry.path == path) metadata);
  boolean = get "networking.firewall.enable";
  enum = get "networking.networkmanager.dns";
  integer = get "boot.consoleLogLevel";
  string = get "networking.hostName";
  lines = get "networking.extraHosts";
in
assert paths == lib.sort builtins.lessThan paths;
assert boolean.value_type == "boolean";
assert enum.value_type == "enum" && enum.enum_values != [ ];
assert integer.value_type == "integer";
assert string.value_type == "string";
assert lines.value_type == "lines";
pkgs.runCommand "nixos-options-metadata-check" { } ''
  test -s ${package}/share/crystal-forge/nixos-options.json
  touch "$out"
''
