use std::collections::VecDeque;
use std::fmt::Debug;
use tokio::sync::Mutex;

type SystemEvalQueue = Arc<Mutex<VecDeque<Job>>>;

#[derive(Debug, Clone)]
struct SystemEvalJob {
    flake_name: String,
    flake_url: String,
    commit: String,
    system_name: Option<String>,
    enqueued_at: chrono::DateTime<chrono::Utc>,
}
