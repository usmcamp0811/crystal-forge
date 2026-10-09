//! Loads finite inventory observations and gives every cache surface one meaning.
//!
//! Refresh replaces the previous observation; no historical measurement is
//! persisted here. A disabled destination always hides earlier values. Logical
//! bytes exclude unknown legacy object sizes and cannot establish physical
//! usage, capacity or cache emptiness. Tracked objects cannot establish a Nix
//! path total. Error bodies and unknown reason text never enter presentation.

use super::*;
use crate::api::models::{CacheMetricsStatus, CacheStorageMetrics};
use std::future::{Future, poll_fn};
use std::task::Poll;

// CONCURRENCY: Three requests bound provider load while allowing slow endpoints
// to share a page load. Each batch completes before the next begins. There is no
// retry or timer; only a successful list load or explicit refresh starts a pass.
const MAX_IN_FLIGHT: usize = 3;

/// Separates a typed observation from an opaque API request failure.
#[derive(Clone, PartialEq)]
pub(super) enum Observation {
    /// Carries credential-free server evidence with explicit status and basis.
    Reported(CacheStorageMetrics),
    /// Discards all network/status bodies and exposes only a static explanation.
    RequestFailed,
}

/// Holds only one page-load generation of response-only observations.
#[derive(Clone, PartialEq)]
pub(super) struct Snapshot {
    /// Identifies the explicit refresh/list-load generation.
    pub generation: u32,
    /// Shares one observation per destination across cards, table and details.
    pub values: HashMap<i32, Observation>,
}

/// Loads a finite destination set with at most three simultaneous API requests.
pub(super) async fn load(generation: u32, ids: Vec<i32>) -> Snapshot {
    Snapshot {
        generation,
        values: load_with(&ids, client::fetch_cache_storage_metrics).await,
    }
}

async fn load_with<F, Fut>(ids: &[i32], fetch: F) -> HashMap<i32, Observation>
where
    F: Fn(i32) -> Fut,
    Fut: Future<Output = Result<CacheStorageMetrics, ApiClientError>>,
{
    let mut values = HashMap::new();
    for batch in ids.chunks(MAX_IN_FLIGHT) {
        let mut pending: Vec<_> = batch
            .iter()
            .map(|id| (*id, Some(Box::pin(fetch(*id)))))
            .collect();
        poll_fn(|cx| {
            for (id, request) in &mut pending {
                if let Some(future) = request {
                    if let Poll::Ready(result) = future.as_mut().poll(cx) {
                        // SECURITY: Discard transport/status bodies immediately.
                        values.insert(
                            *id,
                            match result {
                                Ok(value) => Observation::Reported(value),
                                Err(_) => Observation::RequestFailed,
                            },
                        );
                        *request = None;
                    }
                }
            }
            if pending.iter().all(|(_, request)| request.is_none()) {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        })
        .await;
    }
    values
}

/// Supplies the same values and static basis explanations to every surface.
#[derive(Clone, PartialEq)]
pub(super) struct Display {
    /// Formats known logical bytes as GiB; missing evidence says Unavailable.
    pub storage: String,
    /// Labels native counts as objects, never as Nix paths.
    pub objects: String,
    /// Explains the logical-byte basis or static unavailable reason.
    pub storage_help: String,
    /// Explains the tracked-object basis or static unavailable reason.
    pub objects_help: String,
    /// Identifies the local successful observation time when supplied.
    pub observed: Option<String>,
}

const STORAGE_HELP: &str = "Reported logical bytes: sum of known client-reported uncompressed object sizes. Legacy objects with unknown sizes are excluded. This is not physical storage usage or capacity; zero does not prove an empty cache.";
const OBJECT_HELP: &str = "Live tracked objects, including NAR and metadata objects; excludes tombstones and untracked storage objects. This is not a Nix store path count.";

fn absent(label: &str, reason: &str) -> Display {
    Display {
        storage: label.into(),
        objects: label.into(),
        storage_help: reason.into(),
        objects_help: reason.into(),
        observed: None,
    }
}

fn reason(code: &str) -> &'static str {
    match code {
        "disabled" => {
            "Metrics unavailable: destination disabled. Earlier observations are not retained."
        }
        "requires_admin" => {
            "Metrics unavailable: an Admin role is required to observe this provider."
        }
        "provider_totals_unsupported" => {
            "This provider has no supported cheap inventory totals. No provider inventory request is made."
        }
        "endpoint_unavailable" => {
            "The metrics endpoint is unavailable. This does not identify a server version or an empty cache."
        }
        "deadline_exceeded" => {
            "The bounded metrics observation did not finish before its deadline."
        }
        "transport_unavailable" => {
            "Metrics transport unavailable. Review write transport and server trust."
        }
        "response_unavailable" => "The metrics response could not be read. No values are shown.",
        "access_denied" => "The metrics endpoint denied access. No values are shown.",
        "unexpected_status" => {
            "The metrics endpoint returned an unexpected status. No values are shown."
        }
        "target_policy" => "The configured metrics target is not permitted.",
        "invalid_configuration" => {
            "Metrics unavailable: the selected provider configuration is invalid."
        }
        "invalid_stats" | "stats_too_large" => {
            "The provider returned invalid inventory metadata. No values are shown."
        }
        "destination_unavailable" => "The destination is no longer available for observation.",
        _ => "No usable inventory observation is available. Refresh to try again.",
    }
}

