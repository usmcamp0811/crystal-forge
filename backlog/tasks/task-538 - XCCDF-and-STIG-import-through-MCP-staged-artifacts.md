---
id: TASK-538
title: XCCDF and STIG import through MCP staged artifacts
status: Backlog
assignee: []
created_date: '2026-11-21'
labels: [agent-access, mcp, compliance]
dependencies:
  - TASK-534.8
references:
  - TASK-534.7
priority: medium
---

## Description
<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

TASK-534.7 lets agents import JSON/TOML policy interchange documents. STIG and
XCCDF packages usually exceed the 1 MiB MCP request limit, and no staged
artifact upload exists, so agents cannot use the existing
`/api/v1/compliance/xccdf/preview` and `/import` services.

## Desired outcome

An Admin-owned token uploads a package through a bounded staged-artifact
endpoint (size, expansion, file-count and XML limits of the existing XCCDF
pipeline), receives an opaque artifact id and SHA-256, and MCP tools preview and
import by artifact id and expected digest through the existing XCCDF services.
Imported requirements, supported NixOS suggestions and trusted executable
policies stay distinct; unsupported checks are never turned into Nix
expressions. No URL fetching and no server filesystem paths.
<!-- SECTION:DESCRIPTION:END -->
