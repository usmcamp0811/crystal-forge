//! Reads the position of one system in the tracked branch history of its flake.
//!
//! The System Detail Deploy tab and the External/Reference overview draw this
//! position as a lane: HEAD, the running commit, the commits between them, and
//! the other hosts that run each commit. All reads in this module are
//! observational. They do not grant deployment, rollback, or policy authority.

use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::api::models::{
    FlakePositionBuildState, FlakePositionCommit, FlakePositionOrdering,
    FlakePositionUnknownReason, SystemFlakePositionResponse,
};
use crate::queries::systems::resolve_observational_current_revision;

/// Minimum number of branch commits returned, counted from HEAD.
///
/// This value matches the `/systems/:id/commits` window, so the Deploy tab
/// picker keeps the depth that it had before the lane existed.
pub const FLAKE_POSITION_MIN_COMMITS: i64 = 50;

/// Maximum number of branch commits returned, counted from HEAD.
///
/// PERFORMANCE: The bound keeps the response small for long-lived flakes. A
/// running commit deeper than this bound is reported as
/// [`FlakePositionUnknownReason::BeyondWindow`] instead of extending the list.
pub const FLAKE_POSITION_MAX_COMMITS: i64 = 500;

/// Number of older commits returned after the running commit.
///
/// The lane shows two commits below the running commit for context, as in the
/// design (`fpWindow` in `docs/design/CrystalForge/components/FlakePosition.jsx`).
pub const FLAKE_POSITION_TRAILING_COMMITS: i64 = 2;

#[derive(Debug, sqlx::FromRow)]
struct SystemFlakeRow {
    flake_id: Option<i32>,
    flake_name: Option<String>,
    branch: Option<String>,
    snapshot_ready: bool,
    hostname: String,
    configuration_name: String,
    has_observation: bool,
}

#[derive(Debug, sqlx::FromRow)]
struct PositionCommitRow {
    position: i64,
    deployed_here: bool,
    sha: String,
    message: Option<String>,
    author: Option<String>,
    committed_at: DateTime<Utc>,
    build_state: String,
    peers: Vec<String>,
}

/// Ordered, non-archived commits of one flake with a dense `position`.
///
/// INVARIANT: `position` is a dense 0-based index (0 = HEAD) over the returned
/// order, so `position` differences equal the number of lane steps. When the
/// flake has a ready branch snapshot, only snapshot members are included and
/// they keep snapshot order. Otherwise the order falls back to
/// `commit_timestamp DESC, id DESC`, the same fallback as the dashboard
/// timeline (`fetch_dashboard_flake_timelines`).
const ORDERED_COMMITS_CTE: &str = r#"
    ordered AS (
      SELECT c.id, c.git_commit_hash, c.message, c.author, c.commit_timestamp,
             ROW_NUMBER() OVER (ORDER BY
               CASE WHEN $2 THEN fbcs.position END ASC,
               c.commit_timestamp DESC, c.id DESC)::bigint - 1 AS position
      FROM commits c
      LEFT JOIN flake_branch_commit_snapshot fbcs
        ON fbcs.flake_id = c.flake_id AND fbcs.commit_id = c.id
      WHERE c.flake_id = $1
        AND c.source_archived = false
        AND (NOT $2 OR fbcs.commit_id IS NOT NULL)
    )
"#;

/// Classifies why a running position is unavailable.
///
/// Returns `None` when `running_position` is known. The checks run in order:
/// no commits for the flake, no running observation, an observation that maps
/// to no tracked commit, a commit outside the tracked branch, and a commit
/// deeper than [`FLAKE_POSITION_MAX_COMMITS`].
pub fn classify_flake_position(
    has_history: bool,
    has_observation: bool,
    running_commit: Option<&str>,
    running_position: Option<i64>,
) -> Option<FlakePositionUnknownReason> {
    if !has_history {
        return Some(FlakePositionUnknownReason::NoHistory);
    }
    if !has_observation {
        return Some(FlakePositionUnknownReason::NotObserved);
    }
    let Some(_) = running_commit else {
        return Some(FlakePositionUnknownReason::Untracked);
    };
    match running_position {
        None => Some(FlakePositionUnknownReason::OffBranch),
        Some(position) if position >= FLAKE_POSITION_MAX_COMMITS => {
            Some(FlakePositionUnknownReason::BeyondWindow)
        }
        Some(_) => None,
    }
}

