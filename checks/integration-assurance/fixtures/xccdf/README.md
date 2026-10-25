# Retained XCCDF inputs

The V1R1 and V1R2 Anduril NixOS STIG XML files are inputs for the planned
Assurance import and reconciliation coverage (TASK-490.19). They moved from
the retired Web UI suite. Keeping these inputs does not establish coverage.

There is no Assurance `default.nix` until a meaningful scenario is implemented.
The developer browser harness also reads V1R2 from this directory. The server
crate retains its separate Rust parser fixture; no application code imports
these files.
