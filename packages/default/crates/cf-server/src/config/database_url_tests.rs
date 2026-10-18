//! Regression table for `DatabaseConfig::to_url` (TASK-490.25).
//!
//! `cf-config` owns the serializer and must not depend on a database client.
//! The server crate owns both clients that consume the string, so the table
//! lives here. Each row serializes with the real `to_url` and parses the
//! result with both clients. The assertions compare the configuration each
//! client recovers, not the URL text.
//!
//! SQLx exposes the recovered password through its rebuilt connection URL.
//! Decode that URL with the existing PostgreSQL parser rather than comparing
//! the input string to itself.

use super::DatabaseConfig;
use sqlx::ConnectOptions;
use sqlx::postgres::PgConnectOptions;
use std::path::PathBuf;
use std::str::FromStr;
use tokio_postgres::config::Host;

/// Expected connection target after a client parses the serialized URL.
enum Target {
    /// A Unix socket directory, such as `/run/postgresql`.
    Socket(&'static str),
    /// A TCP host name.
    Tcp(&'static str),
}

struct Row {
    label: &'static str,
    config: DatabaseConfig,
    target: Target,
}

fn config(host: &str, port: u16, user: &str, password: &str, name: &str) -> DatabaseConfig {
    DatabaseConfig {
        host: host.to_string(),
        port,
        user: user.to_string(),
        password: password.to_string(),
        name: name.to_string(),
    }
}

fn table() -> Vec<Row> {
    vec![
        Row {
            // The NixOS module default (`services.crystal-forge.database.host`).
            label: "default unix socket directory",
            config: config(
                "/run/postgresql",
                5432,
                "crystal_forge",
                "password",
                "crystal_forge",
            ),
            target: Target::Socket("/run/postgresql"),
        },
        Row {
            label: "ordinary network host",
            config: config(
                "db.example.internal",
                5432,
                "crystal_forge",
                "password",
                "crystal_forge",
            ),
            target: Target::Tcp("db.example.internal"),
        },
        Row {
            // Space, literal plus sign and URI delimiters in every component,
            // with a nondefault port. A form decoder would turn `+` into a
            // space. A percent decoder must keep it.
            label: "encoding-sensitive tcp values with nondefault port",
            config: config(
                "db.example.internal",
                6543,
                "cf user+1",
                "p@ss w+rd/:?#%",
                "cf db+1",
            ),
            target: Target::Tcp("db.example.internal"),
        },
        Row {
            label: "encoding-sensitive unix socket directory with nondefault port",
            config: config(
                "/run/pg dir+sock",
                6543,
                "cf user+1",
                "p@ss w+rd/:?#%",
                "cf db+1",
            ),
            target: Target::Socket("/run/pg dir+sock"),
        },
    ]
}

#[test]
fn to_url_round_trips_through_tokio_postgres() {
    for row in table() {
        let url = row.config.to_url();
        let parsed = tokio_postgres::Config::from_str(&url)
            .unwrap_or_else(|e| panic!("{}: tokio_postgres rejected the URL: {e}", row.label));

        let expected_host = match &row.target {
            Target::Socket(dir) => Host::Unix(PathBuf::from(dir)),
            Target::Tcp(host) => Host::Tcp((*host).to_string()),
        };
        assert_eq!(parsed.get_hosts(), &[expected_host], "{}: host", row.label);
        assert_eq!(
            parsed.get_ports(),
            &[row.config.port],
            "{}: port",
            row.label
        );
        assert_eq!(
            parsed.get_user(),
            Some(row.config.user.as_str()),
            "{}: user",
            row.label
        );
        assert_eq!(
            parsed.get_password(),
            Some(row.config.password.as_bytes()),
            "{}: password",
            row.label
        );
        assert_eq!(
            parsed.get_dbname(),
            Some(row.config.name.as_str()),
            "{}: database",
            row.label
        );
    }
}

#[test]
fn to_url_round_trips_through_sqlx() {
    for row in table() {
        let url = row.config.to_url();
        let parsed = PgConnectOptions::from_str(&url)
            .unwrap_or_else(|e| panic!("{}: sqlx rejected the URL: {e}", row.label));

        match &row.target {
            Target::Socket(dir) => assert_eq!(
                parsed.get_socket(),
                Some(&PathBuf::from(dir)),
                "{}: socket",
                row.label
            ),
            Target::Tcp(host) => {
                assert_eq!(parsed.get_socket(), None, "{}: socket", row.label);
                assert_eq!(parsed.get_host(), *host, "{}: host", row.label);
            }
        }
        assert_eq!(parsed.get_port(), row.config.port, "{}: port", row.label);
        // SQLx-specific tuning parameters are not understood by tokio_postgres.
        // Passwords reside in the rebuilt URL's authority, not its query.
        let mut recovered_url = parsed.to_url_lossy();
        recovered_url.set_query(None);
        let recovered = tokio_postgres::Config::from_str(recovered_url.as_str())
            .unwrap_or_else(|e| panic!("{}: rebuilt SQLx URL rejected: {e}", row.label));
        assert_eq!(
            recovered.get_password(),
            Some(row.config.password.as_bytes()),
            "{}: SQLx recovered password",
            row.label
        );
        assert_eq!(
            parsed.get_username(),
            row.config.user,
            "{}: user",
            row.label
        );
        assert_eq!(
            parsed.get_database(),
            Some(row.config.name.as_str()),
            "{}: database",
            row.label
        );
    }
}
