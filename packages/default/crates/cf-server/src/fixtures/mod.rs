//! Fixture data support.
//!
//! Provides database seeding from the design fixture JSON file
//! (`crystal-forge.fixtures.json`). When `FIXTURE_JSON_PATH` is set at server
//! startup, the seed module populates application tables so that the regular
//! API handlers return genuine data — no middleware interception needed.

pub mod seed;

pub use seed::seed_from_fixture;
