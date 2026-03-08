//! Cache management view - configure cache destinations and monitor push jobs.

use dioxus::prelude::*;

use crate::api::client;
use crate::api::models::{CacheDestination, CachePushJob, CreateCacheDestination};
use crate::theme;

#[derive(Clone, Copy, PartialEq)]
enum CachesTab {
    Destinations,
    PushJobs,
}

/// Cache management page
#[component]
pub fn CachesView() -> Element {
    let mut active_tab = use_signal(|| CachesTab::Destinations);

    rsx! {
        div {
            class: "space-y-6",

            header {
                class: "flex flex-col gap-4",
                div {
                    h1 { class: "{theme::typography::PAGE_TITLE}", "Cache Management" }
                    p {
                        class: "text-sm {theme::text::SECONDARY}",
                        "Configure binary cache destinations and monitor artifact push jobs."
                    }
                }

                // Tabs
                div {
                    class: "flex border-b border-slate-700",
                    button {
                        class: if active_tab() == CachesTab::Destinations {
                            "px-4 py-2 border-b-2 border-blue-500 text-blue-400 font-medium"
                        } else {
                            "px-4 py-2 border-b-2 border-transparent text-slate-400 hover:text-white transition-colors"
                        },
                        onclick: move |_| active_tab.set(CachesTab::Destinations),
                        "Cache Destinations"
                    }
                    button {
                        class: if active_tab() == CachesTab::PushJobs {
                            "px-4 py-2 border-b-2 border-blue-500 text-blue-400 font-medium"
                        } else {
                            "px-4 py-2 border-b-2 border-transparent text-slate-400 hover:text-white transition-colors"
                        },
                        onclick: move |_| active_tab.set(CachesTab::PushJobs),
                        "Push Jobs"
                    }
                }
            }

            // Tab content
            match active_tab() {
                CachesTab::Destinations => rsx! {
                    CacheDestinationsList {}
                },
                CachesTab::PushJobs => rsx! {
                    CachePushJobsList {}
                },
            }
        }
    }
}

/// List of cache destinations with CRUD operations
#[component]
fn CacheDestinationsList() -> Element {
    let destinations = use_resource(|| async move {
        client::fetch_cache_destinations(false).await
    });

    rsx! {
        div {
            class: "space-y-4",

            // Header with Add button
            div {
                class: "flex justify-between items-center",
                h2 {
                    class: "text-lg font-semibold text-white",
                    "Cache Destinations"
                }
                button {
                    class: "px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg transition-colors",
                    onclick: move |_| {
                        // TODO: Open add modal
                    },
                    "+ Add Destination"
                }
            }

            // List
            match &*destinations.read_unchecked() {
                Some(Ok(dests)) => rsx! {
                    if dests.is_empty() {
                        div {
                            class: "text-center py-12 text-slate-400",
                            p { "No cache destinations configured." }
                            p { class: "text-sm mt-2", "Add a cache destination to start pushing build artifacts." }
                        }
                    } else {
                        div {
                            class: "space-y-2",
                            for dest in dests {
                                CacheDestinationCard { destination: dest.clone() }
                            }
                        }
                    }
                },
                Some(Err(e)) => rsx! {
                    div {
                        class: "p-4 bg-red-900/20 border border-red-700 rounded-lg text-red-400",
                        "Error loading cache destinations: {e}"
                    }
                },
                None => rsx! {
                    div {
                        class: "text-center py-12 text-slate-400",
                        "Loading..."
                    }
                },
            }
        }
    }
}

/// Individual cache destination card
#[component]
fn CacheDestinationCard(destination: CacheDestination) -> Element {
    let enabled_class = if destination.enabled {
        "bg-green-900/20 text-green-400"
    } else {
        "bg-gray-700/20 text-gray-400"
    };
    
    let last_used_str = destination.last_used_at.map(|d| d.format("%Y-%m-%d %H:%M").to_string());
    let created_str = destination.created_at.format("%Y-%m-%d").to_string();

    rsx! {
        div {
            class: "p-4 bg-slate-800 border border-slate-700 rounded-lg hover:border-slate-600 transition-colors",
            
            div {
                class: "flex justify-between items-start",
                
                div {
                    class: "flex-1",
                    div {
                        class: "flex items-center gap-3",
                        h3 {
                            class: "text-lg font-semibold text-white",
                            "{destination.name}"
                        }
                        span {
                            class: "px-2 py-1 text-xs rounded {enabled_class}",
                            if destination.enabled { "Enabled" } else { "Disabled" }
                        }
                        span {
                            class: "px-2 py-1 text-xs bg-blue-900/20 text-blue-400 rounded",
                            "{destination.cache_type}"
                        }
                    }
                    
                    if let Some(ref url) = destination.push_to {
                        p {
                            class: "text-sm text-slate-400 mt-1",
                            "→ {url}"
                        }
                    }
                    
                    div {
                        class: "flex gap-4 mt-2 text-xs text-slate-500",
                        if let Some(ref last_used) = last_used_str {
                            span { "Last used: {last_used}" }
                        } else {
                            span { "Never used" }
                        }
                        span { "Created: {created_str}" }
                    }
                }
                
                div {
                    class: "flex gap-2",
                    button {
                        class: "px-3 py-1 text-sm bg-slate-700 hover:bg-slate-600 text-white rounded transition-colors",
                        onclick: move |_| {
                            // TODO: Open edit modal
                        },
                        "Edit"
                    }
                    button {
                        class: "px-3 py-1 text-sm bg-red-900/20 hover:bg-red-900/40 text-red-400 rounded transition-colors",
                        onclick: move |_| {
                            // TODO: Confirm and delete
                        },
                        "Delete"
                    }
                }
            }
        }
    }
}

