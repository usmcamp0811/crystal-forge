//! Persistence queries for committed XCCDF foreign imports.
//!
//! All durable writes happen in one atomic transaction:
//!
//! ```text
//! BEGIN
//!   upsert source artifact
//!   insert bundle lineage
//!   insert bundle version (semantic_digest = 'pending')
//!   for each non-excluded rule:
//!     insert policy lineage
//!     insert policy version (semantic_digest = 'pending')
//!   insert ordered membership
//!   insert source-object mappings
//!   compute and persist policy digests
//!   compute and persist bundle digest
//!   update bundle current_draft_version_id pointer
//!   write audit event
//! COMMIT
//! ```
//!
//! Any failure rolls back every write in this list.

use anyhow::{Context, Result};
use sqlx::PgPool;
use uuid::Uuid;

use crate::compliance::digest::{
    BundleVersionCanonical, PolicyVersionCanonical, load_bundle_membership,
    write_bundle_version_digest, write_policy_version_digest,
};
use crate::compliance::framework_model::FrameworkVersionCanonical;
use crate::compliance::xccdf::disa_stig_adapter::{
    canonical_for_rule, canonical_key_for_rule, identify_framework, is_disa_stig,
};
use crate::compliance::xccdf::exact_technical_match::{
    ExactTechnicalMatchValidation, revalidate_exact_technical_match,
};
use crate::compliance::xccdf::import_models::ImportedPolicyRecord;
use crate::compliance::xccdf::import_models::{MapExistingProof, ValidatedImportPlan, XccdfCommittedImportResult};
use crate::compliance::xccdf::importer::build_policy_records;
use crate::compliance::xccdf::package::{ProcessedXccdfPackage, build_package_context};
use crate::compliance::xccdf::reconciliation::{
    ExistingPolicyIdentity, NativePolicyIdentity, NativeReconcileFailure, ReconcileConflict,
    ReconcileDecision, plan_policy_reconciliation,
};
use crate::queries::compliance::{PolicyDraftIntent, ensure_policy_draft};
use crate::queries::framework_requirements::{
    insert_bundle_version_requirement, insert_framework_version, insert_policy_mapping_in_tx,
    insert_requirement_version, upsert_framework_lineage, upsert_requirement_lineage,
};

// ── Parser version identifier ─────────────────────────────────────────────────

/// Monotone identifier incremented when the parser's semantic output changes.
/// Stored on the source artifact so future schema migrations can re-classify
/// artifacts parsed by earlier versions.
pub const CF_PARSER_VERSION: &str = "cf-xccdf-parser-0.1";

// ── Main entry point ──────────────────────────────────────────────────────────

/// Commit all durable records for a foreign XCCDF import in one transaction.
///
/// `policy_records` is produced by [`build_policy_records`] before this call;
/// it is passed in so the function has no re-parsing or validation work.
pub async fn commit_foreign_import(
    pool: &PgPool,
    importing_user_id: Uuid,
    pkg: ProcessedXccdfPackage,
    validated: ValidatedImportPlan,
    policy_records: Vec<ImportedPolicyRecord>,
) -> Result<XccdfCommittedImportResult> {
    #[derive(Debug, Clone, Copy)]
    struct NormalizedRequirementImport {
        requirement_version_id: Uuid,
        previous_requirement_version_id: Option<Uuid>,
        unchanged_from_previous_release: bool,
    }
    let mut tx = pool
        .begin()
        .await
        .context("failed to begin import transaction")?;

    // ── 1. Source artifact ────────────────────────────────────────────────────
    let media_type = match pkg.provenance.package_kind {
        crate::compliance::xccdf::zip_extractor::PackageKind::Xml => "application/xml",
        crate::compliance::xccdf::zip_extractor::PackageKind::Zip => "application/zip",
    };
    let artifact_filename = pkg
        .provenance
        .filename
        .as_deref()
        .unwrap_or("unknown")
        .to_owned();
    let package_context = build_package_context(&pkg.provenance);
    let source_sha256 = pkg.provenance.sha256.clone();
    let detected_xccdf_version = pkg.parsed.xccdf_namespace_version.map(str::to_owned);
    let document_class = format!("{:?}", pkg.parsed.class).to_lowercase();
    let fidelity = format!("{:?}", pkg.parsed.fidelity).to_lowercase();

    sqlx::query(
        r#"
        INSERT INTO compliance_source_artifacts
            (content, filename, media_type, sha256, parser_version,
             detected_xccdf_version, package_context, imported_by)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (sha256) DO NOTHING
        "#,
    )
    .bind(&pkg.original_bytes)
    .bind(&artifact_filename)
    .bind(media_type)
    .bind(&source_sha256)
    .bind(CF_PARSER_VERSION)
    .bind(&detected_xccdf_version)
    .bind(&package_context)
    .bind(importing_user_id)
    .execute(&mut *tx)
    .await
    .context("failed to upsert source artifact")?;

    let source_artifact_id: Uuid =
        sqlx::query_scalar("SELECT id FROM compliance_source_artifacts WHERE sha256 = $1")
            .bind(&source_sha256)
            .fetch_one(&mut *tx)
            .await
            .context("failed to load source artifact")?;

    // An artifact is its own immutable import identity.  Returning the original
    // result before creating any bundle or policy rows makes an exact re-import
    // idempotent across all imported objects, including normalized requirements
    // and mappings added below.
    if let Some(existing_bundle_version_id) = sqlx::query_scalar::<_, Uuid>(
        "SELECT bundle_version_id FROM compliance_source_object_mappings \
         WHERE source_artifact_id = $1 AND object_kind = 'benchmark' \
         AND bundle_version_id IS NOT NULL LIMIT 1",
    )
    .bind(source_artifact_id)
    .fetch_optional(&mut *tx)
    .await
    .context("failed to check exact artifact import")?
    {
        let (existing_bundle_id, bundle_semantic_digest): (Uuid, String) = sqlx::query_as(
            "SELECT bundle_id, semantic_digest FROM compliance_bundle_versions WHERE id = $1",
        )
        .bind(existing_bundle_version_id)
        .fetch_one(&mut *tx)
        .await
        .context("failed to load exact artifact bundle version")?;
        let reused_policy_versions: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM compliance_bundle_version_policies WHERE bundle_version_id = $1",
        )
        .bind(existing_bundle_version_id)
        .fetch_one(&mut *tx)
        .await
        .context("failed to count exact artifact policy membership")?;

        tx.commit()
            .await
            .context("failed to commit exact artifact import")?;
        return Ok(XccdfCommittedImportResult {
            source_artifact_id,
            bundle_id: existing_bundle_id,
            bundle_version_id: existing_bundle_version_id,
            created_policy_count: 0,
            created_policy_lineages: 0,
            created_policy_versions: 0,
            reused_policy_versions: reused_policy_versions as u32,
            bundle_lineage_created: false,
            bundle_version_created: false,
            excluded_rule_count: 0,
            created_policy_version_ids: vec![],
            source_sha256,
            bundle_semantic_digest,
            warnings: vec![],
        });
    }

    // ── 2. Bundle lineage ─────────────────────────────────────────────────────
    let bundle_name = validated.bundle.name.trim().to_owned();
    let bundle_framework = validated.bundle.framework.trim().to_owned();
    let bundle_version = validated.bundle.version.trim().to_owned();
    let bundle_layer = validated
        .bundle
        .layer
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("fleet")
        .to_owned();
    let bundle_owner = validated
        .bundle
        .owner
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Platform Security")
        .to_owned();
    let bundle_description = validated
        .bundle
        .description
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);

    let bundle_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO compliance_bundles (name, framework, version, description, layer, owner)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id
        "#,
    )
    .bind(&bundle_name)
    .bind(&bundle_framework)
    .bind(&bundle_version)
    .bind(&bundle_description)
    .bind(&bundle_layer)
    .bind(&bundle_owner)
    .fetch_one(&mut *tx)
    .await
    .context("failed to insert bundle lineage")?;

    // The INSERT above fires a trigger that creates the initial draft bundle
    // version and sets current_draft_version_id. We load that pointer now.
    let bundle_version_id: Uuid =
        sqlx::query_scalar("SELECT current_draft_version_id FROM compliance_bundles WHERE id = $1")
            .bind(bundle_id)
            .fetch_one(&mut *tx)
            .await
            .context("failed to read bundle current_draft_version_id after insert")?;

    // Attach the source artifact to the bundle version.
    sqlx::query(
        "UPDATE compliance_bundle_versions SET source_artifact_id = $1, created_by = $2 WHERE id = $3",
    )
    .bind(source_artifact_id)
    .bind(importing_user_id)
    .bind(bundle_version_id)
    .execute(&mut *tx)
    .await
    .context("failed to set source_artifact_id on bundle version")?;

    // DISA STIGs have a stable framework/release identity and requirement
    // lineages.  Persist those authoritative objects before choosing policy
    // implementations; the legacy policy metadata remains source provenance.
    let normalized_requirements = if is_disa_stig(&pkg.parsed)
        && let Some(identity) = identify_framework(&pkg.parsed)
    {
        let framework_name = identity
            .title
            .as_deref()
            .unwrap_or("DISA Security Technical Implementation Guide");
        let framework_id = upsert_framework_lineage(
            &mut tx,
            framework_name,
            Some(&identity.publisher),
            &identity.canonical_source_key,
            identity.title.as_deref(),
        )
        .await?;
        let previous_framework_version_id: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM compliance_framework_versions \
             WHERE framework_id = $1 \
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(framework_id)
        .fetch_optional(&mut *tx)
        .await
        .context("failed to load prior framework release for STIG reconciliation")?;
        let framework_version_id = insert_framework_version(
            &mut tx,
            framework_id,
            &FrameworkVersionCanonical {
                canonical_source_key: identity.canonical_source_key,
                canonical_release_key: identity.canonical_release_key,
                version: identity.version,
                publisher: Some(identity.publisher),
                title: identity.title,
            },
            Some(source_artifact_id),
            None,
        )
        .await?;

        let mut requirement_versions = std::collections::HashMap::new();
        for (requirement_order, record) in policy_records.iter().enumerate() {
            let rule = pkg
                .parsed
                .rules
                .iter()
                .find(|rule| rule.id == record.source_rule_id)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "IMPORT_RULE_NOT_FOUND: parsed rule {} disappeared before commit",
                        record.source_rule_id
                    )
                })?;
            let canonical_key = canonical_key_for_rule(rule);
            let requirement_id =
                upsert_requirement_lineage(&mut tx, framework_id, &canonical_key).await?;
            let canonical = canonical_for_rule(rule, &canonical_key);
            let previous_requirement_version: Option<(Uuid, String)> =
                if let Some(previous_framework_version_id) = previous_framework_version_id {
                    sqlx::query_as(
                        "SELECT id, semantic_digest FROM compliance_requirement_versions \
                         WHERE requirement_id = $1 AND framework_version_id = $2",
                    )
                    .bind(requirement_id)
                    .bind(previous_framework_version_id)
                    .fetch_optional(&mut *tx)
                    .await
                    .context("failed to load prior requirement version for STIG reconciliation")?
                } else {
                    None
                };
            let requirement_version_id = insert_requirement_version(
                &mut tx,
                requirement_id,
                framework_version_id,
                &canonical,
                None,
            )
            .await?;
            insert_bundle_version_requirement(
                &mut tx,
                bundle_version_id,
                requirement_version_id,
                requirement_order as i32,
            )
            .await?;
            let (previous_requirement_version_id, unchanged_from_previous_release) =
                previous_requirement_version
                    .map(|(id, digest)| (Some(id), digest == canonical.compute_digest()))
                    .unwrap_or((None, false));
            requirement_versions.insert(
                record.source_rule_id.clone(),
                NormalizedRequirementImport {
                    requirement_version_id,
                    previous_requirement_version_id,
                    unchanged_from_previous_release,
                },
            );
        }
        Some(requirement_versions)
    } else {
        None
    };

    // ── 3. Policy lineages and versions ───────────────────────────────────────
    let excluded_rule_count =
        (validated.rules_to_import.len() as u32).saturating_sub(policy_records.len() as u32);

    let mut created_policy_version_ids: Vec<Uuid> = Vec::new();
    let mut effective_mapped_policy_versions = std::collections::HashMap::new();
    let mut inherited_mappings = std::collections::HashMap::new();

    for rec in &policy_records {
        if let Some(mapped_version_id) = rec.mapped_policy_version_id {
            let selected: Option<(Uuid, String, Option<Uuid>)> = sqlx::query_as(
                "SELECT pv.policy_id, pv.publication_state, dp.current_published_version_id \
                 FROM deployment_policy_versions pv \
                 JOIN deployment_policies dp ON dp.id = pv.policy_id \
                 WHERE pv.id = $1",
            )
            .bind(mapped_version_id)
            .fetch_optional(&mut *tx)
            .await
            .context("failed to verify mapped policy version")?;
            let Some((policy_id, publication_state, current_published_version_id)) = selected
            else {
                anyhow::bail!(
                    "IMPORT_POLICY_VERSION_NOT_FOUND: mapped policy version {} does not exist",
                    mapped_version_id
                );
            };

            // Every reuse decision must carry an explicit proof and the proof is
            // never trusted from preview: it is revalidated from authoritative
            // source bytes inside this transaction.
            match rec.mapped_policy_proof {
                Some(MapExistingProof::ExactTechnicalMatch) => {
                    // Re-derive the technical identity from the parsed rule's
                    // authoritative fix text, then re-check that the selected
                    // policy version still exists, is accepted, is the current
                    // published version, and its config still implements the
                    // enforcement. Any stale or superseded decision aborts the
                    // whole import transaction (no partial writes).
                    let parsed_rule = pkg
                        .parsed
                        .rules
                        .iter()
                        .find(|rule| rule.id == rec.source_rule_id)
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "IMPORT_RULE_NOT_FOUND: parsed rule {} disappeared before commit",
                                rec.source_rule_id
                            )
                        })?;
                    let authoritative_fix_text = parsed_rule
                        .fix
                        .as_ref()
                        .map(|fix| fix.content.as_str())
                        .unwrap_or_default();
                    match revalidate_exact_technical_match(
                        &mut tx,
                        mapped_version_id,
                        authoritative_fix_text,
                    )
                    .await?
                    {
                        ExactTechnicalMatchValidation::Valid { .. } => {}
                        ExactTechnicalMatchValidation::Invalid { code, message } => {
                            anyhow::bail!("{code}: {message}");
                        }
                    }
                }
                Some(MapExistingProof::InheritedMapping) | None => {
                    if let Some(requirement_versions) = &normalized_requirements {
                        let requirement = requirement_versions.get(&rec.source_rule_id).ok_or_else(
                            || {
                                anyhow::anyhow!(
                                    "IMPORT_REQUIREMENT_NOT_FOUND: missing normalized requirement for {}",
                                    rec.source_rule_id
                                )
                            },
                        )?;
                        let inherited_mapping: Option<(String, String, Option<String>)> =
                            if requirement.unchanged_from_previous_release {
                                let previous_requirement_version_id =
                                    requirement.previous_requirement_version_id.ok_or_else(|| {
                                        anyhow::anyhow!(
                                            "IMPORT_REUSE_INELIGIBLE: missing prior requirement version"
                                        )
                                    })?;
                                sqlx::query_as(
                                "SELECT relationship, coverage, rationale FROM policy_requirement_mappings \
                                 WHERE policy_version_id = $1 AND requirement_version_id = $2 \
                                   AND trust_state = 'trusted'",
                            )
                            .bind(mapped_version_id)
                            .bind(previous_requirement_version_id)
                            .fetch_optional(&mut *tx)
                            .await
                            .context("failed to validate selected policy reuse")?
                            } else {
                                None
                            };
                        let Some(inherited_mapping) = inherited_mapping else {
                            anyhow::bail!(
                                "IMPORT_REUSE_INELIGIBLE: policy version {} is not a trusted mapping for an unchanged prior requirement",
                                mapped_version_id
                            );
                        };
                        inherited_mappings.insert(rec.source_rule_id.clone(), inherited_mapping);
                    }
                    // Inherited reuse refers to the exact immutable local policy
                    // version, so it must still be the current accepted version.
                    if publication_state != "accepted"
                        || current_published_version_id != Some(mapped_version_id)
                    {
                        anyhow::bail!(
                            "IMPORT_REUSE_INELIGIBLE: policy version {} must be the current accepted version of policy {}",
                            mapped_version_id,
                            policy_id
                        );
                    }
                }
            }
            let effective_policy_version_id = ensure_policy_draft(
                &mut tx,
                policy_id,
                Some(importing_user_id),
                None,
                PolicyDraftIntent::EnsureMutable,
            )
            .await
            .context("failed to derive mutable policy draft for STIG requirement reuse")?;
            effective_mapped_policy_versions.insert(mapped_version_id, effective_policy_version_id);
            continue;
        }
        // Insert policy lineage.
        sqlx::query(
            r#"
            INSERT INTO deployment_policies (id, name, description, policy_type, config, enabled)
            VALUES ($1, $2, $3, $4, $5, false)
            "#,
        )
        .bind(rec.policy_id)
        .bind(&rec.name)
        .bind(&rec.description)
        .bind(&rec.policy_type)
        .bind(&rec.config)
        .execute(&mut *tx)
        .await
        .context("failed to insert policy lineage")?;

        // Insert policy version.
        sqlx::query(
            r#"
            INSERT INTO deployment_policy_versions (
                id, policy_id, version,
                name, description,
                policy_type, implementation_state, execution_phase,
                config, compliance_metadata, dependencies,
                opaque_xml, semantic_digest, source_artifact_id,
                created_by, enabled_by_default
            ) VALUES (
                $1, $2, $3,
                $4, $5,
                $6, $7, $8,
                $9, $10, $11,
                $12, 'pending', $13,
                $14, false
            )
            "#,
        )
        .bind(rec.policy_version_id)
        .bind(rec.policy_id)
        .bind("0.1-draft") // initial draft version label
        .bind(&rec.name)
        .bind(&rec.description)
        .bind(&rec.policy_type)
        .bind(&rec.implementation_state)
        .bind(&rec.execution_phase)
        .bind(&rec.config)
        .bind(&rec.compliance_metadata)
        .bind(&rec.dependencies)
        .bind(&rec.opaque_xml)
        .bind(source_artifact_id)
        .bind(importing_user_id)
        .execute(&mut *tx)
        .await
        .context("failed to insert policy version")?;

        // Set the policy's current_draft_version_id pointer.
        sqlx::query("UPDATE deployment_policies SET current_draft_version_id = $1 WHERE id = $2")
            .bind(rec.policy_version_id)
            .bind(rec.policy_id)
            .execute(&mut *tx)
            .await
            .context("failed to set policy current_draft_version_id")?;

        created_policy_version_ids.push(rec.policy_version_id);
    }

    // ── 4. Ordered membership ─────────────────────────────────────────────────
    let mut bundled_policy_versions = std::collections::HashSet::new();
    let mut policy_order = 0_i32;
    for rec in &policy_records {
        let policy_version_id = rec
            .mapped_policy_version_id
            .and_then(|id| effective_mapped_policy_versions.get(&id).copied())
            .unwrap_or(rec.policy_version_id);
        if bundled_policy_versions.insert(policy_version_id) {
            sqlx::query(
                r#"
            INSERT INTO compliance_bundle_version_policies
                (bundle_version_id, policy_version_id, policy_order, selected)
            VALUES ($1, $2, $3, true)
            "#,
            )
            .bind(bundle_version_id)
            .bind(policy_version_id)
            .bind(policy_order)
            .execute(&mut *tx)
            .await
            .context("failed to insert bundle membership row")?;
            policy_order += 1;
        }

        if let Some(requirement_versions) = &normalized_requirements {
            let requirement = requirement_versions
                .get(&rec.source_rule_id)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "IMPORT_REQUIREMENT_NOT_FOUND: missing normalized requirement for {}",
                        rec.source_rule_id
                    )
                })?;
            // Mapping semantics stay per requirement so a shared policy can
            // carry a different reviewed relationship/coverage for each
            // requirement it satisfies (item 13).
            let (relationship, coverage, rationale, provenance) = if let Some((
                relationship,
                coverage,
                rationale,
            )) = inherited_mappings.get(&rec.source_rule_id)
            {
                (
                    relationship.as_str(),
                    coverage.as_str(),
                    rationale.as_deref(),
                    "inherited",
                )
            } else if rec.mapped_policy_version_id.is_some() {
                // Exact-technical-match reuse: the mapping is established from
                // technical candidate matching, so provenance is `inferred`
                // (the only CHECK-valid value for this origin).
                let semantics = rec.mapping_semantics.as_ref();
                let relationship = semantics
                    .and_then(|s| s.relationship.as_deref())
                    .filter(|value| {
                        matches!(
                            *value,
                            "implements" | "supports" | "provides_evidence_for"
                        )
                    })
                    .unwrap_or("implements");
                let coverage = semantics
                    .and_then(|s| s.coverage.as_deref())
                    .filter(|value| matches!(*value, "full" | "partial"))
                    .unwrap_or("full");
                let rationale = semantics.and_then(|s| s.rationale.as_deref());
                (relationship, coverage, rationale, "inferred")
            } else {
                ("implements", "full", None, "imported")
            };
            insert_policy_mapping_in_tx(
                &mut tx,
                policy_version_id,
                requirement.requirement_version_id,
                relationship,
                coverage,
                rationale,
                provenance,
                Some(source_artifact_id),
                importing_user_id,
            )
            .await?;
        }
    }

    // ── 5. Source-object mappings ─────────────────────────────────────────────
    // Benchmark → bundle version
    if let Some(ref bm) = pkg.parsed.benchmark {
        sqlx::query(
            r#"
            INSERT INTO compliance_source_object_mappings
                (source_artifact_id, object_kind, source_identity, bundle_version_id, fidelity)
            VALUES ($1, 'benchmark', $2, $3, 'preserved_opaque')
            ON CONFLICT (source_artifact_id, object_kind, source_identity) DO NOTHING
            "#,
        )
        .bind(source_artifact_id)
        .bind(&bm.id)
        .bind(bundle_version_id)
        .execute(&mut *tx)
        .await
        .context("failed to insert benchmark source mapping")?;
    }

    // Rules → policy versions
    for rec in &policy_records {
        let policy_version_id = rec
            .mapped_policy_version_id
            .and_then(|id| effective_mapped_policy_versions.get(&id).copied())
            .unwrap_or(rec.policy_version_id);
        sqlx::query(
            r#"
            INSERT INTO compliance_source_object_mappings
                (source_artifact_id, object_kind, source_identity, policy_version_id, fidelity)
            VALUES ($1, 'rule', $2, $3, 'preserved_opaque')
            ON CONFLICT (source_artifact_id, object_kind, source_identity) DO NOTHING
            "#,
        )
        .bind(source_artifact_id)
        .bind(&rec.source_rule_id)
        .bind(policy_version_id)
        .execute(&mut *tx)
        .await
        .context("failed to insert rule source mapping")?;
    }

    // Profile → bundle version (if selected)
    if let Some(ref pid) = validated.expected_sha256.as_str().get(0..0) {
        // placeholder — profile mappings are inserted below
        let _ = pid;
    }

    // ── 6. Compute and persist semantic digests ───────────────────────────────
    for rec in &policy_records {
        if rec.mapped_policy_version_id.is_some() {
            continue;
        }
        let opaque_xml_digest =
            PolicyVersionCanonical::digest_opaque_xml(rec.opaque_xml.as_deref());

        let canonical = PolicyVersionCanonical {
            name: rec.name.clone(),
            description: rec.description.clone(),
            policy_type: rec.policy_type.clone(),
            implementation_state: rec.implementation_state.clone(),
            execution_phase: rec.execution_phase.clone(),
            config: rec.config.clone(),
            compliance_metadata: rec.compliance_metadata.clone(),
            dependencies: rec.dependencies.clone(),
            opaque_xml_digest,
            enabled_by_default: Some(rec.enabled_by_default),
        };

        write_policy_version_digest(&mut tx, rec.policy_id, &canonical)
            .await
            .context("failed to write policy version digest")?;
    }

    // Bundle digest (uses ordered membership).
    let members = load_bundle_membership(&mut tx, bundle_version_id)
        .await
        .context("failed to load bundle membership for digest")?;

    let bundle_canonical = BundleVersionCanonical {
        name: bundle_name.clone(),
        framework: bundle_framework.clone(),
        framework_version: Some(bundle_version.clone()),
        description: bundle_description.clone(),
        layer: bundle_layer.clone(),
        owner: bundle_owner.clone(),
        members,
    };

    write_bundle_version_digest(&mut tx, bundle_id, &bundle_canonical)
        .await
        .context("failed to write bundle version digest")?;

    // Read back the computed bundle digest for the response.
    let bundle_semantic_digest: String =
        sqlx::query_scalar("SELECT semantic_digest FROM compliance_bundle_versions WHERE id = $1")
            .bind(bundle_version_id)
            .fetch_one(&mut *tx)
            .await
            .context("failed to read bundle semantic digest")?;

    // ── 7. Audit event ────────────────────────────────────────────────────────
    let created_policy_count = created_policy_version_ids.len() as u32;
    let reused_policy_versions = effective_mapped_policy_versions
        .values()
        .copied()
        .collect::<std::collections::HashSet<_>>()
        .len() as u32;
    let metadata = serde_json::json!({
        "source_artifact_id": source_artifact_id,
        "original_sha256": source_sha256,
        "bundle_id": bundle_id,
        "bundle_version_id": bundle_version_id,
        "created_policy_count": created_policy_count,
        "reused_policy_versions": reused_policy_versions,
        "excluded_rule_count": excluded_rule_count,
        "document_class": document_class,
        "fidelity": fidelity,
        "trust_state": "untrusted",
    });

    sqlx::query(
        r#"
        INSERT INTO admin_audit_events
            (actor_user_id, actor_identifier, action, target, metadata)
        VALUES ($1, $1::text, 'xccdf_imported', $2, $3)
        "#,
    )
    .bind(importing_user_id)
    .bind(format!("bundle:{bundle_id}"))
    .bind(&metadata)
    .execute(&mut *tx)
    .await
    .context("failed to write audit event")?;

    // ── 8. Commit ─────────────────────────────────────────────────────────────
    tx.commit()
        .await
        .context("failed to commit import transaction")?;

    Ok(XccdfCommittedImportResult {
        source_artifact_id,
        bundle_id,
        bundle_version_id,
        created_policy_count,
        created_policy_lineages: created_policy_count,
        created_policy_versions: created_policy_count,
        reused_policy_versions,
        bundle_lineage_created: true,
        bundle_version_created: true,
        excluded_rule_count,
        created_policy_version_ids,
        source_sha256,
        bundle_semantic_digest,
        warnings: vec![],
    })
}