/// Returns the number of commits to read for a running position.
///
/// The result covers HEAD through the running commit plus
/// [`FLAKE_POSITION_TRAILING_COMMITS`], and stays within
/// [`FLAKE_POSITION_MIN_COMMITS`] and [`FLAKE_POSITION_MAX_COMMITS`].
pub fn flake_position_limit(running_position: Option<i64>) -> i64 {
    running_position
        .map(|position| position + FLAKE_POSITION_TRAILING_COMMITS + 1)
        .unwrap_or(0)
        .clamp(FLAKE_POSITION_MIN_COMMITS, FLAKE_POSITION_MAX_COMMITS)
}

fn parse_build_state(value: &str) -> FlakePositionBuildState {
    match value {
        "building" => FlakePositionBuildState::Building,
        "ready" => FlakePositionBuildState::Ready,
        "unpublished" => FlakePositionBuildState::Unpublished,
        "failed" => FlakePositionBuildState::Failed,
        _ => FlakePositionBuildState::NotBuilt,
    }
}

/// Reads the flake position of one system.
///
/// `peer_visibility_user` limits peer hostnames to systems in environments
/// where that user is a member. Pass `None` only for callers that can access
/// every environment (administrators). The caller MUST authorize access to
/// `system_id` before it calls this function.
///
/// Returns `Ok(None)` when the system does not exist.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot read the system, its flake
/// commits, its builds, or its peers.
pub async fn fetch_system_flake_position(
    pool: &PgPool,
    system_id: Uuid,
    peer_visibility_user: Option<Uuid>,
) -> Result<Option<SystemFlakePositionResponse>> {
    let Some(system) = sqlx::query_as::<_, SystemFlakeRow>(
        r#"
        SELECT s.hostname, f.id AS flake_id, f.name AS flake_name,
               COALESCE(f.branch, 'main') AS branch,
               COALESCE(f.snapshot_ready_at IS NOT NULL, false) AS snapshot_ready,
               COALESCE(NULLIF(BTRIM(s.system_configuration_name), ''), s.hostname)
                 AS configuration_name,
               -- INVARIANT: Observation presence comes from the LATEST eligible
               -- row, the same row the running-commit lookups use. The view
               -- holds every eligible observation, so an older row with a store
               -- path must not make a host whose latest report has none look
               -- observed (`untracked` instead of `not_observed`).
               COALESCE((
                 SELECT NULLIF(BTRIM(state.store_path), '') IS NOT NULL
                 FROM view_agent_current_system_states state
                 WHERE state.hostname = s.hostname
                 ORDER BY state.timestamp DESC, state.id DESC
                 LIMIT 1
               ), false) AS has_observation
        FROM systems s
        LEFT JOIN flakes f ON f.id = s.flake_id AND f.deleted_at IS NULL
        WHERE s.id = $1
        "#,
    )
    .bind(system_id)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };

    let ordering = if system.snapshot_ready {
        FlakePositionOrdering::BranchSnapshot
    } else {
        FlakePositionOrdering::CommitTimestamp
    };
    let Some(flake_id) = system.flake_id else {
        return Ok(Some(SystemFlakePositionResponse {
            flake_id: None,
            flake_name: None,
            branch: None,
            ordering,
            commits: Vec::new(),
            running_commit: None,
            running_position: None,
            unknown_reason: Some(FlakePositionUnknownReason::NoHistory),
            last_tracked_position: None,
        }));
    };

    // INVARIANT: The lane places the host at the newest branch commit whose
    // `nixos` output for this configuration equals the running store path.
    // Commits that do not change the configuration produce the same output, so
    // one store path can map to many commits. The newest one answers "how far
    // behind is this host", and it is the commit that
    // `view_system_deployment_status.current_commit_hash` (Systems list,
    // Overview, peers) reports. The observational resolver can name an older
    // commit with the same output (for example, the commit of the last CF
    // deployment), so it is the fallback only when no branch output matches.
    let closure_match = sqlx::query_as::<_, (i64, String)>(&format!(
        r#"
        WITH {ORDERED_COMMITS_CTE},
        observed AS (
          SELECT NULLIF(BTRIM(state.store_path), '') AS store_path
          FROM view_agent_current_system_states state
          WHERE state.hostname = $3
          ORDER BY state.timestamp DESC, state.id DESC
          LIMIT 1
        )
        SELECT o.position, o.git_commit_hash
        FROM ordered o, observed
        WHERE observed.store_path IS NOT NULL
          AND EXISTS (
            SELECT 1 FROM derivations d
            WHERE d.commit_id = o.id
              AND d.derivation_type = 'nixos'
              AND d.derivation_name = $4
              AND COALESCE(d.store_path, d.expected_store_path) = observed.store_path
          )
        ORDER BY o.position
        LIMIT 1
        "#
    ))
    .bind(flake_id)
    .bind(system.snapshot_ready)
    .bind(&system.hostname)
    .bind(&system.configuration_name)
    .fetch_optional(pool)
    .await?;
    let (running_commit, running_position) = match closure_match {
        Some((position, sha)) => (Some(sha), Some(position)),
        None => {
            let sha = resolve_observational_current_revision(pool, system_id).await?;
            let position = match sha.as_deref() {
                Some(sha) => {
                    sqlx::query_scalar::<_, i64>(&format!(
                        "WITH {ORDERED_COMMITS_CTE} SELECT position FROM ordered WHERE git_commit_hash = $3"
                    ))
                    .bind(flake_id)
                    .bind(system.snapshot_ready)
                    .bind(sha)
                    .fetch_optional(pool)
                    .await?
                }
                None => None,
            };
            (sha, position)
        }
    };

    // SECURITY: Peer hostnames are filtered by the same environment membership
    // rule as the dashboard timeline. A non-admin caller does not learn the
    // hostnames of systems outside its environments, and a system without an
    // environment is visible only to administrators.
    let rows = sqlx::query_as::<_, PositionCommitRow>(&format!(
        r#"
        WITH {ORDERED_COMMITS_CTE},
        peers AS (
          SELECT v.current_commit_hash AS sha,
                 array_agg(DISTINCT peer.hostname ORDER BY peer.hostname) AS hostnames
          FROM systems peer
          JOIN view_system_deployment_status v ON v.hostname = peer.hostname
          WHERE peer.flake_id = $1
            AND peer.id <> $3
            AND peer.is_active = TRUE
            AND v.current_commit_hash IS NOT NULL
            AND ($5::uuid IS NULL OR EXISTS (
              SELECT 1 FROM user_environment_memberships uem
              WHERE uem.user_id = $5 AND uem.environment_id = peer.environment_id
            ))
          GROUP BY v.current_commit_hash
        )
        SELECT o.position, o.git_commit_hash AS sha, o.message, o.author,
               o.commit_timestamp AS committed_at,
               -- Same rule as `deployed_here` in `list_recent_commits_for_system`.
               EXISTS (
                 SELECT 1
                 FROM system_states state
                 JOIN derivations deployed
                   ON state.store_path = COALESCE(deployed.store_path, deployed.expected_store_path)
                 WHERE state.hostname = $7
                   AND deployed.commit_id = o.id
                   AND deployed.derivation_type = 'nixos'
                   AND deployed.derivation_name = $6
               ) AS deployed_here,
               build.build_state,
               COALESCE(peers.hostnames, ARRAY[]::text[]) AS peers
        FROM ordered o
        LEFT JOIN peers ON peers.sha = o.git_commit_hash
        LEFT JOIN LATERAL (
          -- A finished success wins: the closure exists even when a retry or
          -- requeue of the same derivation is queued or running afterwards.
          -- `ready` additionally needs a completed cache push for the output,
          -- the evidence view_system_deployment_status requires for a
          -- deployable build; a success without it is `unpublished`.
          SELECT CASE
            WHEN COUNT(*) FILTER (WHERE bj.status = 'success' AND pub.published) > 0 THEN 'ready'
            WHEN COUNT(*) FILTER (WHERE bj.status = 'success') > 0 THEN 'unpublished'
            WHEN COUNT(*) FILTER (WHERE bj.status IN ('queued', 'building', 'cancelling')) > 0
              THEN 'building'
            WHEN COUNT(*) FILTER (WHERE bj.status = 'failed') > 0 THEN 'failed'
            ELSE 'not_built'
          END AS build_state
          -- INVARIANT: A job and its cache push belong to the derivation that
          -- EXECUTED the build, which is not always this commit's derivation.
          -- A commit that reuses a build has an alias derivation
          -- (`reused_from_derivation_id`) that owns no job of its own. It can
          -- read jobs of its physical origin, and it can adopt a successful job
          -- (`reused_build_job_id`) that belongs to a different alias than the
          -- origin. Publication is therefore checked on the derivation that
          -- owns each job (`exec`), the same rule the reuse admission applies
          -- (`derivation_publication_eligible(source.id, ...)`). Checking the
          -- origin only reported an output built and published on an alias
          -- as `unpublished`.
          FROM derivations d
          JOIN build_jobs bj
            ON bj.derivation_id = d.id
            OR bj.derivation_id = COALESCE(d.reused_from_derivation_id, d.id)
            OR bj.id = d.reused_build_job_id
          JOIN derivations exec ON exec.id = bj.derivation_id
          CROSS JOIN LATERAL (
            SELECT EXISTS (
              SELECT 1 FROM cache_push_jobs cpj
              WHERE cpj.derivation_id = exec.id
                AND cpj.status = 'completed'
                AND cpj.store_path = COALESCE(exec.store_path, exec.expected_store_path)
            ) AS published
          ) pub
          WHERE d.commit_id = o.id
            AND d.derivation_type = 'nixos'
            AND d.derivation_name = $6
        ) build ON TRUE
        WHERE o.position < $4
        ORDER BY o.position
        "#
    ))
    .bind(flake_id)
    .bind(system.snapshot_ready)
    .bind(system_id)
    .bind(flake_position_limit(running_position))
    .bind(peer_visibility_user)
    .bind(&system.configuration_name)
    .bind(&system.hostname)
    .fetch_all(pool)
    .await?;

    let unknown_reason = classify_flake_position(
        !rows.is_empty(),
        system.has_observation,
        running_commit.as_deref(),
        running_position,
    );
    let last_tracked_position = rows
        .iter()
        .find(|row| row.deployed_here)
        .map(|row| row.position);
    let commits = rows
        .into_iter()
        .map(|row| FlakePositionCommit {
            position: row.position,
            sha: row.sha,
            message: row.message.unwrap_or_default(),
            author: row.author.unwrap_or_else(|| "unknown".to_string()),
            committed_at: row.committed_at,
            build_state: parse_build_state(&row.build_state),
            peers: row.peers,
        })
        .collect();

    Ok(Some(SystemFlakePositionResponse {
        flake_id: Some(flake_id),
        flake_name: system.flake_name,
        branch: system.branch,
        ordering,
        commits,
        running_position: running_position.filter(|_| unknown_reason.is_none()),
        running_commit,
        unknown_reason,
        last_tracked_position,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_known_position() {
        assert_eq!(
            classify_flake_position(true, true, Some("abc"), Some(0)),
            None
        );
        assert_eq!(
            classify_flake_position(true, true, Some("abc"), Some(7)),
            None
        );
    }

    #[test]
    fn classifies_unknown_positions_in_order() {
        use FlakePositionUnknownReason as R;
        assert_eq!(
            classify_flake_position(false, false, None, None),
            Some(R::NoHistory)
        );
        assert_eq!(
            classify_flake_position(true, false, None, None),
            Some(R::NotObserved)
        );
        assert_eq!(
            classify_flake_position(true, true, None, None),
            Some(R::Untracked)
        );
        assert_eq!(
            classify_flake_position(true, true, Some("abc"), None),
            Some(R::OffBranch)
        );
        assert_eq!(
            classify_flake_position(true, true, Some("abc"), Some(FLAKE_POSITION_MAX_COMMITS)),
            Some(R::BeyondWindow)
        );
    }

    #[test]
    fn limit_covers_running_commit_and_trailing_context() {
        assert_eq!(flake_position_limit(None), FLAKE_POSITION_MIN_COMMITS);
        assert_eq!(flake_position_limit(Some(3)), FLAKE_POSITION_MIN_COMMITS);
        assert_eq!(flake_position_limit(Some(60)), 63);
        assert_eq!(
            flake_position_limit(Some(10_000)),
            FLAKE_POSITION_MAX_COMMITS
        );
    }
}