fn from_observation(enabled: bool, observation: Option<&Observation>) -> Display {
    if !enabled {
        return absent("Unavailable", reason("disabled"));
    }
    let Some(observation) = observation else {
        return absent(
            "Loading…",
            "Loading a bounded inventory observation; no earlier measurement is shown.",
        );
    };
    let Observation::Reported(value) = observation else {
        return absent("Unavailable", reason("request_failed"));
    };
    if value.status != CacheMetricsStatus::Available {
        return absent("Unavailable", reason(&value.reason_code));
    }
    let mut display = absent(
        "Unavailable",
        "No value with a supported measurement basis was reported.",
    );
    if value.storage_bytes_basis.as_deref() == Some("reported_logical") {
        if let Some(bytes) = value.storage_bytes {
            let gib = bytes as f64 / 1_073_741_824.0;
            display.storage = if bytes > 0 && gib < 0.01 {
                "<0.01 GiB".into()
            } else {
                format!("{gib:.2} GiB")
            };
            display.storage_help = format!("{bytes} reported logical bytes. {STORAGE_HELP}");
        }
    }
    if value.object_count_basis.as_deref() == Some("live_tracked_objects") {
        if let Some(objects) = value.object_count {
            display.objects = format!("{objects} objects");
            display.objects_help = OBJECT_HELP.into();
        }
    }
    if display.storage != "Unavailable" || display.objects != "Unavailable" {
        display.observed = value.measured_at.map(|time| {
            format!(
                "Observed {} UTC (server observation time)",
                time.format("%Y-%m-%d %H:%M:%S")
            )
        });
    }
    display
}

/// Hides earlier-generation values immediately on refresh or disabling.
pub(super) fn display(
    destination: &CacheDestination,
    snapshot: Option<&Snapshot>,
    generation: u32,
) -> Display {
    let current = snapshot.filter(|snapshot| snapshot.generation == generation);
    let observation = current.and_then(|snapshot| snapshot.values.get(&destination.id));
    if current.is_some() && observation.is_none() && destination.enabled {
        return absent("Unavailable", reason("destination_unavailable"));
    }
    from_observation(destination.enabled, observation)
}