/// Commit a validated CF-native import. Unlike the foreign path, portable UUIDs
/// are authoritative and existing immutable versions are reused rather than
/// copied into new local lineages.
pub async fn commit_cf_native_import(
    pool: &PgPool,
    importing_user_id: Uuid,
    pkg: ProcessedXccdfPackage,
    validated: ValidatedImportPlan,
    policy_records: Vec<ImportedPolicyRecord>,
) -> Result<XccdfCommittedImportResult> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to begin import transaction")?;
    let source_sha256 = pkg.provenance.sha256.clone();
    let source_artifact_id = upsert_source_artifact(&mut tx, importing_user_id, &pkg).await?;

    let mut lock_keys: Vec<(u8, Uuid)> = policy_records
        .iter()
        .flat_map(|r| [(0, r.policy_id), (1, r.policy_version_id)])
        .chain(std::iter::once((
            2,
            pkg.parsed
                .cf_bundle_meta
                .as_ref()
                .map(|m| m.bundle_id)
                .unwrap_or_default(),
        )))
        .chain(std::iter::once((
            3,
            pkg.parsed
                .cf_bundle_meta
                .as_ref()
                .map(|m| m.bundle_version_id)
                .unwrap_or_default(),
        )))
        .collect();
    lock_keys.sort();
    lock_keys.dedup();
    for (kind, id) in &lock_keys {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(format!("cf-native:{kind}:{id}"))
            .execute(&mut *tx)
            .await
            .context("failed to acquire CF-native reconciliation lock")?;
    }

    let lineage_ids: Vec<Uuid> = policy_records.iter().map(|r| r.policy_id).collect();
    let version_ids: Vec<Uuid> = policy_records.iter().map(|r| r.policy_version_id).collect();
    let existing: Vec<(Uuid, Uuid, String, String)> = sqlx::query_as(
        "SELECT policy_id, id, policy_type, semantic_digest FROM deployment_policy_versions \
         WHERE id = ANY($1) OR policy_id = ANY($2)",
    )
    .bind(&version_ids)
    .bind(&lineage_ids)
    .fetch_all(&mut *tx)
    .await
    .context("failed to load CF-native policy identities")?;
    let existing_identities: Vec<ExistingPolicyIdentity> = existing
        .into_iter()
        .map(
            |(lineage_id, version_id, policy_type, semantic_digest)| ExistingPolicyIdentity {
                lineage_id,
                version_id,
                policy_type,
                semantic_digest,
            },
        )
        .collect();
    let imported_identities: Vec<NativePolicyIdentity> = policy_records
        .iter()
        .map(|r| NativePolicyIdentity {
            lineage_id: r.policy_id,
            version_id: r.policy_version_id,
            policy_type: r.policy_type.clone(),
            semantic_digest: r.semantic_digest.clone().unwrap_or_default(),
            source_rule_id: r.source_rule_id.clone(),
        })
        .collect();
    let policy_plan = plan_policy_reconciliation(&imported_identities, &existing_identities);
    if !policy_plan.conflicts.is_empty() {
        return Err(anyhow::Error::new(NativeReconcileFailure {
            conflicts: policy_plan.conflicts,
        }));
    }

    let mut resolved_versions = Vec::with_capacity(policy_records.len());
    let mut created_lineages = 0u32;
    let mut created_versions = 0u32;
    let mut reused_versions = 0u32;
    let mut created_version_ids = Vec::new();
    let records_by_version: std::collections::HashMap<Uuid, &ImportedPolicyRecord> = policy_records
        .iter()
        .map(|record| (record.policy_version_id, record))
        .collect();
    for (identity, decision) in policy_plan.decisions.iter() {
        let record = records_by_version
            .get(&identity.version_id)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("CF-native policy plan lost source record"))?;
        match decision {
            ReconcileDecision::ReuseExact {
                local_version_id, ..
            } => {
                resolved_versions.push(*local_version_id);
                reused_versions += 1;
            }
            ReconcileDecision::CreateLineageAndVersion {
                portable_lineage_id,
                portable_version_id,
            } => {
                create_native_policy_lineage_and_version(&mut tx, importing_user_id, record)
                    .await?;
                debug_assert_eq!(*portable_lineage_id, record.policy_id);
                debug_assert_eq!(*portable_version_id, record.policy_version_id);
                resolved_versions.push(record.policy_version_id);
                created_version_ids.push(record.policy_version_id);
                created_lineages += 1;
                created_versions += 1;
            }
            ReconcileDecision::CreateVersionInExistingLineage {
                local_lineage_id,
                portable_version_id,
            } => {
                let draft: Option<Uuid> = sqlx::query_scalar(
                    "SELECT current_draft_version_id FROM deployment_policies WHERE id = $1 FOR UPDATE",
                )
                .bind(local_lineage_id)
                .fetch_one(&mut *tx)
                .await
                .context("failed to lock existing policy lineage")?;
                if draft.is_some() {
                    return Err(anyhow::Error::new(NativeReconcileFailure {
                        conflicts: vec![ReconcileConflict::VersionDigestMismatch {
                            lineage_id: *local_lineage_id,
                            version_id: *portable_version_id,
                            local_digest: "current draft already exists".into(),
                            imported_digest: record.semantic_digest.clone().unwrap_or_default(),
                            source_rule_id: record.source_rule_id.clone(),
                        }],
                    }));
                }
                insert_native_policy_version(&mut tx, importing_user_id, record).await?;
                resolved_versions.push(record.policy_version_id);
                created_version_ids.push(record.policy_version_id);
                created_versions += 1;
            }
        }
    }
    if !created_version_ids.is_empty() {
        sqlx::query(
            "UPDATE deployment_policy_versions SET source_artifact_id = $1 WHERE id = ANY($2)",
        )
        .bind(source_artifact_id)
        .bind(&created_version_ids)
        .execute(&mut *tx)
        .await
        .context("failed to attach CF-native source artifact to policies")?;
    }
    let resolved_by_version: std::collections::HashMap<Uuid, Uuid> = policy_plan
        .decisions
        .iter()
        .zip(resolved_versions.iter())
        .map(|(identity, resolved)| (identity.0.version_id, *resolved))
        .collect();

    let bundle_meta = pkg
        .parsed
        .cf_bundle_meta
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("validated CF-native import has no bundle metadata"))?;
    let bundle_digest = bundle_meta.digest.clone().unwrap_or_default();
    let bundle_id = bundle_meta.bundle_id;
    let bundle_version_id = bundle_meta.bundle_version_id;

    // Load existing bundle version: match the exact portable version id
    // first, then fall back to the lineage's latest version so a new version
    // of an existing lineage plans as CreateVersionInExistingLineage
    // (design 18.2 step 4) instead of a brand-new lineage insert.
    let existing_bundle_version: Option<(Uuid, Uuid, String)> = match sqlx::query_as(
        "SELECT id, bundle_id, semantic_digest FROM compliance_bundle_versions WHERE id = $1",
    )
    .bind(bundle_version_id)
    .fetch_optional(&mut *tx)
    .await
    .context("failed to load CF-native bundle version")?
    {
        Some(found) => Some(found),
        None => sqlx::query_as(
            "SELECT id, bundle_id, semantic_digest FROM compliance_bundle_versions \
             WHERE bundle_id = $1 ORDER BY created_at DESC LIMIT 1",
        )
        .bind(bundle_id)
        .fetch_optional(&mut *tx)
        .await
        .context("failed to load CF-native bundle lineage version")?,
    };

    // Build existing bundle identity if a version of the lineage exists
    let existing_bundle_identity =
        if let Some((local_version_id, local_bundle_id, local_digest)) = existing_bundle_version {
            let local_membership: Vec<(Uuid, bool)> = sqlx::query_as(
                "SELECT policy_version_id, selected FROM compliance_bundle_version_policies \
             WHERE bundle_version_id = $1 ORDER BY policy_order",
            )
            .bind(local_version_id)
            .fetch_all(&mut *tx)
            .await
            .context("failed to load CF-native bundle membership")?;
            Some(
                crate::compliance::xccdf::reconciliation::ExistingBundleIdentity {
                    lineage_id: local_bundle_id,
                    version_id: local_version_id,
                    semantic_digest: local_digest,
                    members: local_membership,
                },
            )
        } else {
            None
        };

    // Build imported bundle identity: (resolved policy_version_id, selected) ordered by policy_order
    let mut imported_members: Vec<(Uuid, bool, i32)> = policy_records
        .iter()
        .map(|r| {
            (
                resolved_by_version[&r.policy_version_id],
                r.selected,
                r.policy_order,
            )
        })
        .collect();
    imported_members.sort_by_key(|(_, _, order)| *order);
    let imported_members: Vec<(Uuid, bool)> = imported_members
        .into_iter()
        .map(|(version_id, selected, _)| (version_id, selected))
        .collect();
    let imported_bundle_identity = crate::compliance::xccdf::reconciliation::NativeBundleIdentity {
        lineage_id: bundle_id,
        version_id: bundle_version_id,
        semantic_digest: bundle_digest.clone(),
        members: imported_members,
    };

    // Plan bundle reconciliation using shared planner
    let bundle_plan = crate::compliance::xccdf::reconciliation::plan_bundle_reconciliation(
        &imported_bundle_identity,
        existing_bundle_identity.as_ref(),
    );

    // Reject if there are conflicts
    if !bundle_plan.conflicts.is_empty() {
        return Err(anyhow::Error::new(NativeReconcileFailure {
            conflicts: bundle_plan.conflicts.into_iter().map(|c| {
                match c {
                    crate::compliance::xccdf::reconciliation::BundleReconcileConflict::VersionDigestMismatch {
                        lineage_id,
                        version_id,
                        local_digest,
                        imported_digest,
                    } => ReconcileConflict::VersionDigestMismatch {
                        lineage_id,
                        version_id,
                        local_digest,
                        imported_digest,
                        source_rule_id: "bundle".into(),
                    },
                    crate::compliance::xccdf::reconciliation::BundleReconcileConflict::VersionBelongsToDifferentLineage {
                        lineage_id,
                        version_id,
                        actual_lineage_id,
                    } => ReconcileConflict::VersionBelongsToDifferentLineage {
                        lineage_id,
                        version_id,
                        actual_lineage_id,
                    },
                    crate::compliance::xccdf::reconciliation::BundleReconcileConflict::BundleMembershipMismatch {
                        lineage_id,
                        version_id,
                    } => ReconcileConflict::VersionDigestMismatch {
                        lineage_id,
                        version_id,
                        local_digest: "bundle membership differs".into(),
                        imported_digest: bundle_digest.clone(),
                        source_rule_id: "bundle".into(),
                    },
                }
            }).collect(),
        }));
    }

    // Apply the bundle reconciliation decision
    let (bundle_version_created, bundle_lineage_created) = match &bundle_plan.decision {
        Some(crate::compliance::xccdf::reconciliation::BundleReconcileDecision::ReuseExact { .. }) => {
            (false, false)
        }
        Some(crate::compliance::xccdf::reconciliation::BundleReconcileDecision::CreateLineageAndVersion { .. }) => {
            create_native_bundle_lineage(
                &mut tx,
                importing_user_id,
                &validated,
                bundle_id,
                bundle_version_id,
                &bundle_digest,
            )
            .await?;
            (true, true)
        }
        Some(crate::compliance::xccdf::reconciliation::BundleReconcileDecision::CreateVersionInExistingLineage { .. }) => {
            let current_draft: Option<Uuid> = sqlx::query_scalar(
                "SELECT current_draft_version_id FROM compliance_bundles WHERE id = $1 FOR UPDATE",
            )
            .bind(bundle_id)
            .fetch_one(&mut *tx)
            .await
            .context("failed to lock CF-native bundle lineage")?;
            if current_draft.is_some() {
                return Err(anyhow::Error::new(NativeReconcileFailure {
                    conflicts: vec![ReconcileConflict::VersionDigestMismatch {
                        lineage_id: bundle_id,
                        version_id: bundle_version_id,
                        local_digest: "current draft already exists".into(),
                        imported_digest: bundle_digest,
                        source_rule_id: "bundle".into(),
                    }],
                }));
            }
            insert_native_bundle_version(
                &mut tx,
                importing_user_id,
                &validated,
                bundle_id,
                bundle_version_id,
                &bundle_digest,
            )
            .await?;
            (true, false)
        }
        None => {
            return Err(anyhow::anyhow!("bundle reconciliation plan has no decision"));
        }
    };

    if bundle_version_created {
        sqlx::query("UPDATE compliance_bundle_versions SET source_artifact_id = $1 WHERE id = $2")
            .bind(source_artifact_id)
            .bind(bundle_version_id)
            .execute(&mut *tx)
            .await
            .context("failed to attach CF-native source artifact")?;
    }

    for record in &policy_records {
        let policy_version_id = resolved_by_version[&record.policy_version_id];
        // Only write membership when this commit created the bundle version.
        // On ReuseExact the planner already verified that the existing version
        // has identical digest and membership, so the rows are present and
        // rewriting them is redundant. A re-insert would also fire the
        // membership immutability BEFORE trigger on published bundles and
        // break the documented "an identical version is reused" contract.
        if bundle_version_created {
            sqlx::query(
                "INSERT INTO compliance_bundle_version_policies \
                 (bundle_version_id, policy_version_id, policy_order, selected) VALUES ($1, $2, $3, $4) \
                 ON CONFLICT DO NOTHING",
            )
            .bind(bundle_version_id)
            .bind(policy_version_id)
            .bind(record.policy_order)
            .bind(record.selected)
            .execute(&mut *tx)
            .await
            .context("failed to persist CF-native bundle membership")?;
        }
        upsert_native_mapping(
            &mut tx,
            source_artifact_id,
            "rule",
            &record.source_rule_id,
            Some(policy_version_id),
            None,
        )
        .await?;
    }
    if bundle_version_created {
        sqlx::query("UPDATE compliance_bundle_versions SET semantic_digest = $1 WHERE id = $2")
            .bind(&bundle_digest)
            .bind(bundle_version_id)
            .execute(&mut *tx)
            .await
            .context("failed to finalize CF-native bundle digest")?;
    }
    if let Some(benchmark) = pkg.parsed.benchmark.as_ref() {
        upsert_native_mapping(
            &mut tx,
            source_artifact_id,
            "benchmark",
            &benchmark.id,
            None,
            Some(bundle_version_id),
        )
        .await?;
    }
    for profile in &pkg.parsed.profiles {
        upsert_native_mapping(
            &mut tx,
            source_artifact_id,
            "profile",
            &profile.id,
            None,
            Some(bundle_version_id),
        )
        .await?;
    }
    for group in &pkg.parsed.groups {
        upsert_native_mapping(
            &mut tx,
            source_artifact_id,
            "group",
            &group.id,
            None,
            Some(bundle_version_id),
        )
        .await?;
    }
    let metadata = serde_json::json!({
        "source_artifact_id": source_artifact_id,
        "original_sha256": source_sha256,
        "bundle_id": bundle_id,
        "bundle_version_id": bundle_version_id,
        "created_policy_lineages": created_lineages,
         "created_policy_versions": created_versions,
         "reused_policy_versions": reused_versions,
         "bundle_lineage_created": bundle_lineage_created,
         "bundle_version_created": bundle_version_created,
        "conflict_count": 0,
        "trust_state": "untrusted",
        "publication_state": "draft"
    });
    sqlx::query(
        "INSERT INTO admin_audit_events (actor_user_id, actor_identifier, action, target, metadata) \
         VALUES ($1, $1::text, 'xccdf_imported', $2, $3)",
    )
    .bind(importing_user_id)
    .bind(format!("bundle:{bundle_id}"))
    .bind(metadata)
    .execute(&mut *tx)
    .await
    .context("failed to write CF-native audit event")?;
    tx.commit()
        .await
        .context("failed to commit CF-native import")?;

    Ok(XccdfCommittedImportResult {
        source_artifact_id,
        bundle_id,
        bundle_version_id,
        created_policy_count: created_versions,
        created_policy_lineages: created_lineages,
        created_policy_versions: created_versions,
        reused_policy_versions: reused_versions,
        bundle_lineage_created,
        bundle_version_created,
        excluded_rule_count: 0,
        created_policy_version_ids: created_version_ids,
        source_sha256,
        bundle_semantic_digest: bundle_digest,
        warnings: vec![],
    })
}

