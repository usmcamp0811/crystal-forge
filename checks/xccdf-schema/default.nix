{ pkgs, ... }:
let
  xccdfSchemas = pkgs.crystal-forge.xccdf-1-2-schemas;
  cfSchema = ../../schemas/cf-xccdf-1/cf-xccdf-1.xsd;
  writerFixture = pkgs.crystal-forge.default.server;
in
pkgs.runCommand "xccdf-schema-validation" {
  nativeBuildInputs = [ pkgs.libxml2 pkgs.openscap writerFixture ];
  meta = {
    description = "Validate vendored XCCDF 1.2 and CF-XCCDF v0.1 schemas against comprehensive writer-generated fixtures, plus OpenSCAP validation";
  };
} ''
  # --- 1. Comprehensive Benchmark fixture (valid against XCCDF 1.2) ---
  # Covers: status, title, description, version, metadata with cf:bundle,
  # Profile with select, Group, Rule with check/fix, ident/reference.
  # CF extension elements inside Rules are NOT valid children per XCCDF 1.2
  # schema (no ##any extension point), so they are omitted here and validated
  # separately against the CF extension schema.
  #
  # Element order in a Rule (from selectableItemType + ruleType):
  #   title, description, warning, question, reference, metadata, rationale,
  #   platform, requires, conflicts, ident, impact-metric, profile-note,
  #   fixtext, fix, check/complex-check, signature
  #
  # ID patterns: xccdf_[^_]+_rule_.+ (second segment must be a single word)
  cat > benchmark.xml <<'XML'
  <?xml version="1.0" encoding="UTF-8"?>
  <Benchmark xmlns="http://checklists.nist.gov/xccdf/1.2"
             xmlns:cf="urn:crystal-forge:xccdf:1"
             id="xccdf_crystalforge_benchmark_comprehensive"
             resolved="false"
             xml:lang="en">
    <status>draft</status>
    <title>Crystal Forge Comprehensive Writer Fixture</title>
    <description>
      <p xmlns="http://www.w3.org/1999/xhtml">
        A comprehensive XCCDF 1.2 Benchmark fixture exercising CF namespace
        extensions in metadata, Profile selection, Group/Rule structure,
        and standard check/fix elements.
      </p>
    </description>
    <version>1.0.0</version>
    <metadata>
      <cf:bundle schema-version="1"
                 bundle-id="urn:uuid:a1b2c3d4-e5f6-7890-abcd-ef1234567890"
                 bundle-version-id="urn:uuid:12345678-abcd-ef01-2345-6789abcdef01"
                 publication-state="draft">
        <cf:framework name="NIST SP 800-53" version="5.1"/>
        <cf:layer>nixos</cf:layer>
        <cf:owner>crystal-forge</cf:owner>
        <cf:content-digest algorithm="sha-256" canonical-model="cf-model-json-1">abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789</cf:content-digest>
      </cf:bundle>
    </metadata>
    <Profile id="xccdf_crystalforge_profile_default" extends="xccdf_crystalforge_profile_base">
      <title>Default Crystal Forge Profile</title>
      <description>
        <p xmlns="http://www.w3.org/1999/xhtml">Default selection of rules for Crystal Forge benchmarks.</p>
      </description>
      <select idref="xccdf_crystalforge_rule_firewall" selected="true"/>
      <select idref="xccdf_crystalforge_rule_custom" selected="true"/>
      <select idref="xccdf_crystalforge_rule_ssh" selected="true"/>
    </Profile>
    <Profile id="xccdf_crystalforge_profile_base">
      <title>Base Crystal Forge Profile</title>
      <description>
        <p xmlns="http://www.w3.org/1999/xhtml">Base profile with no rules selected.</p>
      </description>
      <select idref="xccdf_crystalforge_rule_firewall" selected="false"/>
      <select idref="xccdf_crystalforge_rule_custom" selected="false"/>
      <select idref="xccdf_crystalforge_rule_ssh" selected="false"/>
    </Profile>
    <Group id="xccdf_crystalforge_group_network">
      <title>Network Configuration</title>
      <description>
        <p xmlns="http://www.w3.org/1999/xhtml">Rules related to network and firewall configuration.</p>
      </description>
      <Rule id="xccdf_crystalforge_rule_firewall" role="full" severity="high">
        <title>Enable Firewall</title>
        <description>
          <p xmlns="http://www.w3.org/1999/xhtml">The host firewall must be enabled to filter inbound traffic.</p>
        </description>
        <reference href="https://nvd.nist.gov/vuln/detail/CVE-2024-0001">NIST Example Reference</reference>
        <rationale>
          <p xmlns="http://www.w3.org/1999/xhtml">An enabled firewall reduces the attack surface of the system.</p>
        </rationale>
        <ident system="https://crystal-forge.org/ids">CF-001</ident>
        <fix id="xccdf_crystalforge_fix_firewall" reboot="false" strategy="unknown">
          networking.firewall.enable = true;
        </fix>
        <check system="urn:xccdf:check-engine:crystal-forge">
          <check-content-ref href="#xccdf_crystalforge_cref_firewall" name="crystal-forge"/>
        </check>
      </Rule>
      <Rule id="xccdf_crystalforge_rule_ssh" role="full" severity="medium">
        <title>SSH Hardening</title>
        <description>
          <p xmlns="http://www.w3.org/1999/xhtml">SSH must be configured with hardened defaults.</p>
        </description>
        <reference href="https://infosec.mozilla.org/guidelines/openssh">Mozilla OpenSSH Guidelines</reference>
        <ident system="https://crystal-forge.org/ids">CF-002</ident>
        <fix id="xccdf_crystalforge_fix_ssh" reboot="false" strategy="unknown">
          services.openssh.settings.PermitRootLogin = "no";
        </fix>
        <check system="urn:xccdf:check-engine:crystal-forge">
          <check-content-ref href="#xccdf_crystalforge_cref_ssh" name="crystal-forge"/>
        </check>
      </Rule>
    </Group>
    <Group id="xccdf_crystalforge_group_application">
      <title>Application Configuration</title>
      <description>
        <p xmlns="http://www.w3.org/1999/xhtml">Rules related to application-level configuration.</p>
      </description>
      <Rule id="xccdf_crystalforge_rule_custom" role="full" severity="high">
        <title>Custom Application Check</title>
        <description>
          <p xmlns="http://www.w3.org/1999/xhtml">Application-specific check using CF custom-check implementation.</p>
        </description>
        <reference href="https://example.com/app-spec">Example Application Specification</reference>
        <ident system="https://crystal-forge.org/ids">CF-003</ident>
        <check system="urn:xccdf:check-engine:crystal-forge">
          <check-content-ref href="#xccdf_crystalforge_cref_custom" name="crystal-forge"/>
        </check>
      </Rule>
    </Group>
  </Benchmark>
  XML

  # --- 6. Actual writer output ---
  # This is generated by the same write_bundle_xccdf_export() function used by
  # the API endpoint, not by a hand-written approximation.
  xccdf-export-fixture > generated-writer-output.xml

  # --- 2. CF extension: require-crystal-forge-agent policy ---
  cat > cf-policy-agent.xml <<'XML'
  <?xml version="1.0" encoding="UTF-8"?>
  <cf:policy xmlns:cf="urn:crystal-forge:xccdf:1" schema-version="1">
    <cf:execution phase="nix-evaluation" strict="true"/>
    <cf:implementation state="native">
      <cf:require-crystal-forge-agent/>
    </cf:implementation>
    <cf:config-json>{}</cf:config-json>
    <cf:compliance-metadata-json>{}</cf:compliance-metadata-json>
    <cf:dependencies-json>[]</cf:dependencies-json>
  </cf:policy>
  XML

  # --- 3. CF extension: custom-check policy ---
  cat > cf-policy-custom-check.xml <<'XML'
  <?xml version="1.0" encoding="UTF-8"?>
  <cf:policy xmlns:cf="urn:crystal-forge:xccdf:1" schema-version="1">
    <cf:execution phase="nix-evaluation" strict="true"/>
    <cf:implementation state="native">
      <cf:custom-check mode="all" context="nixos-configuration-v1" binding="cfg">
        <cf:rule field-name="firewallEnabled" strict="true">
          <cf:description>Check that the host firewall is enabled in the NixOS configuration.</cf:description>
          <cf:expression language="nix">cfg.config.networking.firewall.enable</cf:expression>
        </cf:rule>
      </cf:custom-check>
    </cf:implementation>
    <cf:config-json>{"mode":"all"}</cf:config-json>
    <cf:compliance-metadata-json>{}</cf:compliance-metadata-json>
    <cf:dependencies-json>[]</cf:dependencies-json>
  </cf:policy>
  XML

  # --- 4. CF extension: policy-identity ---
  cat > cf-policy-identity.xml <<'XML'
  <?xml version="1.0" encoding="UTF-8"?>
  <cf:policy-identity xmlns:cf="urn:crystal-forge:xccdf:1"
                      policy-id="urn:uuid:b3e4f5a6-c7d8-9012-3456-789abcdef012"
                      policy-version-id="urn:uuid:c4d5e6f7-a8b9-0123-4567-89abcdef0123"
                      publication-state="accepted"
                      enabled-default="true"
                      implementation-state="native"
                      selected="true"
                      policy-order="1">
    <cf:policy-version>1.0.0</cf:policy-version>
    <cf:content-digest algorithm="sha-256" canonical-model="cf-model-json-1">fedcba0987654321fedcba0987654321fedcba0987654321fedcba0987654321</cf:content-digest>
  </cf:policy-identity>
  XML

  # --- 5. Full combined writer-shaped output ---
  cat > full-writer-output.xml <<'XML'
  <?xml version="1.0" encoding="UTF-8"?>
  <Benchmark xmlns="http://checklists.nist.gov/xccdf/1.2"
             xmlns:cf="urn:crystal-forge:xccdf:1"
             id="xccdf_crystalforge_benchmark_full"
             resolved="false"
             xml:lang="en">
    <status>draft</status>
    <title>Crystal Forge Full Writer Output</title>
    <description>
      <p xmlns="http://www.w3.org/1999/xhtml">
        Complete writer-shaped document combining XCCDF structure with
        CF extension elements inside Rule metadata extension points.
      </p>
    </description>
    <version>1.0.0</version>
    <metadata>
      <cf:bundle schema-version="1"
                 bundle-id="urn:uuid:d5e6f7a8-b9c0-1234-5678-9abcdef01234"
                 bundle-version-id="urn:uuid:e6f7a8b9-c0d1-2345-6789-abcdef012345"
                 publication-state="draft">
        <cf:framework name="NIST SP 800-53" version="5.1"/>
        <cf:layer>nixos</cf:layer>
        <cf:owner>crystal-forge</cf:owner>
        <cf:content-digest algorithm="sha-256" canonical-model="cf-model-json-1">1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef</cf:content-digest>
      </cf:bundle>
    </metadata>
    <Profile id="xccdf_crystalforge_profile_full" extends="xccdf_crystalforge_profile_base">
      <title>Full Writer Output Profile</title>
      <select idref="xccdf_crystalforge_rule_fw" selected="true"/>
      <select idref="xccdf_crystalforge_rule_cc" selected="true"/>
    </Profile>
    <Profile id="xccdf_crystalforge_profile_base">
      <title>Base Profile</title>
      <select idref="xccdf_crystalforge_rule_fw" selected="false"/>
      <select idref="xccdf_crystalforge_rule_cc" selected="false"/>
    </Profile>
    <Group id="xccdf_crystalforge_group_full">
      <title>Full Writer Output Group</title>
      <Rule id="xccdf_crystalforge_rule_fw" role="full" severity="high" selected="true" weight="10.0">
        <title>Enable Firewall (require-crystal-forge-agent)</title>
        <description>
          <p xmlns="http://www.w3.org/1999/xhtml">Firewall rule with CF policy embedded in Rule.</p>
        </description>
        <reference href="https://example.com/firewall">Firewall Reference</reference>
        <reference href="https://example.com/firewall">Firewall Reference</reference>
        <metadata>
        <cf:policy-identity xmlns:cf="urn:crystal-forge:xccdf:1"
                            policy-id="urn:uuid:f7a8b9c0-d1e2-3456-789a-bcdef0123456"
                            policy-version-id="urn:uuid:a8b9c0d1-e2f3-4567-89ab-cdef01234567"
                            publication-state="accepted"
                            enabled-default="true"
                            implementation-state="native"
                            selected="true"
                            policy-order="1">
          <cf:policy-version>1.0.0</cf:policy-version>
          <cf:content-digest algorithm="sha-256" canonical-model="cf-model-json-1">abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789</cf:content-digest>
        </cf:policy-identity>
        <cf:policy xmlns:cf="urn:crystal-forge:xccdf:1" schema-version="1">
          <cf:execution phase="nix-evaluation" strict="true"/>
          <cf:implementation state="native">
            <cf:require-crystal-forge-agent/>
          </cf:implementation>
          <cf:config-json>{}</cf:config-json>
          <cf:compliance-metadata-json>{}</cf:compliance-metadata-json>
          <cf:dependencies-json>[]</cf:dependencies-json>
        </cf:policy>
        </metadata>
        <ident system="https://crystal-forge.org/ids">CF-FW-001</ident>
        <check system="urn:xccdf:check-engine:crystal-forge">
          <check-content-ref href="#xccdf_crystalforge_cref_firewall" name="crystal-forge"/>
        </check>
      </Rule>
      <Rule id="xccdf_crystalforge_rule_cc" role="full" severity="high" selected="true" weight="10.0">
        <title>Custom Application Check (custom-check)</title>
        <description>
          <p xmlns="http://www.w3.org/1999/xhtml">Rule with CF custom-check policy embedded in Rule.</p>
        </description>
        <reference href="https://example.com/app-spec">Application Specification</reference>
        <reference href="https://example.com/app-spec">Application Specification</reference>
        <metadata>
        <cf:policy-identity xmlns:cf="urn:crystal-forge:xccdf:1"
                            policy-id="urn:uuid:b9c0d1e2-f3a4-5678-9abc-def012345678"
                            policy-version-id="urn:uuid:c0d1e2f3-a4b5-6789-abcd-ef0123456789"
                            publication-state="draft"
                            enabled-default="true"
                            implementation-state="native"
                            selected="true"
                            policy-order="2">
          <cf:policy-version>1.0.0</cf:policy-version>
          <cf:content-digest algorithm="sha-256" canonical-model="cf-model-json-1">0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef</cf:content-digest>
        </cf:policy-identity>
        <cf:policy xmlns:cf="urn:crystal-forge:xccdf:1" schema-version="1">
          <cf:execution phase="nix-evaluation" strict="true"/>
          <cf:implementation state="native">
            <cf:custom-check mode="all" context="nixos-configuration-v1" binding="cfg">
              <cf:rule field-name="appEnabled" strict="true">
                <cf:description>Check that the application is enabled.</cf:description>
                <cf:expression language="nix">cfg.config.services.app.enable</cf:expression>
              </cf:rule>
          </cf:custom-check>
          </cf:implementation>
          <cf:config-json>{"mode":"all"}</cf:config-json>
          <cf:compliance-metadata-json>{}</cf:compliance-metadata-json>
          <cf:dependencies-json>[]</cf:dependencies-json>
        </cf:policy>
        </metadata>
        <ident system="https://crystal-forge.org/ids">CF-APP-001</ident>
        <check system="urn:xccdf:check-engine:crystal-forge">
          <check-content-ref href="#xccdf_crystalforge_cref_custom" name="crystal-forge"/>
        </check>
      </Rule>
    </Group>
  </Benchmark>
  XML

  # --- Validation: XCCDF 1.2 Benchmark (comprehensive fixture) ---
  xmllint --noout --schema ${xccdfSchemas}/xccdf/1.2/xccdf_1.2.xsd benchmark.xml

  # --- Validation: CF extension elements against CF-XCCDF schema ---
  xmllint --noout --schema ${cfSchema} cf-policy-agent.xml
  xmllint --noout --schema ${cfSchema} cf-policy-custom-check.xml
  xmllint --noout --schema ${cfSchema} cf-policy-identity.xml

  # --- Validation: Full writer-shaped output against XCCDF 1.2 ---
  # CF elements are inside Rule metadata, the XCCDF extension point, so the
  # complete document is schema-valid rather than merely well-formed.
  xmllint --noout --schema ${xccdfSchemas}/xccdf/1.2/xccdf_1.2.xsd full-writer-output.xml

  xmllint --noout --schema ${xccdfSchemas}/xccdf/1.2/xccdf_1.2.xsd generated-writer-output.xml
  test "$(xmllint --xpath 'count(//*[local-name()="check-content"]/*[local-name()="policy"])' generated-writer-output.xml)" = "1"

  # Validate the CF extension nodes extracted from the actual writer output
  # against the CF extension schema as well. The XCCDF schema validates the
  # surrounding Benchmark and Rule content model; these checks validate the
  # extension payloads themselves.
  xmllint --xpath '//*[local-name()="bundle" and namespace-uri()="urn:crystal-forge:xccdf:1"]' generated-writer-output.xml > generated-bundle.xml
  xmllint --xpath '//*[local-name()="policy-identity" and namespace-uri()="urn:crystal-forge:xccdf:1"]' generated-writer-output.xml > generated-policy-identity.xml
  xmllint --xpath '//*[local-name()="policy" and namespace-uri()="urn:crystal-forge:xccdf:1"]' generated-writer-output.xml > generated-policy.xml
  xmllint --xpath '//*[local-name()="source-mappings" and namespace-uri()="urn:crystal-forge:xccdf:1"]' generated-writer-output.xml > generated-source-mappings.xml
  xmllint --xpath '//*[local-name()="opaque-xml" and namespace-uri()="urn:crystal-forge:xccdf:1"]' generated-writer-output.xml > generated-opaque.xml
  sed -i 's#<cf:bundle#<cf:bundle xmlns:cf="urn:crystal-forge:xccdf:1"#' generated-bundle.xml
  sed -i 's#<cf:policy-identity#<cf:policy-identity xmlns:cf="urn:crystal-forge:xccdf:1"#' generated-policy-identity.xml
  sed -i 's#<cf:policy#<cf:policy xmlns:cf="urn:crystal-forge:xccdf:1"#' generated-policy.xml
  sed -i 's#<cf:source-mappings#<cf:source-mappings xmlns:cf="urn:crystal-forge:xccdf:1"#' generated-source-mappings.xml
  sed -i 's#<cf:opaque-xml#<cf:opaque-xml xmlns:cf="urn:crystal-forge:xccdf:1"#' generated-opaque.xml
  xmllint --noout --schema ${cfSchema} generated-bundle.xml
  xmllint --noout --schema ${cfSchema} generated-policy-identity.xml
  xmllint --noout --schema ${cfSchema} generated-policy.xml
  xmllint --noout --schema ${cfSchema} generated-source-mappings.xml
  xmllint --noout --schema ${cfSchema} generated-opaque.xml

  # --- OpenSCAP validation ---
  # Validate the comprehensive Benchmark fixture against OpenSCAP's built-in
  # XCCDF 1.2 schema. This catches issues that xmllint alone may miss (e.g.,
  # subtle content model violations, attribute datatype checks).
  oscap xccdf validate benchmark.xml
  oscap xccdf validate generated-writer-output.xml

  # Show OpenSCAP info for the comprehensive fixture to confirm readability
  oscap info benchmark.xml
  oscap info generated-writer-output.xml

  touch "$out"
''