/// List of cache push jobs with filtering and actions
#[component]
fn CachePushJobsList() -> Element {
    let mut status_filter = use_signal(|| Option::<String>::None);
    let jobs = use_resource(move || {
        let filter = status_filter.read().clone();
        async move {
            client::fetch_cache_push_jobs(filter.as_deref(), 50, 0).await
        }
    });

    rsx! {
        div {
            class: "space-y-4",

            // Header with filter
            div {
                class: "flex justify-between items-center",
                h2 {
                    class: "text-lg font-semibold text-white",
                    "Cache Push Jobs"
                }
                
                select {
                    class: "px-3 py-2 bg-slate-800 border border-slate-700 text-white rounded-lg",
                    onchange: move |evt| {
                        let value = evt.value();
                        status_filter.set(if value.is_empty() { None } else { Some(value) });
                    },
                    option { value: "", "All Statuses" }
                    option { value: "pending", "Pending" }
                    option { value: "in_progress", "In Progress" }
                    option { value: "failed", "Failed" }
                    option { value: "completed", "Completed" }
                    option { value: "cancelled", "Cancelled" }
                    option { value: "permanently_failed", "Permanently Failed" }
                }
            }

            // Job list
            match &*jobs.read_unchecked() {
                Some(Ok(job_list)) => rsx! {
                    if job_list.is_empty() {
                        div {
                            class: "text-center py-12 text-slate-400",
                            "No cache push jobs found."
                        }
                    } else {
                        div {
                            class: "overflow-x-auto",
                            table {
                                class: "w-full",
                                thead {
                                    tr {
                                        class: "border-b border-slate-700",
                                        th { class: "px-4 py-2 text-left text-xs font-semibold text-slate-400 uppercase", "ID" }
                                        th { class: "px-4 py-2 text-left text-xs font-semibold text-slate-400 uppercase", "Status" }
                                        th { class: "px-4 py-2 text-left text-xs font-semibold text-slate-400 uppercase", "Destination" }
                                        th { class: "px-4 py-2 text-left text-xs font-semibold text-slate-400 uppercase", "Attempts" }
                                        th { class: "px-4 py-2 text-left text-xs font-semibold text-slate-400 uppercase", "Scheduled" }
                                        th { class: "px-4 py-2 text-left text-xs font-semibold text-slate-400 uppercase", "Actions" }
                                    }
                                }
                                tbody {
                                    for job in job_list {
                                        CachePushJobRow { job: job.clone() }
                                    }
                                }
                            }
                        }
                    }
                },
                Some(Err(e)) => rsx! {
                    div {
                        class: "p-4 bg-red-900/20 border border-red-700 rounded-lg text-red-400",
                        "Error loading push jobs: {e}"
                    }
                },
                None => rsx! {
                    div {
                        class: "text-center py-12 text-slate-400",
                        "Loading..."
                    }
                },
            }
        }
    }
}

/// Individual job row in the table
#[component]
fn CachePushJobRow(job: CachePushJob) -> Element {
    let status_class = match job.status.as_str() {
        "completed" => "text-green-400",
        "failed" | "permanently_failed" => "text-red-400",
        "in_progress" => "text-blue-400",
        "pending" => "text-yellow-400",
        "cancelled" => "text-gray-400",
        _ => "text-slate-400",
    };
    
    let scheduled_str = job.scheduled_at.format("%Y-%m-%d %H:%M").to_string();

    rsx! {
        tr {
            class: "border-b border-slate-800 hover:bg-slate-800/50",
            td { class: "px-4 py-3 text-sm text-white", "{job.id}" }
            td { class: "px-4 py-3 text-sm {status_class}", "{job.status}" }
            td {
                class: "px-4 py-3 text-sm text-slate-300",
                if let Some(ref dest) = job.cache_destination {
                    "{dest}"
                } else {
                    span { class: "text-slate-500", "(default)" }
                }
            }
            td { class: "px-4 py-3 text-sm text-slate-300", "{job.attempts}" }
            td {
                class: "px-4 py-3 text-sm text-slate-400",
                "{scheduled_str}"
            }
            td {
                class: "px-4 py-3 text-sm",
                div {
                    class: "flex gap-2",
                    if job.status == "failed" || job.status == "permanently_failed" {
                        button {
                            class: "px-2 py-1 text-xs bg-blue-600 hover:bg-blue-700 text-white rounded",
                            onclick: move |_| {
                                let job_id = job.id;
                                spawn(async move {
                                    let _ = client::retry_cache_push_job(job_id).await;
                                    // TODO: Refresh list
                                });
                            },
                            "Retry"
                        }
                    }
                    if job.status == "pending" || job.status == "failed" {
                        button {
                            class: "px-2 py-1 text-xs bg-red-900/20 hover:bg-red-900/40 text-red-400 rounded",
                            onclick: move |_| {
                                let job_id = job.id;
                                spawn(async move {
                                    let _ = client::cancel_cache_push_job(job_id).await;
                                    // TODO: Refresh list
                                });
                            },
                            "Cancel"
                        }
                    }
                }
            }
        }
    }
}