async fn upsert_source_artifact(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    importing_user_id: Uuid,
    pkg: &ProcessedXccdfPackage,
) -> Result<Uuid> {
    let media_type = match pkg.provenance.package_kind {
        crate::compliance::xccdf::zip_extractor::PackageKind::Xml => "application/xml",
        crate::compliance::xccdf::zip_extractor::PackageKind::Zip => "application/zip",
    };
    let filename = pkg.provenance.filename.as_deref().unwrap_or("unknown");
    let detected = pkg.parsed.xccdf_namespace_version.map(str::to_owned);
    let context = build_package_context(&pkg.provenance);
    sqlx::query(
        "INSERT INTO compliance_source_artifacts \
         (content, filename, media_type, sha256, parser_version, detected_xccdf_version, package_context, imported_by) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (sha256) DO NOTHING",
    )
    .bind(&pkg.original_bytes)
    .bind(filename)
    .bind(media_type)
    .bind(&pkg.provenance.sha256)
    .bind(CF_PARSER_VERSION)
    .bind(detected)
    .bind(context)
    .bind(importing_user_id)
    .execute(&mut **tx)
    .await
    .context("failed to upsert source artifact")?;
    sqlx::query_scalar("SELECT id FROM compliance_source_artifacts WHERE sha256 = $1")
        .bind(&pkg.provenance.sha256)
        .fetch_one(&mut **tx)
        .await
        .context("failed to load source artifact")
}

async fn create_native_policy_lineage_and_version(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: Uuid,
    record: &ImportedPolicyRecord,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO deployment_policies (id, name, description, policy_type, config, enabled) \
         VALUES ($1,$2,$3,$4,$5,false)",
    )
    .bind(record.policy_id)
    .bind(&record.name)
    .bind(&record.description)
    .bind(&record.policy_type)
    .bind(&record.config)
    .execute(&mut **tx)
    .await
    .context("failed to create CF-native policy lineage")?;
    let generated: Uuid = sqlx::query_scalar(
        "SELECT current_draft_version_id FROM deployment_policies WHERE id = $1",
    )
    .bind(record.policy_id)
    .fetch_one(&mut **tx)
    .await?;
    sqlx::query("DELETE FROM deployment_policy_versions WHERE id = $1")
        .bind(generated)
        .execute(&mut **tx)
        .await
        .context("failed to replace trigger-created policy draft")?;
    insert_native_policy_version(tx, user_id, record).await
}

async fn insert_native_policy_version(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: Uuid,
    record: &ImportedPolicyRecord,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO deployment_policy_versions \
         (id, policy_id, version, publication_state, name, description, policy_type, \
          implementation_state, execution_phase, config, compliance_metadata, dependencies, \
          semantic_digest, digest_algorithm, canonicalization_version, source_artifact_id, \
          opaque_xml, created_by, enabled_by_default) \
         VALUES ($1,$2,$3,'draft',$4,$5,$6,$7,$8,$9,$10,$11,$12,'sha-256','cf-model-json-1',$13,$14,$15,$16)",
    )
    .bind(record.policy_version_id)
    .bind(record.policy_id)
    .bind(record.version.as_deref().unwrap_or("0.1.0"))
    .bind(&record.name)
    .bind(&record.description)
    .bind(&record.policy_type)
    .bind(&record.implementation_state)
    .bind(&record.execution_phase)
    .bind(&record.config)
    .bind(&record.compliance_metadata)
    .bind(&record.dependencies)
    .bind(record.semantic_digest.as_deref().unwrap_or("pending"))
    .bind(Option::<Uuid>::None)
    .bind(&record.opaque_xml)
    .bind(user_id)
    .bind(record.enabled_by_default)
    .execute(&mut **tx)
    .await
    .context("failed to create CF-native policy version")?;
    sqlx::query("UPDATE deployment_policies SET current_draft_version_id = $1 WHERE id = $2")
        .bind(record.policy_version_id)
        .bind(record.policy_id)
        .execute(&mut **tx)
        .await
        .context("failed to set CF-native policy draft pointer")?;
    Ok(())
}

async fn create_native_bundle_lineage(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: Uuid,
    validated: &ValidatedImportPlan,
    bundle_id: Uuid,
    bundle_version_id: Uuid,
    digest: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO compliance_bundles (id,name,framework,version,description,layer,owner) \
         VALUES ($1,$2,$3,$4,$5,$6,$7)",
    )
    .bind(bundle_id)
    .bind(&validated.bundle.name)
    .bind(&validated.bundle.framework)
    .bind(&validated.bundle.version)
    .bind(&validated.bundle.description)
    .bind(validated.bundle.layer.as_deref().unwrap_or("fleet"))
    .bind(
        validated
            .bundle
            .owner
            .as_deref()
            .unwrap_or("Platform Security"),
    )
    .execute(&mut **tx)
    .await
    .context("failed to create CF-native bundle lineage")?;
    let generated: Uuid =
        sqlx::query_scalar("SELECT current_draft_version_id FROM compliance_bundles WHERE id = $1")
            .bind(bundle_id)
            .fetch_one(&mut **tx)
            .await?;
    sqlx::query("DELETE FROM compliance_bundle_versions WHERE id = $1")
        .bind(generated)
        .execute(&mut **tx)
        .await
        .context("failed to replace trigger-created bundle draft")?;
    insert_native_bundle_version(tx, user_id, validated, bundle_id, bundle_version_id, digest).await
}

