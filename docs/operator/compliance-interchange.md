# Compliance Interchange Operator Guide

## Importing a STIG or XCCDF benchmark

1. **Prerequisites:** Administrator role. The user must have `admin` RBAC.

2. **Navigate to Compliance.** The Compliance view loads bundle and policy data.

3. **Open the Import/Export menu** in the page header.

4. **Select "Import STIG or XCCDF (.xml/.zip)"** or **"Import Crystal Forge bundle (.xml)".**

5. **Choose a file.** The modal accepts `.xml` and `.zip` files up to 50 MiB.
   Only well-formed XCCDF 1.2 documents are accepted.

6. **Review the preview.** The server parses the file and displays:
   - Benchmark title, version, and document classification
   - Rule count and profiles
   - Source SHA-256 digest
   - Any blocking diagnostics or warnings
   For CF-native imports, exact reuse decisions and identity conflicts are shown.

7. **Select rules.** For foreign STIG imports, all rules default to selected and
   are imported as unbound draft policies. Use the rule checklist to include or
   exclude individual rules.

8. **Set the bundle name** and optionally assign environments.

9. **Click "Import N policies"** to commit. The import is atomic — partial failure
   rolls back completely. The server always reparses the file and verifies the
   source digest from the preview.

10. **After import:**
    - New policies are **draft**, **disabled**, and **untrusted**
    - The bundle is **draft** and **untrusted**
    - Nothing is assigned to any environment or system

## Trusting imported content

Imported executable content is never activated automatically. You must review and
trust it before publishing or assigning.

### Trusting a bundle version

1. Select the bundle in the catalog.
2. Select the draft version in the version selector.
3. Use the **Version actions** panel:
   - Click **Mark trusted** to trust the version
   - Optionally add a review note through the API

### Publishing a bundle version

1. Ensure the version is trusted.
2. In the Version actions panel, click **Publish version**.
3. The version becomes immutable (accepted state). Any included draft policy
   versions are published atomically.
4. Once published, the version cannot be modified. Use **Create draft** to build
   a new editable version derived from the published one.

> **Note:** Individual policy trust and publish operations are currently available
> through the API only (`POST /api/v1/policy-versions/:id/trust`,
> `POST /api/v1/policy-versions/:id/publish`).

## Creating assignments

A bundle assignment enforces the bundle's policy baseline on an environment or system.

1. Select a published bundle version.
2. The **Assign bundle** panel appears below the version actions.
3. Choose the scope type (environment or system).
4. Select or enter the scope UUID.
5. Choose enforcement mode (enforce or report-only).
6. Click **Create assignment.**

After creation, the effective policy set for that scope is computed by the resolver.

### Understanding the effective policy set

The resolver uses priority: **system > environment > bundle baseline.**

- A system-level policy version overrides an environment-level version
- An environment version overrides a bundle baseline version
- Same exact version from multiple sources is deduplicated
- Different versions at equal specificity produce a typed conflict

Excluded policies are removed. Added policies are included. Overrides modify
specific fields within the effective policy configuration.

## Managing assignments

- **View:** Existing assignments for a scope can be listed through the API endpoints
  (`GET /api/v1/environments/:id/compliance-assignments`,
  `GET /api/v1/systems/:id/compliance-assignments`).
- **Edit:** Use `PUT /api/v1/compliance/assignments/:id` with an `expected_version_id`
  for optimistic concurrency. A 409 `ASSIGNMENT_STALE_UPDATE` indicates another
  update occurred. Reload and retry.
- **Deactivate:** Assignment deactivation is available in the UI and API
  (`DELETE /api/v1/compliance/assignments/:id`).

## Exporting XCCDF

1. Select a bundle in the catalog.
2. Choose the version (draft or published) from the version selector.
3. Open the Import/Export menu and select **Export XCCDF**.
4. The browser downloads the XCCDF XML file.

The exported document is a valid XCCDF 1.2 `Benchmark` containing:
- One baseline `Profile` selecting every bundle policy
- One `Rule` per policy version
- Standard metadata (titles, descriptions, severities, identifiers, checks, fixes)
- CF-XCCDF extension elements for native policy types

## Exporting JSON/TOML policies

In the Policies view:
- **Single policy:** Click a policy to open the detail drawer. Use the export action
  with JSON or TOML format selectors.
- **Bulk export:** Enable selection mode, check the policies to export, and use the
  Import/Export menu.
- **All custom policies:** Use "Export all custom policies" from the Import/Export menu.

## Reimporting Crystal Forge content

A CF-native XCCDF export can be imported into another Crystal Forge instance:

1. Export a bundle as XCCDF from the source instance.
2. On the target instance, open **Import Crystal Forge bundle** from the Import/Export menu.
3. Select the exported file. The server matches by portable version identities:
   - **Exact versions are reused** (no duplicates)
   - **New versions are created** when lineage matches but version identity differs
   - **Digest mismatches are rejected** with a typed conflict (409)

## Importing JSON/TOML policies

1. Open the Policies view.
2. Use the **Import/Export** menu → **Import policies…**
3. Select a `.json` or `.toml` file containing a `urn:crystal-forge:policy-set:1` document.
4. Click **Preview import.** The server validates and displays the policy set with
   the source SHA-256.
5. Review the preview (policy names, types, implementation states, versions).
6. Click **Commit import** to persist. Imported policies are created as draft versions.
   Exact existing version IDs are reused; digest conflicts are rejected.

## Understanding evidence states

The compliance rollup distinguishes these states:

| State | Meaning |
|-------|---------|
| Pass | Control evaluated and satisfied |
| Warn | Control evaluated with warnings |
| Fail | Control not satisfied |
| Waiver | Failed but formally accepted |
| Not Checked | No evaluation or evidence exists |
| Not Applicable | Control does not apply to this configuration |
| Error | Evaluator could not complete |

**Important:** The evidence system currently computes rollups from direct bundle
membership, not from assignment-resolved effective policies. If you use assignment
exclusions or additions, the rollup counts may not reflect the actual resolved
policy set. This is a known limitation (AC #30, #31).

## Handling conflicts

Common conflict scenarios and resolutions:

- **`POLICY_VERSION_DIGEST_CONFLICT` (409):** A policy version ID already exists
  but with a different semantic digest. The import must not proceed. Resolution:
  import to a clean target, or export with a new version identity.

- **`ASSIGNMENT_STALE_UPDATE` (409):** Another administrator modified the assignment
  between when you loaded it and when you submitted your update. Reload and reapply
  your changes.

- **Assignment resolution conflict:** Two sources contribute different versions of
  the same policy lineage at equal specificity. The UI displays the conflict details.
  Resolution: adjust the assignment or system-level policy choices to remove ambiguity.

## Security limits

The server enforces these limits on XCCDF uploads:

- Maximum file size: 50 MiB
- Maximum XML depth: 50
- Maximum attributes per element: 200
- Maximum text node length: 1 MiB
- DTD processing: disabled
- External entities: rejected
- Network schema retrieval: blocked
- ZIP bombs and path traversal: rejected
