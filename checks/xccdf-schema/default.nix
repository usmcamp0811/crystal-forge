{ pkgs, ... }:
let
  xccdfSchemas = pkgs.crystal-forge.xccdf-1-2-schemas;
  cfSchema = ../../schemas/cf-xccdf-1/cf-xccdf-1.xsd;
in
pkgs.runCommand "xccdf-schema-validation" {
  nativeBuildInputs = [ pkgs.libxml2 ];
  meta = {
    description = "Validate vendored XCCDF 1.2 and CF-XCCDF v0.1 schemas without network access";
  };
} ''
  cat > benchmark.xml <<'XML'
  <?xml version="1.0" encoding="UTF-8"?>
  <Benchmark xmlns="http://checklists.nist.gov/xccdf/1.2"
      id="xccdf_org.crystalforge_benchmark_schema_fixture">
    <status>draft</status>
    <title>Crystal Forge schema fixture</title>
    <version>0.1.0</version>
  </Benchmark>
  XML

  cat > cf-policy.xml <<'XML'
  <?xml version="1.0" encoding="UTF-8"?>
  <cf:policy xmlns:cf="urn:crystal-forge:xccdf:1" schema-version="1">
    <cf:execution phase="nix-evaluation" strict="true"/>
    <cf:implementation>
      <cf:custom-check mode="all" context="nixos-configuration-v1" binding="cfg">
        <cf:rule field-name="firewallEnabled" strict="true">
          <cf:expression language="nix">cfg.config.networking.firewall.enable</cf:expression>
        </cf:rule>
      </cf:custom-check>
    </cf:implementation>
  </cf:policy>
  XML

  xmllint --noout --schema ${xccdfSchemas}/xccdf/1.2/xccdf_1.2.xsd benchmark.xml
  xmllint --noout --schema ${cfSchema} cf-policy.xml
  touch "$out"
''