async fn insert_native_bundle_version(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: Uuid,
    validated: &ValidatedImportPlan,
    bundle_id: Uuid,
    bundle_version_id: Uuid,
    digest: &str,
) -> Result<()> {
    // The CF-native document carries no bundle version string; the stored
    // value is display metadata and is not part of the portable identity or
    // the semantic digest. Keep the placeholder label for the first version
    // of a lineage and deterministically disambiguate with a short form of
    // the portable version UUID when the label is already taken, so a new
    // version in an existing lineage satisfies the UNIQUE (bundle_id, version)
    // constraint instead of failing with a raw constraint error.
    let version_label = {
        let base = "0.1.0";
        let taken: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM compliance_bundle_versions \
             WHERE bundle_id = $1 AND version = $2 LIMIT 1",
        )
        .bind(bundle_id)
        .bind(base)
        .fetch_optional(&mut **tx)
        .await
        .context("failed to check CF-native bundle version label")?;
        match taken {
            Some(existing) if existing != bundle_version_id => {
                format!("{}-{}", base, &bundle_version_id.simple().to_string()[..8])
            }
            _ => base.to_string(),
        }
    };
    sqlx::query(
        "INSERT INTO compliance_bundle_versions \
         (id,bundle_id,version,publication_state,name,framework,framework_version,description,layer,owner,semantic_digest,source_artifact_id,created_by) \
         VALUES ($1,$2,$3,'draft',$4,$5,$6,$7,$8,$9,$10,NULL,$11)",
    )
    .bind(bundle_version_id)
    .bind(bundle_id)
    .bind(&version_label)
    .bind(&validated.bundle.name)
    .bind(&validated.bundle.framework)
    .bind(&validated.bundle.version)
    .bind(&validated.bundle.description)
    .bind(validated.bundle.layer.as_deref().unwrap_or("fleet"))
    .bind(validated.bundle.owner.as_deref().unwrap_or("Platform Security"))
    .bind(digest)
    .bind(user_id)
    .execute(&mut **tx)
    .await
    .context("failed to create CF-native bundle version")?;
    sqlx::query("UPDATE compliance_bundles SET current_draft_version_id = $1 WHERE id = $2")
        .bind(bundle_version_id)
        .bind(bundle_id)
        .execute(&mut **tx)
        .await
        .context("failed to set CF-native bundle draft pointer")?;
    sqlx::query("UPDATE compliance_bundle_versions SET semantic_digest = $1 WHERE id = $2")
        .bind(digest)
        .bind(bundle_version_id)
        .execute(&mut **tx)
        .await
        .context("failed to restore CF-native bundle digest")?;
    Ok(())
}

async fn upsert_native_mapping(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    source_artifact_id: Uuid,
    object_kind: &str,
    source_identity: &str,
    policy_version_id: Option<Uuid>,
    bundle_version_id: Option<Uuid>,
) -> Result<()> {
    let existing: Option<(Option<Uuid>, Option<Uuid>)> = sqlx::query_as(
        "SELECT policy_version_id, bundle_version_id FROM compliance_source_object_mappings \
         WHERE source_artifact_id=$1 AND object_kind=$2 AND source_identity=$3",
    )
    .bind(source_artifact_id)
    .bind(object_kind)
    .bind(source_identity)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some((existing_policy, existing_bundle)) = existing {
        if existing_policy != policy_version_id || existing_bundle != bundle_version_id {
            anyhow::bail!(
                "CF_NATIVE_MAPPING_CONFLICT: source mapping already targets a different local object"
            );
        }
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO compliance_source_object_mappings \
         (source_artifact_id,object_kind,source_identity,policy_version_id,bundle_version_id,fidelity) \
         VALUES ($1,$2,$3,$4,$5,'native_exact')",
    )
    .bind(source_artifact_id)
    .bind(object_kind)
    .bind(source_identity)
    .bind(policy_version_id)
    .bind(bundle_version_id)
    .execute(&mut **tx)
    .await
    .context("failed to persist CF-native source mapping")?;
    Ok(())
}

// ── Database-backed tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_xccdf_bytes() -> Vec<u8> {
        br#"<?xml version="1.0" encoding="UTF-8"?>
<Benchmark xmlns="http://checklists.nist.gov/xccdf/1.2" id="xccdf_test_benchmark">
  <status>draft</status>
  <title>Test Benchmark</title>
  <version>1.0</version>
  <Rule id="xccdf_test_rule_001">
    <title>Rule One</title>
    <description>First rule</description>
    <check system="urn:test">
      <check-content>Verify the setting.</check-content>
    </check>
  </Rule>
  <Rule id="xccdf_test_rule_002">
    <title>Rule Two</title>
    <check system="urn:test">
      <check-content>Verify the other setting.</check-content>
    </check>
  </Rule>
