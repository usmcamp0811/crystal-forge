pub mod admin;
pub mod agent_heartbeat;
pub mod attention;
pub mod auth_identity;
pub mod build_jobs;
pub mod build_reservations;
pub mod builders;
pub mod cache_destinations;
pub mod cache_push;
pub mod commits;
pub mod commits_artifacts;
pub mod compliance;
pub mod config_health;
pub mod cve_scans;
pub mod cves;
pub mod dashboard;
pub mod deployment;
pub mod deployment_policies;
pub mod derivations;
pub mod environments;
pub mod eval_logs;
pub mod flake_credentials;
pub mod flakes;
pub mod hardening_scans;
pub mod navigation;
pub mod scanning;
pub mod status;
pub mod system_events;
pub mod system_states;
pub mod systems;
pub mod user_preferences;
pub mod users;

#[cfg(test)]
mod cve_scans_tests;

#[cfg(test)]
mod hardening_scans_tests;

#[cfg(test)]
mod eval_logs_tests;

#[cfg(test)]
mod scanning_tests;
