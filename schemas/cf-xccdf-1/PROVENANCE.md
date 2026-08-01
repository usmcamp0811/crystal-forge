# CF-XCCDF v0.1 schema provenance

`cf-xccdf-1.xsd` is maintained by Crystal Forge for the frozen extension namespace
`urn:crystal-forge:xccdf:1`. Its normative source is the CF-XCCDF v0.1 profile at
`docs/design/CrystalForge/docs/crystal-forge-xccdf-interchange-profile-v0.1.md`.

The accompanying XCCDF 1.2.1 schema set is packaged as
`packages/xccdf-1-2-schemas`. It copies the XCCDF, CPE language, and XML namespace
schemas shipped by the pinned OpenSCAP package into a self-contained Nix output. The
upstream XCCDF schema identifies NIST IR 7275 Revision 4 and the schema date as
2012-02-23. Crystal Forge does not fetch schemas at runtime.