</Benchmark>"#
            .to_vec()
    }

    fn make_package(bytes: Vec<u8>) -> ProcessedXccdfPackage {
        use crate::compliance::interchange::InterchangeLimits;
        use crate::compliance::xccdf::package::process_xccdf_bytes;
        process_xccdf_bytes(
            bytes,
            Some("test.xml".into()),
            &InterchangeLimits::default(),
        )
        .expect("valid test package")
    }

    fn make_plan(
        pkg: &ProcessedXccdfPackage,
        rule_ids: &[&str],
    ) -> (ValidatedImportPlan, Vec<ImportedPolicyRecord>) {
        use crate::compliance::xccdf::import_models::{
            ImportedBundlePlan, XccdfImportPlan, XccdfRuleImportAction,
        };
        use crate::compliance::xccdf::importer::validate_import_plan;

        let plan = XccdfImportPlan {
            expected_sha256: pkg.provenance.sha256.clone(),
            selected_profile_id: None,
            selected_rule_ids: rule_ids.iter().map(|s| s.to_string()).collect(),
            rule_actions: rule_ids
                .iter()
                .map(|id| XccdfRuleImportAction::CreateManual {
                    rule_id: id.to_string(),
                    customization: Default::default(),
                    evidence_requirements: Vec::new(),
                })
                .collect(),
            mapping_semantics: std::collections::HashMap::new(),

            shared_group_decisions: Vec::new(),

            bundle: ImportedBundlePlan {
                name: "Test Import Bundle".into(),
                framework: "XCCDF-TEST".into(),
                version: "1.0".into(),
                layer: Some("os".into()),
                owner: Some("Security Team".into()),
                description: Some("Imported from test fixture".into()),
            },
        };

        let mut validated = validate_import_plan(plan, &pkg.parsed).expect("valid plan");
        let suffix = Uuid::new_v4().simple().to_string();
        validated.bundle.name = format!("{}-{suffix}", validated.bundle.name);
        let mut records = build_policy_records(&validated);
        for record in &mut records {
            record.name = format!("{}-{suffix}", record.name);
        }
        (validated, records)
    }

    async fn test_pool() -> Option<PgPool> {
        let url = std::env::var("DATABASE_URL").ok()?;
        sqlx::postgres::PgPool::connect(&url).await.ok()
    }

    fn native_fixture_bytes() -> (Vec<u8>, Uuid, Uuid, Uuid, Uuid) {
        let bundle_id = Uuid::new_v4();
        let bundle_version_id = Uuid::new_v4();
        let policy_id = Uuid::new_v4();
        let policy_version_id = Uuid::new_v4();
        let bytes = native_fixture_bytes_with(
            bundle_id,
            bundle_version_id,
            policy_id,
            policy_version_id,
            true,
        );
        (
            bytes,
            bundle_id,
            bundle_version_id,
            policy_id,
            policy_version_id,
        )
    }

    /// Build a CF-native fixture with explicit identities.
    ///
    /// `strict` toggles a field inside the policy config so callers can
    /// produce a same-identity document whose semantic digest differs.
    fn native_fixture_bytes_with(
        bundle_id: Uuid,
        bundle_version_id: Uuid,
        policy_id: Uuid,
        policy_version_id: Uuid,
        strict: bool,
    ) -> Vec<u8> {
        native_fixture_bytes_with_policy_version(
            bundle_id,
            bundle_version_id,
            policy_id,
            policy_version_id,
            strict,
            "1.0",
        )
    }

    /// Variant that also controls the portable `<cf:policy-version>` label.
    ///
    /// Policy lineages enforce `UNIQUE (policy_id, version)`, so a document
    /// that creates a second version in an existing lineage must carry a
    /// distinct version label.
    fn native_fixture_bytes_with_policy_version(
        bundle_id: Uuid,
        bundle_version_id: Uuid,
        policy_id: Uuid,
        policy_version_id: Uuid,
        strict: bool,
        policy_version_label: &str,
    ) -> Vec<u8> {
        use crate::compliance::digest::{BundleVersionCanonical, PolicyVersionCanonical};
        let bundle_name = format!("Native Fixture Bundle {bundle_id}");
        let policy_name = format!("Native Agent Requirement {policy_id}");
        let config = serde_json::json!({
            "mode": "all",
            "context": "nixos-configuration-v1",
            "binding": "cfg",
            "rules": [{
                "field_name": "agentEnabled",
                "description": "The agent is enabled",
                "expression": "cfg.config.services.crystal-forge-agent.enable",
                "strict": strict
            }]
        });
        let metadata = serde_json::json!({});
        let dependencies = serde_json::json!([]);
        let policy_digest = PolicyVersionCanonical {
            name: policy_name.clone(),
            description: Some("Requires the Crystal Forge agent".into()),
            policy_type: "custom_check".into(),
            implementation_state: "native".into(),
            execution_phase: "nix-evaluation".into(),
            config: config.clone(),
            compliance_metadata: metadata.clone(),
            dependencies: dependencies.clone(),
            opaque_xml_digest: None,
            enabled_by_default: Some(true),
        }
        .compute_digest();
        let bundle_digest = BundleVersionCanonical {
            name: bundle_name.clone(),
            framework: "CF-TEST".into(),
            framework_version: Some("1.0".into()),
            description: Some("Native fixture".into()),
            layer: "os".into(),
            owner: "Tests".into(),
            members: vec![crate::compliance::digest::BundleMembershipEntry {
                policy_version_id,
                selected: true,
            }],
        }
        .compute_digest();
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<Benchmark xmlns="http://checklists.nist.gov/xccdf/1.2" xmlns:cf="urn:crystal-forge:xccdf:1" id="xccdf_test_native_{bundle_version_id}">
  <status>draft</status><title>{bundle_name}</title><description>Native fixture</description><version>1.0</version>
  <cf:bundle schema-version="1" bundle-id="urn:uuid:{bundle_id}" bundle-version-id="urn:uuid:{bundle_version_id}" publication-state="draft">
    <cf:framework name="CF-TEST" version="1.0"/><cf:layer>os</cf:layer><cf:owner>Tests</cf:owner>
    <cf:content-digest algorithm="sha-256" canonical-model="cf-model-json-1">{bundle_digest}</cf:content-digest>
  </cf:bundle>
  <Profile id="xccdf_test_profile"><select idref="xccdf_test_rule_{policy_version_id}" selected="true"/></Profile>
  <Rule id="xccdf_test_rule_{policy_version_id}"><title>{policy_name}</title><description>Requires the Crystal Forge agent</description>
      <cf:policy-identity policy-id="urn:uuid:{policy_id}" policy-version-id="urn:uuid:{policy_version_id}" publication-state="draft" enabled-default="true" implementation-state="native" selected="true" policy-order="0">
        <cf:policy-version>{policy_version_label}</cf:policy-version><cf:content-digest algorithm="sha-256" canonical-model="cf-model-json-1">{policy_digest}</cf:content-digest>
      </cf:policy-identity>
    <check system="urn:crystal-forge:check-system:policy:1"><check-content><cf:policy schema-version="1" policy-type="custom_check"><cf:execution phase="nix-evaluation" strict="true"/><cf:implementation state="native"><cf:custom-check mode="all" context="nixos-configuration-v1" binding="cfg"><cf:rule field-name="agentEnabled" strict="true"><cf:description>The agent is enabled</cf:description><cf:expression language="nix">cfg.config.services.crystal-forge-agent.enable</cf:expression></cf:rule></cf:custom-check></cf:implementation><cf:config-json>{config}</cf:config-json><cf:compliance-metadata-json>{metadata}</cf:compliance-metadata-json><cf:dependencies-json>{dependencies}</cf:dependencies-json></cf:policy></check-content></check>
  </Rule>
</Benchmark>"#
        );
        xml.into_bytes()
    }

    /// Insert a test user and return its UUID. The user owns the imported
    /// source artifact (FK users.id) and the audit event.
    async fn ensure_test_user(pool: &PgPool) -> Uuid {
        use crate::queries::users::insert_user;
        let email = format!("xccdf-import-test-{}@example.test", Uuid::new_v4().simple());
        let user = insert_user(pool, &email, Some("XCCDF Import Test"))
            .await
            .expect("test user");
        user.id
    }

    fn disa_stig_bytes(benchmark_id: &str, release: &str, rules: &[(&str, &str)]) -> Vec<u8> {
        let rules = rules
            .iter()
            .enumerate()
            .map(|(index, (vuln_id, title))| {
                format!(
                    r#"  <Rule id="xccdf_test_stig_rule_{index}"><title>{title}</title><description>Stable requirement {vuln_id}</description><ident system="http://cyber.mil/stigs/stig">{vuln_id}</ident><check system="urn:test"><check-content>Verify {vuln_id}.</check-content></check></Rule>"#
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<Benchmark xmlns="http://checklists.nist.gov/xccdf/1.2" id="{benchmark_id}">
  <status>draft</status><title>MapExisting Test STIG</title><version>{release}</version>
{rules}
</Benchmark>"#
        )
        .into_bytes()
    }

    fn map_existing_plan(
        pkg: &ProcessedXccdfPackage,
        policy_version_id: Uuid,
    ) -> (ValidatedImportPlan, Vec<ImportedPolicyRecord>) {
        use crate::compliance::xccdf::import_models::{
            ImportedBundlePlan, XccdfImportPlan, XccdfRuleImportAction,
        };
        use crate::compliance::xccdf::importer::validate_import_plan;

        let rule_ids: Vec<String> = pkg
            .parsed
            .rules
            .iter()
            .map(|rule| rule.id.clone())
            .collect();
        let plan = XccdfImportPlan {
            expected_sha256: pkg.provenance.sha256.clone(),
            selected_profile_id: None,
            selected_rule_ids: rule_ids.clone(),
            rule_actions: rule_ids
                .iter()
                .map(|rule_id| XccdfRuleImportAction::MapExisting {
                    rule_id: rule_id.clone(),
                    policy_version_id,
                    proof: None,
                })
                .collect(),
            mapping_semantics: std::collections::HashMap::new(),

            shared_group_decisions: Vec::new(),

            bundle: ImportedBundlePlan {
                name: format!("MapExisting STIG bundle {}", Uuid::new_v4()),
                framework: "DISA STIG".into(),
                version: "test".into(),
                layer: Some("os".into()),
                owner: Some("Security Team".into()),
                description: None,
            },
        };
        let validated = validate_import_plan(plan, &pkg.parsed).expect("valid MapExisting plan");
        let records = build_policy_records(&validated);
        (validated, records)
    }

    async fn publish_policy_version(
        pool: &PgPool,
        actor_id: Uuid,
        policy_id: Uuid,
        policy_version_id: Uuid,
    ) {
        let mut tx = pool
            .begin()
            .await
            .expect("begin policy publish transaction");
        sqlx::query(
            "UPDATE deployment_policy_versions SET trust_state = 'trusted', trusted_by = $2, trusted_at = CURRENT_TIMESTAMP WHERE id = $1",
        )
        .bind(policy_version_id)
        .bind(actor_id)
        .execute(&mut *tx)
        .await
        .expect("trust policy version");
        sqlx::query(
            "UPDATE deployment_policies SET current_draft_version_id = NULL WHERE id = $1 AND current_draft_version_id = $2",
        )
        .bind(policy_id)
        .bind(policy_version_id)
        .execute(&mut *tx)
        .await
        .expect("clear policy draft pointer");
        sqlx::query(
            "UPDATE deployment_policy_versions SET publication_state = 'accepted', published_at = CURRENT_TIMESTAMP WHERE id = $1",
        )
        .bind(policy_version_id)
        .execute(&mut *tx)
        .await
        .expect("accept policy version");
        sqlx::query(
            "UPDATE deployment_policies SET current_published_version_id = $1 WHERE id = $2",
        )
        .bind(policy_version_id)
        .bind(policy_id)
        .execute(&mut *tx)
        .await
        .expect("set published policy pointer");
        tx.commit()
            .await
            .expect("commit policy publish transaction");
    }

    struct MapExistingSource {
        policy_id: Uuid,
        policy_version_id: Uuid,
        requirement_version_ids: Vec<Uuid>,
        benchmark_id: String,
    }

    async fn prepare_map_existing_source(
        pool: &PgPool,
        user_id: Uuid,
        mapping_trust_state: &str,
        publish: bool,
    ) -> MapExistingSource {
        let benchmark_id = format!(
            "xccdf_mil.disa.stig_benchmark_MapExisting_Test_STIG_{}",
            Uuid::new_v4().simple()
        );
        let pkg = make_package(disa_stig_bytes(
            &benchmark_id,
            "V1R1",
            &[
                ("V-418-001", "First stable requirement"),
                ("V-418-002", "Second stable requirement"),
            ],
        ));
        let (validated, records) =
            make_plan(&pkg, &["xccdf_test_stig_rule_0", "xccdf_test_stig_rule_1"]);
        let result = commit_foreign_import(pool, user_id, pkg, validated, records)
            .await
            .expect("import prior STIG release");
        let policy_version_id = result.created_policy_version_ids[0];
        let policy_id: Uuid =
            sqlx::query_scalar("SELECT policy_id FROM deployment_policy_versions WHERE id = $1")
                .bind(policy_version_id)
                .fetch_one(pool)
                .await
                .expect("load source policy lineage");
        let requirement_version_ids: Vec<Uuid> = sqlx::query_scalar(
            "SELECT requirement_version_id FROM compliance_bundle_version_requirements WHERE bundle_version_id = $1 ORDER BY requirement_order",
        )
        .bind(result.bundle_version_id)
        .fetch_all(pool)
        .await
        .expect("load prior requirement versions");

        // Both prior requirements deliberately share the one source policy and
        // carry non-default mapping semantics that the inherited reuse must copy.
        sqlx::query(
            "UPDATE policy_requirement_mappings SET relationship = 'supports', coverage = 'partial', rationale = 'shared inherited rationale', trust_state = $2 WHERE policy_version_id = $1",
        )
        .bind(policy_version_id)
        .bind(mapping_trust_state)
        .execute(pool)
        .await
        .expect("update first source mapping semantics");
        sqlx::query(
            "INSERT INTO policy_requirement_mappings (policy_version_id, requirement_version_id, relationship, coverage, rationale, provenance, trust_state, created_by) VALUES ($1, $2, 'supports', 'partial', 'shared inherited rationale', 'manual', $3, $4)",
        )
        .bind(policy_version_id)
        .bind(requirement_version_ids[1])
        .bind(mapping_trust_state)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("map shared source policy to second requirement");

        if publish {
            publish_policy_version(pool, user_id, policy_id, policy_version_id).await;
        }
        MapExistingSource {
            policy_id,
            policy_version_id,
            requirement_version_ids,
            benchmark_id,
        }
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn map_existing_stig_reuses_current_policy_draft_and_preserves_inherited_mappings() {
        let pool = test_pool().await.expect("DATABASE_URL required");
        let user_id = ensure_test_user(&pool).await;
        let source = prepare_map_existing_source(&pool, user_id, "trusted", true).await;
        let source_version_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM deployment_policy_versions WHERE policy_id = $1",
        )
        .bind(source.policy_id)
        .fetch_one(&pool)
        .await
        .expect("count source policy versions before reuse");
        let pkg = make_package(disa_stig_bytes(
            &source.benchmark_id,
            "V1R2",
            &[
                ("V-418-001", "First stable requirement"),
                ("V-418-002", "Second stable requirement"),
            ],
        ));
        let (validated, records) = map_existing_plan(&pkg, source.policy_version_id);
        let result = commit_foreign_import(&pool, user_id, pkg, validated, records)
            .await
            .expect("current accepted STIG mapping should be reused");

        assert_eq!(result.created_policy_count, 0);
        assert_eq!(result.created_policy_versions, 0);
        assert_eq!(result.reused_policy_versions, 1);
        assert!(result.created_policy_version_ids.is_empty());

        let draft_id: Uuid = sqlx::query_scalar(
            "SELECT current_draft_version_id FROM deployment_policies WHERE id = $1",
        )
        .bind(source.policy_id)
        .fetch_one(&pool)
        .await
        .expect("load derived policy draft");
        assert_ne!(draft_id, source.policy_version_id);
        let derived_from: Option<Uuid> = sqlx::query_scalar(
            "SELECT derived_from_version_id FROM deployment_policy_versions WHERE id = $1",
        )
        .bind(draft_id)
        .fetch_one(&pool)
        .await
        .expect("load derived draft provenance");
        assert_eq!(derived_from, Some(source.policy_version_id));

        let members: Vec<(Uuid, i32)> = sqlx::query_as(
            "SELECT policy_version_id, policy_order FROM compliance_bundle_version_policies WHERE bundle_version_id = $1 ORDER BY policy_order",
        )
        .bind(result.bundle_version_id)
        .fetch_all(&pool)
        .await
        .expect("load bundle membership");
        assert_eq!(members, vec![(draft_id, 0)]);

        let mappings: Vec<(Uuid, String, String, Option<String>, String)> = sqlx::query_as(
            "SELECT requirement_version_id, relationship, coverage, rationale, provenance FROM policy_requirement_mappings WHERE policy_version_id = $1 ORDER BY requirement_version_id",
        )
        .bind(draft_id)
        .fetch_all(&pool)
        .await
        .expect("load inherited mappings");
        assert_eq!(mappings.len(), 2);
        assert_eq!(
            mappings
                .iter()
                .map(|(_, relationship, coverage, rationale, provenance)| (
                    relationship.as_str(),
                    coverage.as_str(),
                    rationale.as_deref(),
                    provenance.as_str(),
                ))
                .collect::<Vec<_>>(),
            vec![
                (
                    "supports",
                    "partial",
                    Some("shared inherited rationale"),
                    "inherited"
                ),
                (
                    "supports",
                    "partial",
                    Some("shared inherited rationale"),
                    "inherited"
                ),
            ]
        );
        assert_ne!(mappings[0].0, source.requirement_version_ids[0]);
        assert_ne!(mappings[1].0, source.requirement_version_ids[1]);

        let version_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM deployment_policy_versions WHERE policy_id = $1",
        )
        .bind(source.policy_id)
        .fetch_one(&pool)
        .await
        .expect("count source policy versions");
        assert_eq!(
            version_count,
            source_version_count + 1,
            "both rules must reuse one existing mutable draft"
        );

        let audit_counts: (i64, i64) = sqlx::query_as(
            "SELECT (metadata->>'created_policy_count')::bigint, (metadata->>'reused_policy_versions')::bigint FROM admin_audit_events WHERE (metadata->>'bundle_id')::uuid = $1",
        )
        .bind(result.bundle_id)
        .fetch_one(&pool)
        .await
        .expect("load import accounting audit event");
        assert_eq!(audit_counts, (0, 1));
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn map_existing_stig_rejects_non_current_nonaccepted_untrusted_and_changed_sources() {
        let pool = test_pool().await.expect("DATABASE_URL required");
        let user_id = ensure_test_user(&pool).await;

        let mutable = prepare_map_existing_source(&pool, user_id, "trusted", false).await;
        let pkg = make_package(disa_stig_bytes(
            &mutable.benchmark_id,
            "V1R2",
            &[
                ("V-418-001", "First stable requirement"),
                ("V-418-002", "Second stable requirement"),
            ],
        ));
        let (validated, records) = map_existing_plan(&pkg, mutable.policy_version_id);
        let error = commit_foreign_import(&pool, user_id, pkg, validated, records)
            .await
            .expect_err("mutable selected version must be rejected");
        assert!(error.to_string().contains("IMPORT_REUSE_INELIGIBLE"));

        let untrusted = prepare_map_existing_source(&pool, user_id, "suggested", true).await;
        let pkg = make_package(disa_stig_bytes(
            &untrusted.benchmark_id,
            "V1R2",
            &[
                ("V-418-001", "First stable requirement"),
                ("V-418-002", "Second stable requirement"),
            ],
        ));
        let (validated, records) = map_existing_plan(&pkg, untrusted.policy_version_id);
        let error = commit_foreign_import(&pool, user_id, pkg, validated, records)
            .await
            .expect_err("untrusted source mapping must be rejected");
        assert!(error.to_string().contains("IMPORT_REUSE_INELIGIBLE"));

        let superseded = prepare_map_existing_source(&pool, user_id, "trusted", true).await;
        let mut tx = pool
            .begin()
            .await
            .expect("begin superseding draft transaction");
        let replacement = ensure_policy_draft(
            &mut tx,
            superseded.policy_id,
            Some(user_id),
            None,
            PolicyDraftIntent::EnsureMutable,
        )
        .await
        .expect("derive replacement draft");
        sqlx::query(
            "INSERT INTO policy_requirement_mappings (policy_version_id, requirement_version_id, relationship, coverage, provenance, trust_state, created_by) VALUES ($1, $2, 'implements', 'full', 'manual', 'trusted', $3)",
        )
        .bind(replacement)
        .bind(superseded.requirement_version_ids[0])
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .expect("map replacement draft");
        tx.commit().await.expect("commit replacement draft");
        sqlx::query(
            "UPDATE deployment_policy_versions SET publication_state = 'deprecated' WHERE id = $1",
        )
        .bind(superseded.policy_version_id)
        .execute(&pool)
        .await
        .expect("retire superseded source policy version");
        publish_policy_version(&pool, user_id, superseded.policy_id, replacement).await;
        let pkg = make_package(disa_stig_bytes(
            &superseded.benchmark_id,
            "V1R2",
            &[
                ("V-418-001", "First stable requirement"),
                ("V-418-002", "Second stable requirement"),
            ],
        ));
        let (validated, records) = map_existing_plan(&pkg, superseded.policy_version_id);
        let error = commit_foreign_import(&pool, user_id, pkg, validated, records)
            .await
            .expect_err("superseded selected version must be rejected");
        assert!(error.to_string().contains("IMPORT_REUSE_INELIGIBLE"));

        let deprecated = prepare_map_existing_source(&pool, user_id, "trusted", true).await;
        sqlx::query(
            "UPDATE deployment_policy_versions SET publication_state = 'deprecated' WHERE id = $1",
        )
        .bind(deprecated.policy_version_id)
        .execute(&pool)
        .await
        .expect("deprecate selected policy version");
        let pkg = make_package(disa_stig_bytes(
            &deprecated.benchmark_id,
            "V1R2",
            &[
                ("V-418-001", "First stable requirement"),
                ("V-418-002", "Second stable requirement"),
            ],
        ));
        let (validated, records) = map_existing_plan(&pkg, deprecated.policy_version_id);
        let error = commit_foreign_import(&pool, user_id, pkg, validated, records)
            .await
            .expect_err("deprecated selected version must be rejected");
        assert!(error.to_string().contains("IMPORT_REUSE_INELIGIBLE"));

        let changed = prepare_map_existing_source(&pool, user_id, "trusted", true).await;
        let pkg = make_package(disa_stig_bytes(
            &changed.benchmark_id,
            "V1R2",
            &[
                ("V-418-001", "Changed requirement"),
                ("V-418-002", "Second stable requirement"),
            ],
        ));
        let (validated, records) = map_existing_plan(&pkg, changed.policy_version_id);
        let error = commit_foreign_import(&pool, user_id, pkg, validated, records)
            .await
            .expect_err("changed requirement must not inherit a prior mapping");
        assert!(error.to_string().contains("IMPORT_REUSE_INELIGIBLE"));
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn map_existing_stig_rolls_back_derived_draft_and_import_rows_on_late_failure() {
        let pool = test_pool().await.expect("DATABASE_URL required");
        let user_id = ensure_test_user(&pool).await;
        let source = prepare_map_existing_source(&pool, user_id, "trusted", true).await;
        let source_version_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM deployment_policy_versions WHERE policy_id = $1",
        )
        .bind(source.policy_id)
        .fetch_one(&pool)
        .await
        .expect("count source policy versions before failed reuse");
        let pkg = make_package(disa_stig_bytes(
            &source.benchmark_id,
            "V1R2",
            &[
                ("V-418-001", "First stable requirement"),
                ("V-418-002", "Second stable requirement"),
            ],
        ));
        let failed_sha256 = pkg.provenance.sha256.clone();
        let artifact_count_before: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM compliance_source_artifacts WHERE sha256 = $1",
        )
        .bind(&failed_sha256)
        .fetch_one(&pool)
        .await
        .expect("count future artifact rows");
        let (validated, mut records) = map_existing_plan(&pkg, source.policy_version_id);
        let bundle_name = validated.bundle.name.clone();
        records[1].mapped_policy_version_id = Some(Uuid::new_v4());
        let error = commit_foreign_import(&pool, user_id, pkg, validated, records)
            .await
            .expect_err("second invalid mapping must fail after the first draft derivation");
        assert!(
            error
                .to_string()
                .contains("IMPORT_POLICY_VERSION_NOT_FOUND")
        );

        let artifact_count_after: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM compliance_source_artifacts WHERE sha256 = $1",
        )
        .bind(&failed_sha256)
        .fetch_one(&pool)
        .await
        .expect("count failed import artifact rows");
        let bundle_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM compliance_bundles WHERE name = $1")
                .bind(bundle_name)
                .fetch_one(&pool)
                .await
                .expect("count failed import bundle rows");
        assert_eq!(bundle_count, 0);
        assert_eq!(artifact_count_after, artifact_count_before);
        let draft_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM deployment_policy_versions WHERE policy_id = $1",
        )
        .bind(source.policy_id)
        .fetch_one(&pool)
        .await
        .expect("count derived drafts after rollback");
        assert_eq!(
            draft_count, source_version_count,
            "failed import must not retain its derived draft"
        );
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn successful_import_creates_all_expected_rows() {
        let pool = test_pool().await.expect("DATABASE_URL required");
        let user_id = ensure_test_user(&pool).await;
        let mut bytes = minimal_xccdf_bytes();
        bytes.extend_from_slice(format!("\n<!-- {} -->", Uuid::new_v4()).as_bytes());
        let pkg = make_package(bytes);
        let (validated, policy_records) =
            make_plan(&pkg, &["xccdf_test_rule_001", "xccdf_test_rule_002"]);

        let expected_sha256 = pkg.provenance.sha256.clone();
        let result = commit_foreign_import(
            &pool,
            user_id, // test user
            pkg,
            validated,
            policy_records,
        )
        .await
        .expect("import should succeed");

        // HTTP 201 contract fields.
        assert_eq!(result.created_policy_count, 2);
        assert_eq!(result.excluded_rule_count, 0);
        assert_eq!(result.created_policy_version_ids.len(), 2);
        assert_eq!(result.source_sha256, expected_sha256);
        assert!(!result.bundle_semantic_digest.is_empty());
        assert_ne!(result.bundle_semantic_digest, "pending");

        // Source artifact row.
        let artifact_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM compliance_source_artifacts WHERE id = $1)",
        )
        .bind(result.source_artifact_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(artifact_exists, "source artifact must exist");

        // Bundle lineage + version.
        let current_draft_id: Option<Uuid> = sqlx::query_scalar(
            "SELECT current_draft_version_id FROM compliance_bundles WHERE id = $1",
        )
        .bind(result.bundle_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(current_draft_id, Some(result.bundle_version_id));

        let bundle_ver_state: String = sqlx::query_scalar(
            "SELECT publication_state FROM compliance_bundle_versions WHERE id = $1",
        )
        .bind(result.bundle_version_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(bundle_ver_state, "draft");

        // Policy versions: all disabled, all draft, implementation_state = manual.
        for pvid in &result.created_policy_version_ids {
            let (pub_state, impl_state, enabled_by_default): (String, String, Option<bool>) =
                sqlx::query_as(
                    "SELECT publication_state, implementation_state, enabled_by_default \
                     FROM deployment_policy_versions WHERE id = $1",
                )
                .bind(pvid)
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(pub_state, "draft");
            assert_eq!(impl_state, "manual");
            assert_eq!(enabled_by_default, Some(false));
        }

        // Membership: count matches, order is stable.
        let member_ids: Vec<Uuid> = sqlx::query_scalar(
            "SELECT policy_version_id FROM compliance_bundle_version_policies \
             WHERE bundle_version_id = $1 ORDER BY policy_order ASC",
        )
        .bind(result.bundle_version_id)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(member_ids.len(), 2);
        assert_eq!(member_ids, result.created_policy_version_ids);

        // Source-object mappings.
        let mapping_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM compliance_source_object_mappings WHERE source_artifact_id = $1",
        )
        .bind(result.source_artifact_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(mapping_count >= 2, "at least one mapping per rule");

        // No assignments.
        let assignment_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM compliance_bundle_assignments WHERE bundle_version_id = $1",
        )
        .bind(result.bundle_version_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            assignment_count, 0,
            "no assignments must exist after import"
        );

        // No published versions.
        let published_bundle_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM compliance_bundle_versions \
             WHERE bundle_id = $1 AND publication_state = 'accepted'",
        )
        .bind(result.bundle_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(published_bundle_count, 0);

        // Audit event.
        let audit_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM admin_audit_events \
             WHERE action = 'xccdf_imported' AND (metadata->>'bundle_id')::uuid = $1)",
        )
        .bind(result.bundle_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(audit_exists, "audit event must be written");

        // Cleanup.
        cleanup_import(&pool, result.bundle_id, &result.created_policy_version_ids).await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn cf_native_reimport_reuses_exact_identities() {
        let pool = test_pool().await.expect("DATABASE_URL required");
        let user_id = ensure_test_user(&pool).await;
        let (bytes, bundle_id, bundle_version_id, policy_id, policy_version_id) =
            native_fixture_bytes();
        let pkg = make_package(bytes.clone());
        let (validated, records) =
            crate::compliance::xccdf::importer::validate_cf_native_document(&pkg.parsed)
                .expect("native fixture should validate");
        let first = commit_cf_native_import(&pool, user_id, pkg, validated, records)
            .await
            .expect("first native import");
        assert_eq!(first.created_policy_versions, 1);
        assert_eq!(first.reused_policy_versions, 0);

        let pkg = make_package(bytes);
        let (validated, records) =
            crate::compliance::xccdf::importer::validate_cf_native_document(&pkg.parsed)
                .expect("native fixture should validate on repeat");
        let second = commit_cf_native_import(&pool, user_id, pkg, validated, records)
            .await
            .expect("second native import");
        assert_eq!(second.created_policy_versions, 0);
        assert_eq!(second.reused_policy_versions, 1);
        assert_eq!(second.bundle_id, bundle_id);
        assert_eq!(second.bundle_version_id, bundle_version_id);

        let policy_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM deployment_policy_versions WHERE policy_id = $1",
        )
        .bind(policy_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let version_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM deployment_policy_versions WHERE id = $1")
                .bind(policy_version_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(policy_count, 1);
        assert_eq!(version_count, 1);
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn cf_native_concurrent_identical_imports_are_serialized() {
        let pool = test_pool().await.expect("DATABASE_URL required");
        let user_id = ensure_test_user(&pool).await;
        let (bytes, bundle_id, bundle_version_id, policy_id, policy_version_id) =
            native_fixture_bytes();

        let import = |bytes: Vec<u8>| {
            let pool = pool.clone();
            async move {
                let pkg = make_package(bytes);
                let (validated, records) =
                    crate::compliance::xccdf::importer::validate_cf_native_document(&pkg.parsed)
                        .expect("native fixture should validate");
                commit_cf_native_import(&pool, user_id, pkg, validated, records).await
            }
        };

        let (first, second) = tokio::join!(import(bytes.clone()), import(bytes));
        let first = first.expect("first concurrent native import");
        let second = second.expect("second concurrent native import");

        assert_eq!(first.bundle_id, bundle_id);
        assert_eq!(second.bundle_id, bundle_id);
        assert_eq!(first.bundle_version_id, bundle_version_id);
        assert_eq!(second.bundle_version_id, bundle_version_id);
        assert_eq!(
            first.created_policy_versions + second.created_policy_versions,
            1
        );
        assert_eq!(
            first.reused_policy_versions + second.reused_policy_versions,
            1
        );

        let policy_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM deployment_policy_versions WHERE policy_id = $1",
        )
        .bind(policy_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let version_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM deployment_policy_versions WHERE id = $1")
                .bind(policy_version_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(policy_count, 1);
        assert_eq!(version_count, 1);
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn excluded_rules_create_no_rows() {
        let pool = test_pool().await.expect("DATABASE_URL required");
        let user_id = ensure_test_user(&pool).await;
        let mut bytes = minimal_xccdf_bytes();
        bytes.extend_from_slice(format!("\n<!-- {} -->", Uuid::new_v4()).as_bytes());
        let pkg = make_package(bytes);

        use crate::compliance::xccdf::import_models::{
            ImportedBundlePlan, XccdfImportPlan, XccdfRuleImportAction,
        };
        use crate::compliance::xccdf::importer::validate_import_plan;

        let plan = XccdfImportPlan {
            expected_sha256: pkg.provenance.sha256.clone(),
            selected_profile_id: None,
            selected_rule_ids: vec!["xccdf_test_rule_001".into(), "xccdf_test_rule_002".into()],
            rule_actions: vec![
                XccdfRuleImportAction::CreateManual {
                    rule_id: "xccdf_test_rule_001".into(),
                    customization: Default::default(),
                    evidence_requirements: Vec::new(),
                },
                XccdfRuleImportAction::Exclude {
                    rule_id: "xccdf_test_rule_002".into(),
                },
            ],
            mapping_semantics: std::collections::HashMap::new(),

            shared_group_decisions: Vec::new(),

            bundle: ImportedBundlePlan {
                name: "Partial Import Bundle".into(),
                framework: "XCCDF-TEST".into(),
                version: "1.0".into(),
                layer: None,
                owner: None,
                description: None,
            },
        };

        let mut validated = validate_import_plan(plan, &pkg.parsed).unwrap();
        let suffix = Uuid::new_v4().simple().to_string();
        validated.bundle.name = format!("{}-{suffix}", validated.bundle.name);
        let mut policy_records = build_policy_records(&validated);
        for record in &mut policy_records {
            record.name = format!("{}-{suffix}", record.name);
        }

        let result = commit_foreign_import(&pool, user_id, pkg, validated, policy_records)
            .await
            .unwrap();

        assert_eq!(
            result.created_policy_count, 1,
            "only rule_001 should be created"
        );
        assert_eq!(result.excluded_rule_count, 1, "rule_002 must be excluded");

        // Verify rule_002 has no policy row.
        let rule_002_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM compliance_source_object_mappings \
             WHERE source_artifact_id = $1 AND source_identity = 'xccdf_test_rule_002')",
        )
        .bind(result.source_artifact_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            !rule_002_exists,
            "excluded rule must not have a source mapping"
        );

        cleanup_import(&pool, result.bundle_id, &result.created_policy_version_ids).await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn rollback_on_duplicate_policy_version_id() {
        // Force a failure mid-transaction to prove atomicity.
        let pool = test_pool().await.expect("DATABASE_URL required");
        let user_id = ensure_test_user(&pool).await;
        let mut bytes = minimal_xccdf_bytes();
        bytes.extend_from_slice(format!("<!-- {} -->", Uuid::new_v4()).as_bytes());
        let pkg = make_package(bytes);
        let artifact_count_before: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM compliance_source_artifacts WHERE sha256 = $1",
        )
        .bind(&pkg.provenance.sha256)
        .fetch_one(&pool)
        .await
        .unwrap();
        let (validated, mut policy_records) =
            make_plan(&pkg, &["xccdf_test_rule_001", "xccdf_test_rule_002"]);

        // Use the same policy_version_id twice to trigger a UNIQUE violation.
        let first_vid = policy_records[0].policy_version_id;
        policy_records[1].policy_id = policy_records[0].policy_id;
        policy_records[1].policy_version_id = first_vid;

        let sha256_before = &pkg.provenance.sha256.clone();
        let result = commit_foreign_import(&pool, user_id, pkg, validated, policy_records).await;

        assert!(
            result.is_err(),
            "duplicate version id must cause a transaction failure"
        );

        // Nothing must have been persisted.
        let artifact_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM compliance_source_artifacts WHERE sha256 = $1",
        )
        .bind(sha256_before)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(artifact_count, artifact_count_before);
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn preview_and_import_produce_same_digest_and_rule_set() {
        let pool = test_pool().await.expect("DATABASE_URL required");
        let user_id = ensure_test_user(&pool).await;
        let bytes = minimal_xccdf_bytes();

        // Preview.
        let preview_pkg = make_package(bytes.clone());
        let preview_sha256 = preview_pkg.provenance.sha256.clone();
        let preview_rule_ids: Vec<String> = preview_pkg
            .parsed
            .rules
            .iter()
            .map(|r| r.id.clone())
            .collect();

        // Import the same bytes.
        let import_pkg = make_package(bytes);
        let import_sha256 = import_pkg.provenance.sha256.clone();
        let (validated, policy_records) =
            make_plan(&import_pkg, &["xccdf_test_rule_001", "xccdf_test_rule_002"]);
        let import_rule_ids: Vec<String> = import_pkg
            .parsed
            .rules
            .iter()
            .map(|r| r.id.clone())
            .collect();

        assert_eq!(preview_sha256, import_sha256, "same bytes → same digest");
        assert_eq!(
            preview_rule_ids, import_rule_ids,
            "same bytes → same rule set"
        );

        let result = commit_foreign_import(&pool, user_id, import_pkg, validated, policy_records)
            .await
            .unwrap();
        assert_eq!(result.source_sha256, preview_sha256);

        cleanup_import(&pool, result.bundle_id, &result.created_policy_version_ids).await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn digest_mismatch_produces_no_writes() {
        let pool = test_pool().await.expect("DATABASE_URL required");
        let bytes = minimal_xccdf_bytes();
        let pkg = make_package(bytes);
        let artifact_count_before: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM compliance_source_artifacts WHERE sha256 = $1",
        )
        .bind(&pkg.provenance.sha256)
        .fetch_one(&pool)
        .await
        .unwrap();

        // Use the wrong digest — this is caught at the handler layer before
        // calling commit_foreign_import, but we validate the guard explicitly here.
        let wrong_digest = "b".repeat(64);
        assert_ne!(pkg.provenance.sha256, wrong_digest);

        use crate::compliance::xccdf::importer::validate_sha256_match;
        let err = validate_sha256_match(&wrong_digest, &pkg.provenance.sha256);
        assert!(err.is_some(), "mismatch must be detected");
        assert_eq!(err.unwrap().code, "SOURCE_DIGEST_MISMATCH");

        // Confirm no artifact rows were inserted.
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM compliance_source_artifacts WHERE sha256 = $1",
        )
        .bind(&pkg.provenance.sha256)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, artifact_count_before);
    }

    /// Remove test rows created by a successful import to keep the test DB clean.
    async fn cleanup_import(pool: &PgPool, bundle_id: Uuid, policy_version_ids: &[Uuid]) {
        // Delete policy versions first (FK from membership), then lineages.
        for pvid in policy_version_ids {
            let _ = sqlx::query("DELETE FROM deployment_policy_versions WHERE id = $1")
                .bind(pvid)
                .execute(pool)
                .await;
            let policy_id: Option<Uuid> = sqlx::query_scalar(
                "SELECT policy_id FROM deployment_policy_versions WHERE id = $1",
            )
            .bind(pvid)
            .fetch_optional(pool)
            .await
            .unwrap_or(None);
            if let Some(pid) = policy_id {
                let _ = sqlx::query("DELETE FROM deployment_policies WHERE id = $1")
                    .bind(pid)
                    .execute(pool)
                    .await;
            }
        }
        // Cascade deletes bundle_version_policies, mappings, audit rows.
        let _ = sqlx::query("DELETE FROM compliance_bundles WHERE id = $1")
            .bind(bundle_id)
            .execute(pool)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn cf_native_sequential_idempotent_reimport_succeeds() {
        let pool = test_pool().await.expect("DATABASE_URL required");
        let user_id = ensure_test_user(&pool).await;
        let (bytes, bundle_id, bundle_version_id, policy_id, policy_version_id) =
            native_fixture_bytes();

        let pkg = make_package(bytes.clone());
        let (validated, records) =
            crate::compliance::xccdf::importer::validate_cf_native_document(&pkg.parsed)
                .expect("native fixture should validate");

        let first = commit_cf_native_import(&pool, user_id, pkg, validated, records)
            .await
            .expect("first sequential import");
        assert_eq!(first.created_policy_versions, 1);
        assert_eq!(first.reused_policy_versions, 0);

        let second_pkg = make_package(bytes);
        let (second_validated, second_records) =
            crate::compliance::xccdf::importer::validate_cf_native_document(&second_pkg.parsed)
                .expect("native fixture should validate on repeat");

        let second =
            commit_cf_native_import(&pool, user_id, second_pkg, second_validated, second_records)
                .await
                .expect("second sequential import");
        assert_eq!(
            second.created_policy_versions, 0,
            "should not create duplicate versions"
        );
        assert_eq!(
            second.reused_policy_versions, 1,
            "should reuse exact version"
        );
        assert_eq!(second.bundle_id, bundle_id, "bundle id must match");
        assert_eq!(
            second.bundle_version_id, bundle_version_id,
            "bundle version id must match"
        );

        let bundle_versions: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM compliance_bundle_versions WHERE bundle_id = $1",
        )
        .bind(bundle_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            bundle_versions, 1,
            "should not create duplicate bundle versions"
        );

        let policy_versions: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM deployment_policy_versions WHERE policy_id = $1",
        )
        .bind(policy_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            policy_versions, 1,
            "should not create duplicate policy versions"
        );
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn cf_native_preview_exact_match_after_commit() {
        use crate::handlers::api::compliance::compute_cf_native_reconciliation;

        let pool = test_pool().await.expect("DATABASE_URL required");
        let user_id = ensure_test_user(&pool).await;
        let (bytes, _bundle_id, bundle_version_id, _policy_id, policy_version_id) =
            native_fixture_bytes();

        let pkg = make_package(bytes.clone());
        let (validated, records) =
            crate::compliance::xccdf::importer::validate_cf_native_document(&pkg.parsed)
                .expect("native fixture should validate");
        commit_cf_native_import(&pool, user_id, pkg, validated, records)
            .await
            .expect("native import should succeed");

        let preview_pkg = make_package(bytes);
        let preview = compute_cf_native_reconciliation(&pool, &preview_pkg.parsed)
            .await
            .expect("reconciliation should succeed")
            .expect("cf-native document should produce a preview");

        assert_eq!(preview.bundle.reconciliation_state, "exact_match");
        assert_eq!(
            preview.bundle.local_version_id.as_deref(),
            Some(bundle_version_id.to_string().as_str())
        );
        assert!(
            !preview.has_blocking_conflicts,
            "exact match must not report conflicts"
        );
        assert_eq!(preview.policies.len(), 1);
        assert_eq!(preview.policies[0].reconciliation_state, "exact_match");
        assert_eq!(
            preview.policies[0].local_version_id.as_deref(),
            Some(policy_version_id.to_string().as_str())
        );
        assert!(preview.policies[0].blocking_conflicts.is_empty());
        assert_eq!(preview.signature_status, "not_supported");
        assert_eq!(preview.import_trust_state, "untrusted");
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn cf_native_preview_new_version_same_lineage() {
        use crate::handlers::api::compliance::compute_cf_native_reconciliation;

        let pool = test_pool().await.expect("DATABASE_URL required");
        let user_id = ensure_test_user(&pool).await;
        let (bytes, bundle_id, _bundle_version_id, policy_id, _policy_version_id) =
            native_fixture_bytes();

        let pkg = make_package(bytes.clone());
        let (validated, records) =
            crate::compliance::xccdf::importer::validate_cf_native_document(&pkg.parsed)
                .expect("native fixture should validate");
        commit_cf_native_import(&pool, user_id, pkg, validated, records)
            .await
            .expect("native import should succeed");

        // Same lineages, brand new version IDs.
        let new_bundle_version_id = Uuid::new_v4();
        let new_policy_version_id = Uuid::new_v4();
        let new_bytes = native_fixture_bytes_with(
            bundle_id,
            new_bundle_version_id,
            policy_id,
            new_policy_version_id,
            true,
        );
        let preview_pkg = make_package(new_bytes);
        let preview = compute_cf_native_reconciliation(&pool, &preview_pkg.parsed)
            .await
            .expect("reconciliation should succeed")
            .expect("cf-native document should produce a preview");

        assert_eq!(preview.bundle.reconciliation_state, "new_version");
        assert_eq!(preview.policies[0].reconciliation_state, "new_version");
        assert_eq!(
            preview.policies[0].local_lineage_id.as_deref(),
            Some(policy_id.to_string().as_str())
        );
        assert_eq!(preview.policies[0].local_version_id.as_deref(), None);
        assert!(
            !preview.has_blocking_conflicts,
            "new version of an existing lineage must not block"
        );
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn cf_native_preview_reports_blocking_digest_conflict() {
        use crate::handlers::api::compliance::compute_cf_native_reconciliation;

        let pool = test_pool().await.expect("DATABASE_URL required");
        let user_id = ensure_test_user(&pool).await;
        let (bytes, bundle_id, bundle_version_id, policy_id, policy_version_id) =
            native_fixture_bytes();

        let pkg = make_package(bytes.clone());
        let (validated, records) =
            crate::compliance::xccdf::importer::validate_cf_native_document(&pkg.parsed)
                .expect("native fixture should validate");
        commit_cf_native_import(&pool, user_id, pkg, validated, records)
            .await
            .expect("native import should succeed");

        // Same identities, different content => semantic digest conflict.
        let conflicting = native_fixture_bytes_with(
            bundle_id,
            bundle_version_id,
            policy_id,
            policy_version_id,
            false,
        );
        let preview_pkg = make_package(conflicting);
        let preview = compute_cf_native_reconciliation(&pool, &preview_pkg.parsed)
            .await
            .expect("reconciliation should succeed")
            .expect("cf-native document should produce a preview");

        assert!(
            preview.has_blocking_conflicts,
            "digest conflict must be reported as blocking"
        );
        assert_eq!(
            preview.policies[0].reconciliation_state,
            "identity_conflict"
        );
        assert!(
            !preview.policies[0].blocking_conflicts.is_empty(),
            "policy must carry its blocking conflict"
        );
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn cf_native_preview_is_mutation_free() {
        use crate::handlers::api::compliance::compute_cf_native_reconciliation;

        let pool = test_pool().await.expect("DATABASE_URL required");
        let _user_id = ensure_test_user(&pool).await;
        let (bytes, bundle_id, _bundle_version_id, policy_id, _policy_version_id) =
            native_fixture_bytes();

        let bundle_count_before: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM compliance_bundles WHERE id = $1")
                .bind(bundle_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let policy_count_before: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM deployment_policies WHERE id = $1")
                .bind(policy_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(bundle_count_before, 0);
        assert_eq!(policy_count_before, 0);

        let preview_pkg = make_package(bytes);
        let preview = compute_cf_native_reconciliation(&pool, &preview_pkg.parsed)
            .await
            .expect("reconciliation should succeed")
            .expect("cf-native document should produce a preview");

        assert_eq!(preview.bundle.reconciliation_state, "new_lineage");
        assert_eq!(preview.policies[0].reconciliation_state, "new_lineage");

        let bundle_count_after: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM compliance_bundles WHERE id = $1")
                .bind(bundle_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let policy_count_after: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM deployment_policies WHERE id = $1")
                .bind(policy_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            bundle_count_before, bundle_count_after,
            "preview must not insert bundles"
        );
        assert_eq!(
            policy_count_before, policy_count_after,
            "preview must not insert policies"
        );
    }

    /// Publish a CF-native fixture lineage through the full trigger-safe
    /// lifecycle, mirroring the handler order:
    ///
    /// 1. Trust the policy version while it is still draft.
    /// 2. Publish the policy version: clear draft pointer, accept (DEFERRED
    ///    trigger queued), then set the published pointer.
    /// 3. Trust the bundle version while it is still draft.
    /// 4. Publish the bundle version with the same trigger-safe order.
    ///
    /// Accepted versions cannot be deleted (immutability triggers), so tests
    /// that publish intentionally leave their fixture rows behind, matching
    /// the compliance.rs suite pattern of unique UUIDs per run.
    async fn publish_native_lifecycle(
        pool: &PgPool,
        actor_id: Uuid,
        policy_id: Uuid,
        policy_version_id: Uuid,
        bundle_id: Uuid,
        bundle_version_id: Uuid,
    ) {
        let mut tx = pool.begin().await.expect("begin publish tx");

        sqlx::query(
            "UPDATE deployment_policy_versions \
             SET trust_state = 'trusted', trusted_by = $2, trusted_at = CURRENT_TIMESTAMP \
             WHERE id = $1",
        )
        .bind(policy_version_id)
        .bind(actor_id)
        .execute(&mut *tx)
        .await
        .expect("trust policy version");

        sqlx::query(
            "UPDATE deployment_policies SET current_draft_version_id = NULL \
             WHERE id = $1 AND current_draft_version_id = $2",
        )
        .bind(policy_id)
        .bind(policy_version_id)
        .execute(&mut *tx)
        .await
        .expect("clear policy draft pointer");
        sqlx::query(
            "UPDATE deployment_policy_versions \
             SET publication_state = 'accepted', published_at = CURRENT_TIMESTAMP \
             WHERE id = $1",
        )
        .bind(policy_version_id)
        .execute(&mut *tx)
        .await
        .expect("accept policy version");
        sqlx::query(
            "UPDATE deployment_policies SET current_published_version_id = $1 WHERE id = $2",
        )
        .bind(policy_version_id)
        .bind(policy_id)
        .execute(&mut *tx)
        .await
        .expect("set policy published pointer");

        sqlx::query(
            "UPDATE compliance_bundle_versions \
             SET trust_state = 'trusted', trusted_by = $2, trusted_at = CURRENT_TIMESTAMP \
             WHERE id = $1",
        )
        .bind(bundle_version_id)
        .bind(actor_id)
        .execute(&mut *tx)
        .await
        .expect("trust bundle version");

        sqlx::query(
            "UPDATE compliance_bundles SET current_draft_version_id = NULL \
             WHERE id = $1 AND current_draft_version_id = $2",
        )
        .bind(bundle_id)
        .bind(bundle_version_id)
        .execute(&mut *tx)
        .await
        .expect("clear bundle draft pointer");
        sqlx::query(
            "UPDATE compliance_bundle_versions \
             SET publication_state = 'accepted', published_at = CURRENT_TIMESTAMP \
             WHERE id = $1",
        )
        .bind(bundle_version_id)
        .execute(&mut *tx)
        .await
        .expect("accept bundle version");
        sqlx::query(
            "UPDATE compliance_bundles SET current_published_version_id = $1 WHERE id = $2",
        )
        .bind(bundle_version_id)
        .bind(bundle_id)
        .execute(&mut *tx)
        .await
        .expect("set bundle published pointer");

        tx.commit().await.expect("commit publish lifecycle");
    }

    /// Proof A: after a full publish lifecycle (trust while draft, then
    /// publish policy before bundle in trigger-safe order), an exact re-import
    /// of the same bytes must reuse the exact published identities. It must
    /// not trip the bundle-membership immutability trigger, must not create or
    /// rewrite rows, must leave publication/trust state and pointers
    /// untouched, and must keep source mappings on the same exact versions.
    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn cf_native_reimport_exact_after_publish_lifecycle() {
        let pool = test_pool().await.expect("DATABASE_URL required");
        let user_id = ensure_test_user(&pool).await;
        let (bytes, bundle_id, bundle_version_id, policy_id, policy_version_id) =
            native_fixture_bytes();

        let pkg = make_package(bytes.clone());
        let (validated, records) =
            crate::compliance::xccdf::importer::validate_cf_native_document(&pkg.parsed)
                .expect("native fixture should validate");
        let first = commit_cf_native_import(&pool, user_id, pkg, validated, records)
            .await
            .expect("first native import");
        assert_eq!(first.created_policy_versions, 1);
        assert!(first.bundle_version_created);

        publish_native_lifecycle(
            &pool,
            user_id,
            policy_id,
            policy_version_id,
            bundle_id,
            bundle_version_id,
        )
        .await;

        // Confirm the published/trusted state before re-import.
        let (pol_state, pol_trust): (String, String) = sqlx::query_as(
            "SELECT publication_state, trust_state FROM deployment_policy_versions WHERE id = $1",
        )
        .bind(policy_version_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            (pol_state.as_str(), pol_trust.as_str()),
            ("accepted", "trusted")
        );
        let (bun_state, bun_trust): (String, String) = sqlx::query_as(
            "SELECT publication_state, trust_state FROM compliance_bundle_versions WHERE id = $1",
        )
        .bind(bundle_version_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            (bun_state.as_str(), bun_trust.as_str()),
            ("accepted", "trusted")
        );

        // Exact re-import of the same bytes must reuse, not error.
        let second_pkg = make_package(bytes);
        let (second_validated, second_records) =
            crate::compliance::xccdf::importer::validate_cf_native_document(&second_pkg.parsed)
                .expect("native fixture should validate on repeat");
        let second =
            commit_cf_native_import(&pool, user_id, second_pkg, second_validated, second_records)
                .await
                .expect("exact re-import after publish must reuse, not fail");
        assert_eq!(second.created_policy_lineages, 0);
        assert_eq!(second.created_policy_versions, 0);
        assert_eq!(second.reused_policy_versions, 1);
        assert!(!second.bundle_lineage_created);
        assert!(!second.bundle_version_created);
        assert_eq!(second.bundle_id, bundle_id);
        assert_eq!(second.bundle_version_id, bundle_version_id);
        assert_eq!(
            second.source_artifact_id, first.source_artifact_id,
            "sha256 dedupe must reuse the original artifact row"
        );

        // Lifecycle state must be unchanged by the re-import.
        let (pol_state, pol_trust): (String, String) = sqlx::query_as(
            "SELECT publication_state, trust_state FROM deployment_policy_versions WHERE id = $1",
        )
        .bind(policy_version_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            (pol_state.as_str(), pol_trust.as_str()),
            ("accepted", "trusted")
        );
        let (bun_state, bun_trust): (String, String) = sqlx::query_as(
            "SELECT publication_state, trust_state FROM compliance_bundle_versions WHERE id = $1",
        )
        .bind(bundle_version_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            (bun_state.as_str(), bun_trust.as_str()),
            ("accepted", "trusted")
        );

        // Pointer invariants survive the re-import.
        let (pol_draft, pol_pub): (Option<Uuid>, Option<Uuid>) = sqlx::query_as(
            "SELECT current_draft_version_id, current_published_version_id \
             FROM deployment_policies WHERE id = $1",
        )
        .bind(policy_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(pol_draft, None);
        assert_eq!(pol_pub, Some(policy_version_id));
        let (bun_draft, bun_pub): (Option<Uuid>, Option<Uuid>) = sqlx::query_as(
            "SELECT current_draft_version_id, current_published_version_id \
             FROM compliance_bundles WHERE id = $1",
        )
        .bind(bundle_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(bun_draft, None);
        assert_eq!(bun_pub, Some(bundle_version_id));

        // Membership must be unchanged: exactly one ordered row.
        let members: Vec<(Uuid, bool, i32)> = sqlx::query_as(
            "SELECT policy_version_id, selected, policy_order \
             FROM compliance_bundle_version_policies WHERE bundle_version_id = $1 \
             ORDER BY policy_order ASC",
        )
        .bind(bundle_version_id)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].0, policy_version_id);
        assert!(members[0].1);
        assert_eq!(members[0].2, 0);

        // No duplicate rows anywhere.
        let policy_versions: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM deployment_policy_versions WHERE policy_id = $1",
        )
        .bind(policy_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(policy_versions, 1);
        let bundle_versions: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM compliance_bundle_versions WHERE bundle_id = $1",
        )
        .bind(bundle_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(bundle_versions, 1);

        // Source mappings must still point at the exact reused versions.
        let (mapped_policy, mapped_bundle): (Option<Uuid>, Option<Uuid>) = sqlx::query_as(
            "SELECT policy_version_id, bundle_version_id FROM compliance_source_object_mappings \
             WHERE source_artifact_id = $1 AND object_kind = 'rule' AND source_identity = $2",
        )
        .bind(second.source_artifact_id)
        .bind(format!("xccdf_test_rule_{policy_version_id}"))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(mapped_policy, Some(policy_version_id));
        assert_eq!(mapped_bundle, None);
        let benchmark_mapped: Option<Uuid> = sqlx::query_scalar(
            "SELECT bundle_version_id FROM compliance_source_object_mappings \
             WHERE source_artifact_id = $1 AND object_kind = 'benchmark'",
        )
        .bind(second.source_artifact_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(benchmark_mapped, Some(bundle_version_id));
    }

    /// Proof B: committing a new version into an existing PUBLISHED lineage
    /// must plan as `new_version` in the preview, create draft version B rows
    /// in the same lineages, leave the accepted version A rows untouched, and
    /// keep the draft/published pointers satisfying the deferred-trigger
    /// invariants (draft points at B, published points at A).
    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn cf_native_commit_new_version_in_existing_lineage() {
        use crate::handlers::api::compliance::compute_cf_native_reconciliation;

        let pool = test_pool().await.expect("DATABASE_URL required");
        let user_id = ensure_test_user(&pool).await;
        let (bytes_a, bundle_id, bundle_version_id, policy_id, policy_version_id) =
            native_fixture_bytes();

        // Import and publish version A, leaving no mutable draft behind.
        let pkg = make_package(bytes_a.clone());
        let (validated, records) =
            crate::compliance::xccdf::importer::validate_cf_native_document(&pkg.parsed)
                .expect("native fixture should validate");
        let first = commit_cf_native_import(&pool, user_id, pkg, validated, records)
            .await
            .expect("first native import");
        assert_eq!(first.created_policy_versions, 1);
        publish_native_lifecycle(
            &pool,
            user_id,
            policy_id,
            policy_version_id,
            bundle_id,
            bundle_version_id,
        )
        .await;

        // Build version B: same lineages, fresh version UUIDs, same content.
        // A distinct <cf:policy-version> label is required because policy
        // lineages enforce UNIQUE (policy_id, version).
        let new_bundle_version_id = Uuid::new_v4();
        let new_policy_version_id = Uuid::new_v4();
        let bytes_b = native_fixture_bytes_with_policy_version(
            bundle_id,
            new_bundle_version_id,
            policy_id,
            new_policy_version_id,
            true,
            "1.1",
        );

        // Preview B: new_version for both, lineage witnesses, no blocking.
        let preview_pkg = make_package(bytes_b.clone());
        let preview = compute_cf_native_reconciliation(&pool, &preview_pkg.parsed)
            .await
            .expect("reconciliation should succeed")
            .expect("cf-native document should produce a preview");
        assert_eq!(preview.bundle.reconciliation_state, "new_version");
        assert_eq!(
            preview.bundle.local_lineage_id.as_deref(),
            Some(bundle_id.to_string().as_str())
        );
        assert_eq!(
            preview.bundle.local_version_id.as_deref(),
            Some(bundle_version_id.to_string().as_str())
        );
        assert_eq!(preview.policies[0].reconciliation_state, "new_version");
        assert_eq!(
            preview.policies[0].local_lineage_id.as_deref(),
            Some(policy_id.to_string().as_str())
        );
        assert_eq!(preview.policies[0].local_version_id.as_deref(), None);
        assert!(
            !preview.has_blocking_conflicts,
            "new version of a published lineage must not block"
        );

        // Commit B.
        let commit_pkg = make_package(bytes_b);
        let (commit_validated, commit_records) =
            crate::compliance::xccdf::importer::validate_cf_native_document(&commit_pkg.parsed)
                .expect("version B should validate");
        let second =
            commit_cf_native_import(&pool, user_id, commit_pkg, commit_validated, commit_records)
                .await
                .expect("commit version B into existing published lineage");
        assert_eq!(second.created_policy_lineages, 0);
        assert_eq!(second.created_policy_versions, 1);
        assert_eq!(second.reused_policy_versions, 0);
        assert!(!second.bundle_lineage_created);
        assert!(second.bundle_version_created);
        assert_eq!(second.bundle_id, bundle_id);
        assert_eq!(second.bundle_version_id, new_bundle_version_id);
        assert_eq!(
            second.created_policy_version_ids,
            vec![new_policy_version_id]
        );

        // Version A rows must be unchanged (still accepted + trusted).
        let (a_state, a_trust): (String, String) = sqlx::query_as(
            "SELECT publication_state, trust_state FROM deployment_policy_versions WHERE id = $1",
        )
        .bind(policy_version_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            (a_state.as_str(), a_trust.as_str()),
            ("accepted", "trusted")
        );
        let (ab_state, ab_trust): (String, String) = sqlx::query_as(
            "SELECT publication_state, trust_state FROM compliance_bundle_versions WHERE id = $1",
        )
        .bind(bundle_version_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            (ab_state.as_str(), ab_trust.as_str()),
            ("accepted", "trusted")
        );

        // Version B rows exist in the same lineages as fresh drafts.
        let (b_state, b_trust): (String, String) = sqlx::query_as(
            "SELECT publication_state, trust_state FROM deployment_policy_versions WHERE id = $1",
        )
        .bind(new_policy_version_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!((b_state.as_str(), b_trust.as_str()), ("draft", "untrusted"));
        let (bb_state, bb_trust): (String, String) = sqlx::query_as(
            "SELECT publication_state, trust_state FROM compliance_bundle_versions WHERE id = $1",
        )
        .bind(new_bundle_version_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            (bb_state.as_str(), bb_trust.as_str()),
            ("draft", "untrusted")
        );

        // The bundle version label must be lineage-unique: version B gets a
        // deterministic disambiguated label instead of colliding with A's.
        let bb_label: String =
            sqlx::query_scalar("SELECT version FROM compliance_bundle_versions WHERE id = $1")
                .bind(new_bundle_version_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let short_b = &new_bundle_version_id.simple().to_string()[..8];
        assert_eq!(bb_label, format!("0.1.0-{short_b}"));
        let a_label: String =
            sqlx::query_scalar("SELECT version FROM compliance_bundle_versions WHERE id = $1")
                .bind(bundle_version_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(a_label, "0.1.0");

        // Pointer invariants: draft now points at B, published still at A.
        let (pol_draft, pol_pub): (Option<Uuid>, Option<Uuid>) = sqlx::query_as(
            "SELECT current_draft_version_id, current_published_version_id \
             FROM deployment_policies WHERE id = $1",
        )
        .bind(policy_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(pol_draft, Some(new_policy_version_id));
        assert_eq!(pol_pub, Some(policy_version_id));
        let (bun_draft, bun_pub): (Option<Uuid>, Option<Uuid>) = sqlx::query_as(
            "SELECT current_draft_version_id, current_published_version_id \
             FROM compliance_bundles WHERE id = $1",
        )
        .bind(bundle_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(bun_draft, Some(new_bundle_version_id));
        assert_eq!(bun_pub, Some(bundle_version_id));

        // Membership for B is exactly (B policy version, selected) ordered.
        let members_b: Vec<(Uuid, bool, i32)> = sqlx::query_as(
            "SELECT policy_version_id, selected, policy_order \
             FROM compliance_bundle_version_policies WHERE bundle_version_id = $1 \
             ORDER BY policy_order ASC",
        )
        .bind(new_bundle_version_id)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(members_b.len(), 1);
        assert_eq!(members_b[0].0, new_policy_version_id);
        assert!(members_b[0].1);
        assert_eq!(members_b[0].2, 0);

        // Membership of A is untouched.
        let members_a: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM compliance_bundle_version_policies WHERE bundle_version_id = $1",
        )
        .bind(bundle_version_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(members_a, 1);

        // Source mappings point at the exact versions created by this commit.
        let mapped_policy: Option<Uuid> = sqlx::query_scalar(
            "SELECT policy_version_id FROM compliance_source_object_mappings \
             WHERE source_artifact_id = $1 AND object_kind = 'rule'",
        )
        .bind(second.source_artifact_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(mapped_policy, Some(new_policy_version_id));
        let mapped_bundle: Option<Uuid> = sqlx::query_scalar(
            "SELECT bundle_version_id FROM compliance_source_object_mappings \
             WHERE source_artifact_id = $1 AND object_kind = 'benchmark'",
        )
        .bind(second.source_artifact_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(mapped_bundle, Some(new_bundle_version_id));
    }

    /// Single-rule DISA STIG whose fix text carries explicit NixOS assignments,
    /// giving the exact-technical-match commit revalidation something
    /// authoritative to re-derive the technical identity from.
    fn stig_bytes_with_fix(benchmark_id: &str, vuln_id: &str, title: &str, fix_text: &str) -> Vec<u8> {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<Benchmark xmlns="http://checklists.nist.gov/xccdf/1.2" id="{benchmark_id}">
  <status>draft</status><title>Exact Match STIG</title><version>V1R1</version>
  <Rule id="xccdf_test_stig_rule_0"><title>{title}</title><description>Stable requirement {vuln_id}</description><ident system="http://cyber.mil/stigs/stig">{vuln_id}</ident><fix>{fix_text}</fix><check system="urn:test"><check-content>Verify {vuln_id}.</check-content></check></Rule>
</Benchmark>"#
        )
        .into_bytes()
    }

    /// Plan mapping every selected rule to one existing policy version with an
    /// explicit proof and per-requirement reviewed mapping semantics.
    fn exact_match_plan(
        pkg: &ProcessedXccdfPackage,
        policy_version_id: Uuid,
        proof: Option<MapExistingProof>,
        semantics: Option<crate::compliance::xccdf::import_models::ImportedMappingSemantics>,
    ) -> (ValidatedImportPlan, Vec<ImportedPolicyRecord>) {
        use crate::compliance::xccdf::import_models::{
            ImportedBundlePlan, XccdfImportPlan, XccdfRuleImportAction,
        };
        use crate::compliance::xccdf::importer::validate_import_plan;

        let rule_ids: Vec<String> = pkg
            .parsed
            .rules
            .iter()
            .map(|rule| rule.id.clone())
            .collect();
        let plan = XccdfImportPlan {
            expected_sha256: pkg.provenance.sha256.clone(),
            selected_profile_id: None,
            selected_rule_ids: rule_ids.clone(),
            rule_actions: rule_ids
                .iter()
                .map(|rule_id| XccdfRuleImportAction::MapExisting {
                    rule_id: rule_id.clone(),
                    policy_version_id,
                    proof,
                })
                .collect(),
            mapping_semantics: semantics
                .map(|s| {
                    rule_ids
                        .iter()
                        .map(|id| (id.clone(), s.clone()))
                        .collect()
                })
                .unwrap_or_default(),
            shared_group_decisions: Vec::new(),
            bundle: ImportedBundlePlan {
                name: format!("ExactMatch STIG bundle {}", Uuid::new_v4()),
                framework: "DISA STIG".into(),
                version: "test".into(),
                layer: Some("os".into()),
                owner: Some("Security Team".into()),
                description: None,
            },
        };
        let validated = validate_import_plan(plan, &pkg.parsed).expect("valid exact match plan");
        let records = build_policy_records(&validated);
        (validated, records)
    }

    /// Create an accepted, currently-published policy whose flat config
    /// implements the given NixOS option assignments.  This simulates any
    /// policy established by an earlier import from a different framework.
    /// Policy names are randomized to avoid unique constraint violations across test runs.
    async fn insert_published_technical_policy(
        pool: &PgPool,
        policy_name_base: &str,
        config: serde_json::Value,
    ) -> (Uuid, Uuid) {
        // Note: this must be called from an async context and the caller should pass user_id
        // For now, use a placeholder that violates the FK but can be fixed by the test helper
        let trusted_user_id = ensure_test_user(pool).await;
        let policy_id = Uuid::new_v4();
        let policy_version_id = Uuid::new_v4();
        let policy_name = format!("{}-{}", policy_name_base, Uuid::new_v4().simple());
        sqlx::query(
            "INSERT INTO deployment_policies (id, name, description, policy_type) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(policy_id)
        .bind(&policy_name)
        .bind(format!("{policy_name} technical enforcement"))
        .bind("native")
        .execute(pool)
        .await
        .unwrap();
        // Insert version in draft state (required by trigger guard_version_insert_state)
        sqlx::query(
            "INSERT INTO deployment_policy_versions \
             (id, policy_id, version, publication_state, name, policy_type, \
              implementation_state, execution_phase, config, compliance_metadata, \
              dependencies, semantic_digest, digest_algorithm) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
        )
        .bind(policy_version_id)
        .bind(policy_id)
        .bind("1.0")
        .bind("draft")
        .bind(&policy_name)
        .bind("native")
        .bind("native")
        .bind("deploy")
        .bind(&config)
        .bind(serde_json::json!({}))
        .bind(serde_json::json!([]))
        .bind("test-digest")
        .bind("sha-256")
        .execute(pool)
        .await
        .unwrap();
        
        // Publish the version so it becomes the current published version
         // First mark as trusted
         sqlx::query(
             "UPDATE deployment_policy_versions SET trust_state = 'trusted', trusted_by = $2, \
              trusted_at = CURRENT_TIMESTAMP WHERE id = $1",
         )
         .bind(policy_version_id)
         .bind(trusted_user_id)
         .execute(pool)
         .await
         .unwrap();
         
         // Then mark as accepted AND update pointer in same transaction
         // (trigger validate_policy_lineage_pointer_after_state_change requires this)
         let mut tx = pool.begin().await.unwrap();
         sqlx::query(
             "UPDATE deployment_policy_versions SET publication_state = 'accepted', \
              published_at = CURRENT_TIMESTAMP WHERE id = $1",
         )
         .bind(policy_version_id)
         .execute(&mut *tx)
         .await
         .unwrap();
         
         sqlx::query(
             "UPDATE deployment_policies SET current_published_version_id = $1 WHERE id = $2",
         )
         .bind(policy_version_id)
         .bind(policy_id)
         .execute(&mut *tx)
         .await
         .unwrap();
         
         tx.commit().await.unwrap();
        (policy_id, policy_version_id)
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn exact_technical_match_end_to_end_commit() {
        // Phase 21 end-to-end: a brand-new requirement with no prior mapping
        // whose exact technical match was found at preview, selected via
        // MapExisting + ExactTechnicalMatch proof, and committed through
        // commit_foreign_import.  The proof is revalidated from the parsed
        // rule's authoritative fix text inside the import transaction.
        let pool = test_pool().await.expect("DATABASE_URL required");
        let user_id = ensure_test_user(&pool).await;

        let ssh_config = serde_json::json!({
            "services.openssh.enable": false,
            "services.openssh.settings.PermitRootLogin": "no",
        });
        let (_existing_policy_id, existing_policy_version_id) =
            insert_published_technical_policy(&pool, "existing-ssh-hardening", ssh_config).await;

        let benchmark_id = format!(
            "xccdf_mil.disa.stig_benchmark_Exact_Match_End_To_End_{}",
            Uuid::new_v4().simple()
        );
        let fix_text = "services.openssh.enable = false;\nservices.openssh.settings.PermitRootLogin = \"no\";";
        let pkg = make_package(stig_bytes_with_fix(
            &benchmark_id,
            "V-418-101",
            "SSH root login must be disabled",
            fix_text,
        ));

        let semantics = crate::compliance::xccdf::import_models::ImportedMappingSemantics {
            relationship: Some("supports".into()),
            coverage: Some("partial".into()),
            rationale: Some("independent reviewed rationale".into()),
        };
        let (validated, records) = exact_match_plan(
            &pkg,
            existing_policy_version_id,
            Some(MapExistingProof::ExactTechnicalMatch),
            Some(semantics),
        );

        let result = commit_foreign_import(&pool, user_id, pkg, validated, records)
            .await
            .expect("exact technical match commit should succeed");

        // Nothing new was created; the reuse path derived a mutable draft.
        assert_eq!(result.created_policy_count, 0);
        assert_eq!(result.created_policy_lineages, 0);
        assert_eq!(result.created_policy_versions, 0);
        assert_eq!(result.reused_policy_versions, 1);

        // The bundle contains the derived draft (one member), not the accepted
        // published version, which must remain untouched.
        let members: Vec<Uuid> = sqlx::query_scalar(
            "SELECT policy_version_id FROM compliance_bundle_version_policies \
             WHERE bundle_version_id = $1",
        )
        .bind(result.bundle_version_id)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(members.len(), 1);
        let effective_draft = members[0];
        assert_ne!(effective_draft, existing_policy_version_id);

        let published_state: String = sqlx::query_scalar(
            "SELECT publication_state FROM deployment_policy_versions WHERE id = $1",
        )
        .bind(existing_policy_version_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(published_state, "accepted");

        // The derived draft carries the independent, per-requirement reviewed
        // mapping semantics with inferred provenance (technical candidate origin).
        let mapping: (String, String, Option<String>, String, String) = sqlx::query_as(
            "SELECT relationship, coverage, rationale, provenance, trust_state \
             FROM policy_requirement_mappings WHERE policy_version_id = $1",
        )
        .bind(effective_draft)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(mapping.0, "supports");
        assert_eq!(mapping.1, "partial");
        assert_eq!(mapping.2.as_deref(), Some("independent reviewed rationale"));
        assert_eq!(mapping.3, "inferred");
        assert_eq!(mapping.4, "trusted");

        // Source mapping points at the effective draft.
        let mapped_rule: Uuid = sqlx::query_scalar(
            "SELECT policy_version_id FROM compliance_source_object_mappings \
             WHERE source_artifact_id = $1 AND object_kind = 'rule'",
        )
        .bind(result.source_artifact_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(mapped_rule, effective_draft);

        // Requirement baseline membership exists for the one imported rule.
        let requirement_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM compliance_bundle_version_requirements \
             WHERE bundle_version_id = $1",
        )
        .bind(result.bundle_version_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(requirement_count, 1);
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn exact_technical_match_end_to_end_mismatch_rolls_back() {
        // A stale or mismatched enforcement must abort the whole import: the
        // policy config no longer implements what the authoritative fix text
        // demands, so nothing may be written (no partial import).
        let pool = test_pool().await.expect("DATABASE_URL required");
        let user_id = ensure_test_user(&pool).await;

        let ssh_config = serde_json::json!({
            "services.openssh.enable": false,
            "services.openssh.settings.PermitRootLogin": "no",
        });
        let (_existing_policy_id, existing_policy_version_id) =
            insert_published_technical_policy(&pool, "existing-ssh-hardening", ssh_config).await;

        let benchmark_id = format!(
            "xccdf_mil.disa.stig_benchmark_Exact_Match_Mismatch_{}",
            Uuid::new_v4().simple()
        );
        // Authoritative fix text now demands the opposite enforcement.
        let mismatched_fix_text =
            "services.openssh.enable = true;\nservices.openssh.settings.PermitRootLogin = \"yes\";";
        let pkg = make_package(stig_bytes_with_fix(
            &benchmark_id,
            "V-418-102",
            "SSH root login must be disabled",
            mismatched_fix_text,
        ));
        let (validated, records) = exact_match_plan(
            &pkg,
            existing_policy_version_id,
            Some(MapExistingProof::ExactTechnicalMatch),
            None,
        );
        let bundle_name = validated.bundle.name.clone();
        let source_sha256 = pkg.provenance.sha256.clone();

        let err = commit_foreign_import(&pool, user_id, pkg, validated, records)
            .await
            .expect_err("mismatched enforcement must be rejected");
        let message = format!("{err:#}");
        assert!(
            message.contains("IMPORT_REUSE_INELIGIBLE"),
            "unexpected error: {message}"
        );

        // The entire transaction rolled back: no bundle lineage, no source
        // artifact, and no mapping for the reused policy version.
        let bundle_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM compliance_bundles WHERE name = $1")
                .bind(&bundle_name)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(bundle_count, 0);
        let artifact_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM compliance_source_artifacts WHERE sha256 = $1",
        )
        .bind(&source_sha256)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(artifact_count, 0);
        let mapping_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM policy_requirement_mappings WHERE policy_version_id = $1",
        )
        .bind(existing_policy_version_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(mapping_count, 0);
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn exact_technical_match_end_to_end_missing_policy_rolls_back() {
        // Mapping onto a policy version that no longer exists must fail the
        // commit cleanly, leaving no bundle behind.
        let pool = test_pool().await.expect("DATABASE_URL required");
        let user_id = ensure_test_user(&pool).await;

        let benchmark_id = format!(
            "xccdf_mil.disa.stig_benchmark_Exact_Match_Missing_{}",
            Uuid::new_v4().simple()
        );
        let fix_text = "services.openssh.enable = false;";
        let pkg = make_package(stig_bytes_with_fix(
            &benchmark_id,
            "V-418-103",
            "SSH service must be disabled",
            fix_text,
        ));
        let (validated, records) = exact_match_plan(
            &pkg,
            Uuid::new_v4(),
            Some(MapExistingProof::ExactTechnicalMatch),
            None,
        );
        let bundle_name = validated.bundle.name.clone();

        let err = commit_foreign_import(&pool, user_id, pkg, validated, records)
            .await
            .expect_err("missing target policy must be rejected");
        let message = format!("{err:#}");
        assert!(
            message.contains("IMPORT_POLICY_VERSION_NOT_FOUND"),
            "unexpected error: {message}"
        );

        let bundle_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM compliance_bundles WHERE name = $1")
                .bind(&bundle_name)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(bundle_count, 0);
    }
}