/// Renders provider-neutral details without capacity ratios or path inference.
pub(super) fn details(display: &Display) -> Element {
    rsx! {
        section { class: "panel-section", h3 { "Storage" }
            dl { class: "kv-grid",
                dt { "Reported logical size" } dd { title: "{display.storage_help}", "{display.storage}" }
                dt { "Paths / Objects" } dd { title: "{display.objects_help}", "{display.objects}" }
            }
            p { class: "help", "{display.storage_help}" }
            if display.objects_help != display.storage_help { p { class: "help", "{display.objects_help}" } }
            if let Some(observed) = &display.observed { p { class: "help", "{observed}" } }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    fn reported(bytes: Option<u64>, objects: Option<u64>) -> Observation {
        Observation::Reported(
            serde_json::from_value(serde_json::json!({
                "status": "available", "reason_code": "native_stats",
                "storage_bytes": bytes, "storage_bytes_basis": "reported_logical",
                "object_count": objects, "object_count_basis": "live_tracked_objects",
                "path_count": null, "measured_at": "2026-10-08T12:00:00Z"
            }))
            .unwrap(),
        )
    }

    #[test]
    fn logical_bytes_objects_zero_and_unknown_remain_distinct() {
        let known = from_observation(true, Some(&reported(Some(1_073_741_824), Some(7))));
        assert_eq!(known.storage, "1.00 GiB");
        assert_eq!(known.objects, "7 objects");
        assert!(known.storage_help.contains("not physical storage"));
        assert!(known.storage_help.contains("unknown sizes are excluded"));
        assert!(known.objects_help.contains("not a Nix store path count"));
        assert!(
            known
                .observed
                .as_ref()
                .unwrap()
                .contains("2026-10-08 12:00:00 UTC")
        );
        let zero = from_observation(true, Some(&reported(Some(0), Some(0))));
        assert_eq!(zero.storage, "0.00 GiB");
        assert_eq!(zero.objects, "0 objects");
        assert!(
            zero.storage_help
                .contains("zero does not prove an empty cache")
        );
        assert_eq!(
            from_observation(true, Some(&reported(Some(1), None))).storage,
            "<0.01 GiB"
        );
        let unknown = from_observation(true, Some(&reported(None, None)));
        assert_eq!(unknown.storage, "Unavailable");
        assert_eq!(unknown.objects, "Unavailable");
        assert!(unknown.observed.is_none());
    }

    #[test]
    fn status_basis_and_disabled_gate_values_without_echoing_unknown_reason() {
        let Observation::Reported(mut value) = reported(Some(99), Some(9)) else {
            unreachable!()
        };
        for status in [
            CacheMetricsStatus::Unsupported,
            CacheMetricsStatus::Unavailable,
            CacheMetricsStatus::Error,
            CacheMetricsStatus::Unknown,
        ] {
            value.status = status;
            value.reason_code = "https://private-upstream.example/secret-marker".into();
            let rendered = from_observation(true, Some(&Observation::Reported(value.clone())));
            assert_eq!(rendered.storage, "Unavailable");
            assert_eq!(rendered.objects, "Unavailable");
            assert!(!rendered.storage_help.contains("private-upstream"));
            assert!(rendered.observed.is_none());
        }
        value.status = CacheMetricsStatus::Available;
        let disabled = from_observation(false, Some(&Observation::Reported(value.clone())));
        assert_eq!(disabled.storage, "Unavailable");
        assert!(disabled.storage_help.contains("disabled"));
        value.storage_bytes_basis = Some("physical_or_unknown".into());
        value.object_count_basis = None;
        value.path_count = Some(999);
        let rendered = from_observation(true, Some(&Observation::Reported(value)));
        assert_eq!(rendered.storage, "Unavailable");
        assert_eq!(rendered.objects, "Unavailable");
    }

    #[test]
    fn refresh_hides_prior_observation_and_never_infers_endpoint_version() {
        let destination: CacheDestination = serde_json::from_value(serde_json::json!({
            "id": 1, "name": "fixture", "cache_type": "Niks3", "enabled": true,
            "created_at": "2026-10-08T12:00:00Z", "updated_at": "2026-10-08T12:00:00Z"
        }))
        .unwrap();
        let snapshot = Snapshot {
            generation: 1,
            values: [(1, reported(Some(99), Some(9)))].into(),
        };
        assert_eq!(
            display(&destination, Some(&snapshot), 2).storage,
            "Loading…"
        );
        assert_eq!(
            display(&destination, Some(&snapshot), 1).objects,
            "9 objects"
        );
        let mut disabled = destination.clone();
        disabled.enabled = false;
        assert_eq!(
            display(&disabled, Some(&snapshot), 1).objects,
            "Unavailable"
        );
        assert!(reason("endpoint_unavailable").contains("does not identify a server version"));
        assert!(reason("requires_admin").contains("Admin role"));
        assert!(reason("provider_totals_unsupported").contains("No provider inventory request"));
    }

    #[test]
    fn metrics_dto_rejects_negative_or_fractional_totals() {
        for value in [
            serde_json::json!(-1),
            serde_json::json!(1.5),
            serde_json::json!("99"),
        ] {
            assert!(
                serde_json::from_value::<CacheStorageMetrics>(serde_json::json!({
                    "status": "available", "reason_code": "native_stats", "storage_bytes": value
                }))
                .is_err()
            );
        }
    }

    #[tokio::test]
    async fn observation_pass_is_bounded_finite_and_independent_of_one_failure() {
        let active = Rc::new(Cell::new(0));
        let peak = Rc::new(Cell::new(0));
        let calls = Rc::new(RefCell::new(Vec::new()));
        let values = load_with(&[1, 2, 3, 4, 5, 6, 7], |id| {
            let (active, peak, calls) = (active.clone(), peak.clone(), calls.clone());
            async move {
                calls.borrow_mut().push(id);
                active.set(active.get() + 1);
                peak.set(peak.get().max(active.get()));
                tokio::task::yield_now().await;
                active.set(active.get() - 1);
                if id == 2 {
                    Err(ApiClientError::Network("private-upstream-marker".into()))
                } else {
                    let Observation::Reported(value) = reported(Some(id as u64), Some(1)) else {
                        unreachable!()
                    };
                    Ok(value)
                }
            }
        })
        .await;
        assert_eq!(peak.get(), 3);
        assert_eq!(active.get(), 0);
        assert_eq!(*calls.borrow(), vec![1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(values.len(), 7);
        assert!(matches!(values.get(&2), Some(Observation::RequestFailed)));
        assert!(matches!(values.get(&7), Some(Observation::Reported(_))));
    }
}
