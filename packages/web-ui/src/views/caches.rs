//! Cache management view - configure cache destinations and monitor push jobs.

use dioxus::prelude::*;

use crate::api::client;
use crate::api::models::{
    CacheDestination, CachePushJob, CreateCacheDestination, UpdateCacheDestination,
};
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
                    h1 { class: "{theme::typography::PAGE_TITLE} {theme::text::PRIMARY}", "Cache Management" }
                    p {
                        class: "text-sm {theme::text::SECONDARY}",
                        "Configure binary cache destinations and monitor artifact push jobs."
                    }
                }

                // Tabs
                div {
                    class: "flex border-b {theme::surface::DIVIDER}",
                    button {
                        class: if active_tab() == CachesTab::Destinations {
                            "px-4 py-2 border-b-2 border-blue-500 text-blue-400 font-medium"
                        } else {
                            "px-4 py-2 border-b-2 border-transparent {theme::text::SECONDARY} hover:{theme::text::PRIMARY} transition-colors"
                        },
                        onclick: move |_| active_tab.set(CachesTab::Destinations),
                        "Cache Destinations"
                    }
                    button {
                        class: if active_tab() == CachesTab::PushJobs {
                            "px-4 py-2 border-b-2 border-blue-500 text-blue-400 font-medium"
                        } else {
                            "px-4 py-2 border-b-2 border-transparent {theme::text::SECONDARY} hover:{theme::text::PRIMARY} transition-colors"
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
    let mut refresh_nonce = use_signal(|| 0_u32);
    let destinations = use_resource(move || {
        let _nonce = refresh_nonce();
        async move { client::fetch_cache_destinations(false).await }
    });
    
    let mut show_add_modal = use_signal(|| false);
    let mut add_name = use_signal(String::new);
    let mut add_type = use_signal(|| "Nix".to_string());
    let mut add_push_to = use_signal(String::new);
    let mut add_attic_cache_name = use_signal(String::new);
    let mut add_error = use_signal(|| None::<String>);
    let mut add_submitting = use_signal(|| false);

    rsx! {
        div {
            class: "space-y-4",

            // Header with Add button
            div {
                class: "flex justify-between items-center",
                h2 {
                    class: "{theme::typography::SECTION_TITLE} {theme::text::PRIMARY}",
                    "Cache Destinations"
                }
                button {
                    class: "px-4 py-2 rounded-lg text-sm font-medium {theme::interactive::PRIMARY_BTN}",
                    onclick: move |_| {
                        show_add_modal.set(true);
                    },
                    "+ Add Destination"
                }
            }

            // List
            match &*destinations.read_unchecked() {
                Some(Ok(dests)) => rsx! {
                    if dests.is_empty() {
                        div {
                            class: "{theme::presets::CARD} text-center py-12",
                            p { class: "{theme::text::SECONDARY}", "No cache destinations configured." }
                            p { class: "{theme::text::MUTED} text-sm mt-2", "Add your first cache destination to start pushing build artifacts." }
                        }
                    } else {
                        div {
                            class: "grid gap-4",
                            for dest in dests {
                                CacheDestinationCard {
                                    destination: dest.clone(),
                                    on_change: move |_| refresh_nonce.set(refresh_nonce() + 1),
                                }
                            }
                        }
                    }
                },
                Some(Err(e)) => rsx! {
                    div {
                        class: "{theme::presets::CARD} border-red-500/30 bg-red-500/5",
                        p { class: "text-red-400", "Error loading destinations: {e}" }
                    }
                },
                None => rsx! {
                    div {
                        class: "{theme::presets::CARD} text-center py-12",
                        p { class: "{theme::text::SECONDARY}", "Loading cache destinations..." }
                    }
                },
            }
            
            // Add modal placeholder
            if show_add_modal() {
                div {
                    class: "fixed inset-0 bg-black/50 flex items-center justify-center z-50",
                    onclick: move |_| show_add_modal.set(false),
                    div {
                        class: "{theme::presets::CARD} max-w-2xl w-full mx-4",
                        onclick: move |e| e.stop_propagation(),
                        
                        div {
                            class: "flex justify-between items-center mb-6",
                            h3 { class: "{theme::typography::SECTION_TITLE} {theme::text::PRIMARY}", "Add Cache Destination" }
                            button {
                                class: "{theme::text::SECONDARY} hover:{theme::text::PRIMARY}",
                                onclick: move |_| show_add_modal.set(false),
                                "✕"
                            }
                        }
                        
                        div {
                            class: "space-y-4",
                            div {
                                label { class: "block text-sm {theme::text::SECONDARY} mb-1", "Name" }
                                input {
                                    class: "w-full px-3 py-2 rounded-lg text-sm {theme::interactive::INPUT} {theme::text::PRIMARY}",
                                    placeholder: "main-cache",
                                    value: add_name(),
                                    oninput: move |evt| add_name.set(evt.value()),
                                }
                            }

                            div {
                                label { class: "block text-sm {theme::text::SECONDARY} mb-1", "Type" }
                                select {
                                    class: "w-full px-3 py-2 rounded-lg text-sm {theme::interactive::INPUT} {theme::text::PRIMARY}",
                                    value: add_type(),
                                    onchange: move |evt| add_type.set(evt.value()),
                                    option { class: "text-slate-900 bg-white", value: "Nix", "Nix" }
                                    option { class: "text-slate-900 bg-white", value: "Http", "Http" }
                                    option { class: "text-slate-900 bg-white", value: "S3", "S3" }
                                    option { class: "text-slate-900 bg-white", value: "Attic", "Attic" }
                                }
                            }

                            if add_type() == "Attic" {
                                div {
                                    label { class: "block text-sm {theme::text::SECONDARY} mb-1", "Attic Cache Name" }
                                    input {
                                        class: "w-full px-3 py-2 rounded-lg text-sm {theme::interactive::INPUT} {theme::text::PRIMARY}",
                                        placeholder: "my-attic-cache",
                                        value: add_attic_cache_name(),
                                        oninput: move |evt| add_attic_cache_name.set(evt.value()),
                                    }
                                }
                            } else {
                                div {
                                    label { class: "block text-sm {theme::text::SECONDARY} mb-1", "Destination URL" }
                                    input {
                                        class: "w-full px-3 py-2 rounded-lg text-sm {theme::interactive::INPUT} {theme::text::PRIMARY}",
                                        placeholder: "https://cache.example.com or s3://bucket",
                                        value: add_push_to(),
                                        oninput: move |evt| add_push_to.set(evt.value()),
                                    }
                                }
                            }

                            if let Some(err) = add_error() {
                                p { class: "text-sm text-red-400", "{err}" }
                            }
                        }

                        div {
                            class: "mt-6 flex justify-end gap-3",
                            button {
                                class: "px-4 py-2 rounded-lg text-sm font-medium {theme::interactive::GHOST_BTN} {theme::text::SECONDARY}",
                                onclick: move |_| {
                                    show_add_modal.set(false);
                                    add_error.set(None);
                                },
                                "Cancel"
                            }
                            button {
                                class: "px-4 py-2 rounded-lg text-sm font-medium {theme::interactive::PRIMARY_BTN}",
                                disabled: add_submitting(),
                                onclick: move |_| {
                                    let name = add_name().trim().to_string();
                                    let cache_type = add_type();
                                    let push_to = add_push_to().trim().to_string();
                                    let attic_cache_name = add_attic_cache_name().trim().to_string();

                                    if name.is_empty() {
                                        add_error.set(Some("Name is required".to_string()));
                                        return;
                                    }

                                    if cache_type == "Attic" && attic_cache_name.is_empty() {
                                        add_error.set(Some("Attic cache name is required".to_string()));
                                        return;
                                    }

                                    if cache_type != "Attic" && push_to.is_empty() {
                                        add_error.set(Some("Destination URL is required".to_string()));
                                        return;
                                    }

                                    add_submitting.set(true);
                                    add_error.set(None);

                                    spawn(async move {
                                        let req = CreateCacheDestination {
                                            name,
                                            cache_type: cache_type.clone(),
                                            push_to: if cache_type == "Attic" { None } else { Some(push_to) },
                                            enabled: Some(true),
                                            signing_key_path: None,
                                            compression: None,
                                            s3_region: None,
                                            s3_profile: None,
                                            attic_token: None,
                                            attic_cache_name: if cache_type == "Attic" { Some(attic_cache_name) } else { None },
                                            attic_ignore_upstream_cache_filter: Some(true),
                                            attic_jobs: Some(5),
                                            parallel_uploads: Some(1),
                                            max_retries: Some(3),
                                            retry_delay_seconds: Some(5),
                                            push_timeout_seconds: Some(3600),
                                            force_repush: Some(false),
                                            require_sigs: Some(true),
                                        };

                                        match client::create_cache_destination(&req).await {
                                            Ok(_) => {
                                                show_add_modal.set(false);
                                                add_name.set(String::new());
                                                add_push_to.set(String::new());
                                                add_attic_cache_name.set(String::new());
                                                refresh_nonce.set(refresh_nonce() + 1);
                                            }
                                            Err(e) => {
                                                add_error.set(Some(format!("Failed to create destination: {e}")));
                                            }
                                        }
                                        add_submitting.set(false);
                                    });
                                },
                                if add_submitting() { "Creating..." } else { "Create Destination" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Individual cache destination card
#[component]
fn CacheDestinationCard(destination: CacheDestination, on_change: EventHandler<()>) -> Element {
    let enabled_badge_class = if destination.enabled {
        format!("{} bg-emerald-500/10 text-emerald-400 border-emerald-500/30", theme::presets::BADGE)
    } else {
        format!("{} bg-gray-500/10 text-gray-400 border-gray-500/30", theme::presets::BADGE)
    };
    
    let type_badge_class = format!("{} bg-blue-500/10 text-blue-400 border-blue-500/30", theme::presets::BADGE);
    
    let last_used_str = destination.last_used_at.map(|d| d.format("%Y-%m-%d %H:%M").to_string());
    let created_str = destination.created_at.format("%Y-%m-%d").to_string();
    
    let mut show_delete_confirm = use_signal(|| false);
    let mut show_edit_modal = use_signal(|| false);
    let mut edit_name = use_signal(|| destination.name.clone());
    let mut edit_type = use_signal(|| destination.cache_type.clone());
    let mut edit_push_to = use_signal(|| destination.push_to.clone().unwrap_or_default());
    let mut edit_attic_cache_name =
        use_signal(|| destination.attic_cache_name.clone().unwrap_or_default());
    let mut edit_error = use_signal(|| None::<String>);
    let mut edit_submitting = use_signal(|| false);

    rsx! {
        div {
            class: "{theme::presets::CARD}",
            
            div {
                class: "flex justify-between items-start",
                
                div {
                    class: "flex-1",
                    div {
                        class: "flex items-center gap-3 mb-2",
                        h3 {
                            class: "{theme::typography::SECTION_TITLE} {theme::text::PRIMARY}",
                            "{destination.name}"
                        }
                        span {
                            class: "{enabled_badge_class} border",
                            if destination.enabled { "Enabled" } else { "Disabled" }
                        }
                        span {
                            class: "{type_badge_class} border",
                            "{destination.cache_type}"
                        }
                    }
                    
                    if let Some(ref url) = destination.push_to {
                        p {
                            class: "text-sm {theme::text::SECONDARY} mb-3",
                            "→ {url}"
                        }
                    }
                    
                    div {
                        class: "flex gap-4 text-xs {theme::text::MUTED}",
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
                        class: "px-3 py-1 text-sm rounded-lg {theme::interactive::GHOST_BTN} {theme::text::SECONDARY}",
                        onclick: move |_| {
                            show_edit_modal.set(true);
                        },
                        "Edit"
                    }
                    button {
                        class: "px-3 py-1 text-sm rounded-lg {theme::interactive::DANGER_BTN}",
                        onclick: move |_| {
                            show_delete_confirm.set(true);
                        },
                        "Delete"
                    }
                }
            }

            if show_edit_modal() {
                div {
                    class: "fixed inset-0 bg-black/50 flex items-center justify-center z-50",
                    onclick: move |_| show_edit_modal.set(false),
                    div {
                        class: "{theme::presets::CARD} max-w-2xl w-full mx-4",
                        onclick: move |e| e.stop_propagation(),

                        div {
                            class: "flex justify-between items-center mb-6",
                            h3 { class: "{theme::typography::SECTION_TITLE} {theme::text::PRIMARY}", "Edit Cache Destination" }
                            button {
                                class: "{theme::text::SECONDARY} hover:{theme::text::PRIMARY}",
                                onclick: move |_| show_edit_modal.set(false),
                                "✕"
                            }
                        }

                        div {
                            class: "space-y-4",
                            div {
                                label { class: "block text-sm {theme::text::SECONDARY} mb-1", "Name" }
                                input {
                                    class: "w-full px-3 py-2 rounded-lg text-sm {theme::interactive::INPUT} {theme::text::PRIMARY}",
                                    value: edit_name(),
                                    oninput: move |evt| edit_name.set(evt.value()),
                                }
                            }

                            div {
                                label { class: "block text-sm {theme::text::SECONDARY} mb-1", "Type" }
                                select {
                                    class: "w-full px-3 py-2 rounded-lg text-sm {theme::interactive::INPUT} {theme::text::PRIMARY}",
                                    value: edit_type(),
                                    onchange: move |evt| edit_type.set(evt.value()),
                                    option { class: "text-slate-900 bg-white", value: "Nix", "Nix" }
                                    option { class: "text-slate-900 bg-white", value: "Http", "Http" }
                                    option { class: "text-slate-900 bg-white", value: "S3", "S3" }
                                    option { class: "text-slate-900 bg-white", value: "Attic", "Attic" }
                                }
                            }

                            if edit_type() == "Attic" {
                                div {
                                    label { class: "block text-sm {theme::text::SECONDARY} mb-1", "Attic Cache Name" }
                                    input {
                                        class: "w-full px-3 py-2 rounded-lg text-sm {theme::interactive::INPUT} {theme::text::PRIMARY}",
                                        value: edit_attic_cache_name(),
                                        oninput: move |evt| edit_attic_cache_name.set(evt.value()),
                                    }
                                }
                            } else {
                                div {
                                    label { class: "block text-sm {theme::text::SECONDARY} mb-1", "Destination URL" }
                                    input {
                                        class: "w-full px-3 py-2 rounded-lg text-sm {theme::interactive::INPUT} {theme::text::PRIMARY}",
                                        value: edit_push_to(),
                                        oninput: move |evt| edit_push_to.set(evt.value()),
                                    }
                                }
                            }

                            if let Some(err) = edit_error() {
                                p { class: "text-sm text-red-400", "{err}" }
                            }
                        }

                        div {
                            class: "mt-6 flex justify-end gap-3",
                            button {
                                class: "px-4 py-2 rounded-lg text-sm font-medium {theme::interactive::GHOST_BTN} {theme::text::SECONDARY}",
                                onclick: move |_| {
                                    show_edit_modal.set(false);
                                    edit_error.set(None);
                                },
                                "Cancel"
                            }
                            button {
                                class: "px-4 py-2 rounded-lg text-sm font-medium {theme::interactive::PRIMARY_BTN}",
                                disabled: edit_submitting(),
                                onclick: move |_| {
                                    let name = edit_name().trim().to_string();
                                    let cache_type = edit_type();
                                    let push_to = edit_push_to().trim().to_string();
                                    let attic_cache_name = edit_attic_cache_name().trim().to_string();

                                    if name.is_empty() {
                                        edit_error.set(Some("Name is required".to_string()));
                                        return;
                                    }
                                    if cache_type == "Attic" && attic_cache_name.is_empty() {
                                        edit_error.set(Some("Attic cache name is required".to_string()));
                                        return;
                                    }
                                    if cache_type != "Attic" && push_to.is_empty() {
                                        edit_error.set(Some("Destination URL is required".to_string()));
                                        return;
                                    }

                                    edit_submitting.set(true);
                                    edit_error.set(None);
                                    let on_change = on_change.clone();

                                    spawn(async move {
                                        let req = UpdateCacheDestination {
                                            name: Some(name),
                                            cache_type: Some(cache_type.clone()),
                                            push_to: if cache_type == "Attic" { None } else { Some(push_to) },
                                            enabled: None,
                                            signing_key_path: None,
                                            compression: None,
                                            s3_region: None,
                                            s3_profile: None,
                                            attic_token: None,
                                            attic_cache_name: if cache_type == "Attic" {
                                                Some(attic_cache_name)
                                            } else {
                                                None
                                            },
                                            attic_ignore_upstream_cache_filter: None,
                                            attic_jobs: None,
                                            parallel_uploads: None,
                                            max_retries: None,
                                            retry_delay_seconds: None,
                                            push_timeout_seconds: None,
                                            force_repush: None,
                                            require_sigs: None,
                                        };

                                        match client::update_cache_destination(destination.id, &req).await {
                                            Ok(_) => {
                                                show_edit_modal.set(false);
                                                on_change.call(());
                                            }
                                            Err(e) => {
                                                edit_error.set(Some(format!("Failed to update destination: {e}")));
                                            }
                                        }
                                        edit_submitting.set(false);
                                    });
                                },
                                if edit_submitting() { "Saving..." } else { "Save Changes" }
                            }
                        }
                    }
                }
            }
            
            // Delete confirmation modal
            if show_delete_confirm() {
                div {
                    class: "fixed inset-0 bg-black/50 flex items-center justify-center z-50",
                    onclick: move |_| show_delete_confirm.set(false),
                    div {
                        class: "{theme::presets::CARD} max-w-md w-full mx-4",
                        onclick: move |e| e.stop_propagation(),
                        
                        h3 { class: "{theme::typography::SECTION_TITLE} {theme::text::PRIMARY} mb-4", "Delete Cache Destination?" }
                        p { class: "{theme::text::SECONDARY} mb-6", "Are you sure you want to delete \"{destination.name}\"? This action cannot be undone." }
                        
                        div {
                            class: "flex gap-3 justify-end",
                            button {
                                class: "px-4 py-2 rounded-lg text-sm font-medium {theme::interactive::GHOST_BTN} {theme::text::SECONDARY}",
                                onclick: move |_| show_delete_confirm.set(false),
                                "Cancel"
                            }
                            button {
                                class: "px-4 py-2 rounded-lg text-sm font-medium {theme::interactive::DANGER_BTN}",
                                onclick: move |_| {
                                    let dest_id = destination.id;
                                    let on_change = on_change.clone();
                                    spawn(async move {
                                        if client::delete_cache_destination(dest_id).await.is_ok() {
                                            on_change.call(());
                                        }
                                    });
                                    show_delete_confirm.set(false);
                                },
                                "Delete"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// List of cache push jobs with filtering
#[component]
fn CachePushJobsList() -> Element {
    let mut status_filter = use_signal(|| None::<String>);
    
    let jobs = use_resource(move || {
        let filter = status_filter();
        async move {
            client::fetch_cache_push_jobs(filter.as_deref(), 100, 0).await
        }
    });

    rsx! {
        div {
            class: "space-y-4",

            // Header with filter
            div {
                class: "flex justify-between items-center",
                h2 {
                    class: "{theme::typography::SECTION_TITLE} {theme::text::PRIMARY}",
                    "Cache Push Jobs"
                }
                
                select {
                    class: "px-3 py-2 rounded-lg text-sm {theme::interactive::INPUT} {theme::text::PRIMARY}",
                    onchange: move |evt| {
                        let value = evt.value();
                        status_filter.set(if value.is_empty() { None } else { Some(value) });
                    },
                    option { class: "text-slate-900 bg-white", value: "", selected: status_filter().is_none(), "All Statuses" }
                    option { class: "text-slate-900 bg-white", value: "pending", selected: status_filter() == Some("pending".to_string()), "Pending" }
                    option { class: "text-slate-900 bg-white", value: "in_progress", selected: status_filter() == Some("in_progress".to_string()), "In Progress" }
                    option { class: "text-slate-900 bg-white", value: "failed", selected: status_filter() == Some("failed".to_string()), "Failed" }
                    option { class: "text-slate-900 bg-white", value: "completed", selected: status_filter() == Some("completed".to_string()), "Completed" }
                    option { class: "text-slate-900 bg-white", value: "cancelled", selected: status_filter() == Some("cancelled".to_string()), "Cancelled" }
                    option { class: "text-slate-900 bg-white", value: "permanently_failed", selected: status_filter() == Some("permanently_failed".to_string()), "Permanently Failed" }
                }
            }

            // Job list
            match &*jobs.read_unchecked() {
                Some(Ok(job_list)) => rsx! {
                    if job_list.is_empty() {
                        div {
                            class: "{theme::presets::CARD} text-center py-12",
                            p { class: "{theme::text::SECONDARY}", "No cache push jobs found." }
                        }
                    } else {
                        div {
                            class: "{theme::presets::TABLE_CONTAINER}",
                            table {
                                class: "w-full",
                                thead {
                                    class: "{theme::surface::SUBTLE_BG}",
                                    tr {
                                        th { class: "{theme::spacing::TABLE_CELL} text-left {theme::typography::TABLE_HEADER}", "ID" }
                                        th { class: "{theme::spacing::TABLE_CELL} text-left {theme::typography::TABLE_HEADER}", "Status" }
                                        th { class: "{theme::spacing::TABLE_CELL} text-left {theme::typography::TABLE_HEADER}", "Destination" }
                                        th { class: "{theme::spacing::TABLE_CELL} text-left {theme::typography::TABLE_HEADER}", "Attempts" }
                                        th { class: "{theme::spacing::TABLE_CELL} text-left {theme::typography::TABLE_HEADER}", "Scheduled" }
                                        th { class: "{theme::spacing::TABLE_CELL} text-left {theme::typography::TABLE_HEADER}", "Actions" }
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
                        class: "{theme::presets::CARD} border-red-500/30 bg-red-500/5",
                        p { class: "text-red-400", "Error loading push jobs: {e}" }
                    }
                },
                None => rsx! {
                    div {
                        class: "{theme::presets::CARD} text-center py-12",
                        p { class: "{theme::text::SECONDARY}", "Loading jobs..." }
                    }
                },
            }
        }
    }
}

/// Individual job row in the table
#[component]
fn CachePushJobRow(job: CachePushJob) -> Element {
    let (status_text, status_badge_class) = match job.status.as_str() {
        "completed" => ("Completed", format!("{} bg-emerald-500/10 text-emerald-400 border-emerald-500/30", theme::presets::BADGE)),
        "failed" | "permanently_failed" => ("Failed", format!("{} bg-red-500/10 text-red-400 border-red-500/30", theme::presets::BADGE)),
        "in_progress" => ("In Progress", format!("{} bg-blue-500/10 text-blue-400 border-blue-500/30", theme::presets::BADGE)),
        "pending" => ("Pending", format!("{} bg-yellow-500/10 text-yellow-400 border-yellow-500/30", theme::presets::BADGE)),
        "cancelled" => ("Cancelled", format!("{} bg-gray-500/10 text-gray-400 border-gray-500/30", theme::presets::BADGE)),
        _ => (&*job.status, format!("{} bg-gray-500/10 text-gray-400 border-gray-500/30", theme::presets::BADGE)),
    };
    
    let scheduled_str = job.scheduled_at.format("%Y-%m-%d %H:%M").to_string();

    rsx! {
        tr {
            class: "border-t {theme::surface::DIVIDER} hover:{theme::surface::SUBTLE_BG}",
            td { class: "{theme::spacing::TABLE_CELL} text-sm {theme::text::PRIMARY}", "{job.id}" }
            td { 
                class: "{theme::spacing::TABLE_CELL}",
                span { class: "{status_badge_class} border", "{status_text}" }
            }
            td {
                class: "{theme::spacing::TABLE_CELL} text-sm {theme::text::SECONDARY}",
                if let Some(ref dest) = job.cache_destination {
                    "{dest}"
                } else {
                    span { class: "{theme::text::MUTED}", "(default)" }
                }
            }
            td { class: "{theme::spacing::TABLE_CELL} text-sm {theme::text::SECONDARY}", "{job.attempts}" }
            td {
                class: "{theme::spacing::TABLE_CELL} text-sm {theme::text::MUTED}",
                "{scheduled_str}"
            }
            td {
                class: "{theme::spacing::TABLE_CELL}",
                div {
                    class: "flex gap-2",
                    if job.status == "failed" || job.status == "permanently_failed" {
                        button {
                            class: "px-2 py-1 text-xs rounded {theme::interactive::PRIMARY_BTN}",
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
                    if job.status == "pending" || job.status == "in_progress" {
                        button {
                            class: "px-2 py-1 text-xs rounded {theme::interactive::DANGER_BTN}",
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
