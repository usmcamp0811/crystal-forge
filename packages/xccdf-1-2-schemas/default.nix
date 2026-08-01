{ pkgs, ... }:
pkgs.stdenvNoCC.mkDerivation {
  pname = "xccdf-1.2-schemas";
  version = "1.2.1";
  dontUnpack = true;
  dontBuild = true;
  installPhase = ''
    schema_root="${pkgs.openscap}/share/openscap/schemas"
    mkdir -p "$out/common" "$out/cpe/2.3" "$out/xccdf/1.2"
    cp "$schema_root/common/xml.xsd" "$out/common/"
    cp "$schema_root/cpe/2.3/cpe-naming_2.3.xsd" "$out/cpe/2.3/"
    cp "$schema_root/xccdf/1.2/xccdf_1.2.xsd" "$out/xccdf/1.2/"
    cp "$schema_root/xccdf/1.2/cpe-language_2.3.xsd" "$out/xccdf/1.2/"
  '';
  meta = {
    description = "Vendored XCCDF 1.2.1 schema set from OpenSCAP";
  };
}
