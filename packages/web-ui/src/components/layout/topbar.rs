//! Top bar layout component.

use crate::api::client::{
    dismiss_user_notification, fetch_user_notifications, mark_all_user_notifications_read,
    mark_user_notification_read,
};
use crate::api::models::{NotificationCategory, UpdateUserPreferences, UserNotificationDto};
use crate::components::layout::sidebar::{PreferencesContext, SidebarContext};
use crate::routes::Route;
use crate::state::app_state::AppState;
use crate::state::preferences;
use crate::state::theme::UiTheme;
use crate::theme;
use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use std::collections::{HashMap, HashSet};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;

const NOTIFICATION_PAGE_SIZE: i64 = 50;
const MAX_NOTIFICATION_RECONCILIATION_PAGES: usize = 4;
const MAX_NOTIFICATION_RECONCILIATION_ROWS: usize =
    NOTIFICATION_PAGE_SIZE as usize * MAX_NOTIFICATION_RECONCILIATION_PAGES;

/// Shares the account-scoped notification feed with shell components.
#[derive(Clone, Copy)]
pub struct AccountNotificationsContext {
    /// Holds the feed owned by the current authenticated account generation.
    pub(crate) feed: Signal<NotificationFeed>,
}

/// Identifies one authenticated account generation for response fencing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NotificationOwner {
    user_id: String,
    auth_generation: u64,
}

impl NotificationOwner {
    /// Creates an owner for one authenticated user and auth generation.
    pub(crate) fn new(user_id: String, auth_generation: u64) -> Self {
        Self {
            user_id,
            auth_generation,
        }
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
enum NotificationRequestKind {
    Head,
    Append(String),
    Reconcile(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NotificationRequest {
    owner: NotificationOwner,
    generation: u64,
    mutation_epoch: u64,
    kind: NotificationRequestKind,
    range_boundary: Option<(DateTime<Utc>, uuid::Uuid)>,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
enum NotificationMutation {
    Read(uuid::Uuid),
    Dismiss(uuid::Uuid),
    MarkAll,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NotificationMutationAttempt {
    owner: NotificationOwner,
    generation: u64,
    operation: NotificationMutation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NotificationFailure<T> {
    operation: T,
    message: String,
    mutation_epoch: Option<u64>,
}

#[derive(Clone, Debug)]
struct NotificationReconciliation {
    mutation_epoch: u64,
    boundary: (DateTime<Utc>, uuid::Uuid),
    collected: Vec<UserNotificationDto>,
    unread_count: i64,
    pages: usize,
}

/// Coordinates account notification requests, mutations, and pagination.
#[derive(Clone, Debug, Default)]
pub(crate) struct NotificationFeed {
    owner: Option<NotificationOwner>,
    request_generation: u64,
    active_request: Option<NotificationRequest>,
    mutation_epoch: u64,
    mutation_generations: HashMap<NotificationMutation, u64>,
    pending_mutations: HashSet<NotificationMutation>,
    items: Vec<UserNotificationDto>,
    next_cursor: Option<String>,
    reconciliation: Option<NotificationReconciliation>,
    unread_count: i64,
    loading: bool,
    loading_more: bool,
    load_failure: Option<NotificationFailure<NotificationRequestKind>>,
    mutation_failures: Vec<NotificationFailure<NotificationMutation>>,
    pagination_notice: Option<String>,
    queued_refresh: bool,
}

impl NotificationFeed {
    /// Clears the feed and assigns its authenticated owner.
    ///
    /// CONCURRENCY: Every response and mutation is fenced by this owner. Reset
    /// removes all prior-account rows, cursors, tombstones, and queued work.
    pub(crate) fn reset(&mut self, owner: Option<NotificationOwner>) {
        self.request_generation = self.request_generation.saturating_add(1);
        *self = Self {
            owner,
            request_generation: self.request_generation,
            ..Self::default()
        };
    }

    fn start_head(
        &mut self,
        owner: &NotificationOwner,
        queue_if_busy: bool,
    ) -> Option<NotificationRequest> {
        if self.owner.as_ref() != Some(owner) {
            return None;
        }
        if self.active_request.is_some() {
            self.queued_refresh |= queue_if_busy;
            return None;
        }
        self.reconciliation = None;
        self.start_request(owner, NotificationRequestKind::Head)
    }

    fn start_append(&mut self, owner: &NotificationOwner) -> Option<NotificationRequest> {
        if self.owner.as_ref() != Some(owner)
            || self.active_request.is_some()
            || self.reconciliation.is_some()
        {
            return None;
        }
        let cursor = self.next_cursor.clone()?;
        self.start_request(owner, NotificationRequestKind::Append(cursor))
    }

    fn start_request(
        &mut self,
        owner: &NotificationOwner,
        kind: NotificationRequestKind,
    ) -> Option<NotificationRequest> {
        self.start_request_at_epoch(owner, kind, self.mutation_epoch)
    }

    fn start_request_at_epoch(
        &mut self,
        owner: &NotificationOwner,
        kind: NotificationRequestKind,
        mutation_epoch: u64,
    ) -> Option<NotificationRequest> {
        self.request_generation = self.request_generation.saturating_add(1);
        let request = NotificationRequest {
            owner: owner.clone(),
            generation: self.request_generation,
            mutation_epoch,
            range_boundary: matches!(kind, NotificationRequestKind::Head)
                .then(|| self.items.last().map(notification_key))
                .flatten(),
            kind,
        };
        self.loading = matches!(
            request.kind,
            NotificationRequestKind::Head | NotificationRequestKind::Reconcile(_)
        );
        self.loading_more = matches!(request.kind, NotificationRequestKind::Append(_));
        self.active_request = Some(request.clone());
        Some(request)
    }

    fn retry_request(
        &mut self,
        owner: &NotificationOwner,
        kind: NotificationRequestKind,
        originating_mutation_epoch: u64,
    ) -> Option<NotificationRequest> {
        if self.owner.as_ref() != Some(owner) || self.active_request.is_some() {
            return None;
        }
        if originating_mutation_epoch != self.mutation_epoch {
            self.reconciliation = None;
            return self.start_head(owner, false);
        }
        if matches!(kind, NotificationRequestKind::Head) {
            self.reconciliation = None;
        }
        self.start_request_at_epoch(owner, kind, originating_mutation_epoch)
    }

    fn request_is_current(&self, request: &NotificationRequest) -> bool {
        self.owner.as_ref() == Some(&request.owner) && self.active_request.as_ref() == Some(request)
    }

    // INVARIANT: A GET can update rows and unread count only when no mutation
    // completed after that GET started.
    fn finish_success(
        &mut self,
        request: &NotificationRequest,
        response: crate::api::models::UserNotificationsResponse,
    ) -> Option<NotificationRequest> {
        if !self.request_is_current(request) {
            return None;
        }
        // CONCURRENCY: A mutation makes every part of an older GET snapshot
        // stale. Discard its rows and count together, then fetch one new head.
        if self.mutation_epoch != request.mutation_epoch {
            self.finish_request();
            self.reconciliation = None;
            self.queued_refresh = false;
            return self.start_head(&request.owner, false);
        }
        let next_request = match &request.kind {
            NotificationRequestKind::Head => {
                if let Some(boundary) = request.range_boundary {
                    self.start_reconciliation(boundary, response, request.mutation_epoch)
                } else {
                    self.items = deduplicate_notifications(response.notifications);
                    self.next_cursor = response.next_cursor;
                    self.unread_count = response.unread_count.max(0);
                    self.load_failure = None;
                    None
                }
            }
            NotificationRequestKind::Append(cursor) => {
                self.items.extend(response.notifications);
                self.items = deduplicate_notifications(self.items.drain(..).collect());
                self.next_cursor = response.next_cursor;
                self.unread_count = response.unread_count.max(0);
                if self.load_failure.as_ref().is_some_and(|failure| {
                    failure.operation == NotificationRequestKind::Append(cursor.clone())
                }) {
                    self.load_failure = None;
                }
                None
            }
            NotificationRequestKind::Reconcile(cursor) => {
                self.continue_reconciliation(request, cursor, response)
            }
        };
        self.finish_request();

        if let Some((kind, mutation_epoch)) = next_request {
            return self.start_request_at_epoch(&request.owner, kind, mutation_epoch);
        }
        if std::mem::take(&mut self.queued_refresh) {
            return self.start_head(&request.owner, false);
        }
        None
    }

    fn finish_error(
        &mut self,
        request: &NotificationRequest,
        message: String,
    ) -> Option<NotificationRequest> {
        if !self.request_is_current(request) {
            return None;
        }
        if self.mutation_epoch != request.mutation_epoch {
            self.finish_request();
            self.reconciliation = None;
            self.queued_refresh = false;
            return self.start_head(&request.owner, false);
        }
        self.load_failure = Some(NotificationFailure {
            operation: request.kind.clone(),
            message,
            mutation_epoch: Some(request.mutation_epoch),
        });
        self.finish_request();
        if std::mem::take(&mut self.queued_refresh) {
            return self.start_head(&request.owner, false);
        }
        None
    }

    fn finish_request(&mut self) {
        self.active_request = None;
        self.loading = false;
        self.loading_more = false;
    }

    // INVARIANT: The fetched sequence is the sole authority for every row
    // through the oldest key that was displayed when the refresh started.
    fn finish_reconciliation(
        &mut self,
        reconciliation: NotificationReconciliation,
        next_cursor: Option<String>,
        bounded_reset: bool,
    ) {
        self.items = reconciliation.collected;
        self.unread_count = reconciliation.unread_count.max(0);
        self.next_cursor = next_cursor;
        self.reconciliation = None;
        self.pagination_notice = bounded_reset.then(|| {
            "Notification history changed substantially. Older loaded history was reset to the bounded refreshed range; use Load more to continue."
                .to_string()
        });
        // INVARIANT: This cursor belongs to the newly authoritative range. A
        // prior append retry cannot be valid against it, even when the refresh
        // reached the old row boundary before it traversed that failed cursor.
        self.load_failure = None;
    }

    fn start_reconciliation(
        &mut self,
        boundary: (DateTime<Utc>, uuid::Uuid),
        response: crate::api::models::UserNotificationsResponse,
        mutation_epoch: u64,
    ) -> Option<(NotificationRequestKind, u64)> {
        let reconciliation = NotificationReconciliation {
            mutation_epoch,
            boundary,
            collected: deduplicate_notifications(response.notifications),
            unread_count: response.unread_count,
            pages: 1,
        };
        if reconciliation_reached_boundary(&reconciliation) {
            self.finish_reconciliation(reconciliation, response.next_cursor, false);
            return None;
        }
        self.advance_or_reset_reconciliation(reconciliation, response.next_cursor)
    }

    fn continue_reconciliation(
        &mut self,
        request: &NotificationRequest,
        _cursor: &str,
        response: crate::api::models::UserNotificationsResponse,
    ) -> Option<(NotificationRequestKind, u64)> {
        let Some(mut reconciliation) = self.reconciliation.take() else {
            return Some((NotificationRequestKind::Head, self.mutation_epoch));
        };
        // CONCURRENCY: A continuation can consume only rows collected under
        // its originating mutation epoch. Never attach a fresh epoch to them.
        if reconciliation.mutation_epoch != request.mutation_epoch
            || reconciliation.mutation_epoch != self.mutation_epoch
        {
            return Some((NotificationRequestKind::Head, self.mutation_epoch));
        }
        reconciliation.collected.extend(response.notifications);
        reconciliation.collected =
            deduplicate_notifications(reconciliation.collected.drain(..).collect());
        reconciliation.unread_count = response.unread_count;
        reconciliation.pages += 1;
        if reconciliation_reached_boundary(&reconciliation) {
            self.finish_reconciliation(reconciliation, response.next_cursor, false);
            return None;
        }
        self.advance_or_reset_reconciliation(reconciliation, response.next_cursor)
    }

    fn advance_or_reset_reconciliation(
        &mut self,
        reconciliation: NotificationReconciliation,
        next_cursor: Option<String>,
    ) -> Option<(NotificationRequestKind, u64)> {
        if let Some(cursor) = next_cursor.as_ref()
            && reconciliation.pages < MAX_NOTIFICATION_RECONCILIATION_PAGES
            && reconciliation.collected.len() < MAX_NOTIFICATION_RECONCILIATION_ROWS
        {
            let mutation_epoch = reconciliation.mutation_epoch;
            self.reconciliation = Some(reconciliation);
            return Some((
                NotificationRequestKind::Reconcile(cursor.clone()),
                mutation_epoch,
            ));
        }

        let bounded_reset = next_cursor.is_some();
        self.finish_reconciliation(reconciliation, next_cursor, bounded_reset);
        None
    }

    fn start_mutation(
        &mut self,
        owner: &NotificationOwner,
        operation: NotificationMutation,
    ) -> Option<NotificationMutationAttempt> {
        if self.owner.as_ref() != Some(owner) {
            return None;
        }
        // CONCURRENCY: One durable operation identity can own at most one
        // in-flight request. Independent row operations remain concurrent.
        if !self.pending_mutations.insert(operation.clone()) {
            return None;
        }
        let generation = self
            .mutation_generations
            .entry(operation.clone())
            .and_modify(|generation| *generation = generation.saturating_add(1))
            .or_insert(1);
        Some(NotificationMutationAttempt {
            owner: owner.clone(),
            generation: *generation,
            operation,
        })
    }

    fn mutation_is_current(&self, attempt: &NotificationMutationAttempt) -> bool {
        self.owner.as_ref() == Some(&attempt.owner)
            && self.mutation_generations.get(&attempt.operation) == Some(&attempt.generation)
            && self.pending_mutations.contains(&attempt.operation)
    }

    fn mutation_pending(&self, operation: &NotificationMutation) -> bool {
        self.pending_mutations.contains(operation)
    }

    fn finish_mutation(&mut self, attempt: &NotificationMutationAttempt) {
        self.pending_mutations.remove(&attempt.operation);
    }

    fn clear_mutation_failure(&mut self, operation: &NotificationMutation) {
        self.mutation_failures
            .retain(|failure| &failure.operation != operation);
    }

    // CONCURRENCY: A successful mutation invalidates every GET snapshot and
    // retry captured before the mutation. A new head request is the only safe
    // source for rebuilding the visible range.
    fn mutation_succeeded(&mut self, owner: &NotificationOwner) -> Option<NotificationRequest> {
        self.mutation_epoch = self.mutation_epoch.saturating_add(1);
        self.reconciliation = None;
        if self.load_failure.as_ref().is_some_and(|failure| {
            matches!(failure.operation, NotificationRequestKind::Reconcile(_))
                && failure
                    .mutation_epoch
                    .is_some_and(|epoch| epoch < self.mutation_epoch)
        }) {
            self.load_failure = None;
        }
        if self.active_request.is_some() {
            self.queued_refresh = true;
            None
        } else {
            self.queued_refresh = false;
            self.start_head(owner, false)
        }
    }

    fn read_succeeded(
        &mut self,
        attempt: &NotificationMutationAttempt,
        id: uuid::Uuid,
    ) -> Option<NotificationRequest> {
        if !self.mutation_is_current(attempt) {
            return None;
        }
        self.finish_mutation(attempt);
        if let Some(item) = self.items.iter_mut().find(|item| item.id == id)
            && item.read_at.is_none()
        {
            item.read_at = Some(Utc::now());
            self.unread_count = (self.unread_count - 1).max(0);
        }
        self.clear_mutation_failure(&attempt.operation);
        self.mutation_succeeded(&attempt.owner)
    }

    fn dismiss_succeeded(
        &mut self,
        attempt: &NotificationMutationAttempt,
        id: uuid::Uuid,
    ) -> (bool, Option<NotificationRequest>) {
        if !self.mutation_is_current(attempt) {
            return (false, None);
        }
        self.finish_mutation(attempt);
        let was_unread = self
            .items
            .iter()
            .find(|item| item.id == id)
            .is_some_and(|item| item.read_at.is_none());
        self.items.retain(|item| item.id != id);
        if was_unread {
            self.unread_count = (self.unread_count - 1).max(0);
        }
        self.clear_mutation_failure(&attempt.operation);
        (true, self.mutation_succeeded(&attempt.owner))
    }

    fn mark_all_succeeded(
        &mut self,
        attempt: &NotificationMutationAttempt,
    ) -> Option<NotificationRequest> {
        if !self.mutation_is_current(attempt) {
            return None;
        }
        self.finish_mutation(attempt);
        for item in &mut self.items {
            item.read_at.get_or_insert_with(Utc::now);
        }
        self.unread_count = 0;
        self.clear_mutation_failure(&attempt.operation);
        self.mutation_succeeded(&attempt.owner)
    }

    fn mutation_failed(&mut self, attempt: &NotificationMutationAttempt, message: String) {
        if self.mutation_is_current(attempt) {
            self.finish_mutation(attempt);
            self.clear_mutation_failure(&attempt.operation);
            self.mutation_failures.push(NotificationFailure {
                operation: attempt.operation.clone(),
                message,
                mutation_epoch: None,
            });
        }
    }
}

fn notification_key(item: &UserNotificationDto) -> (DateTime<Utc>, uuid::Uuid) {
    (item.created_at, item.id)
}

fn reconciliation_reached_boundary(reconciliation: &NotificationReconciliation) -> bool {
    reconciliation
        .collected
        .last()
        .is_some_and(|item| notification_key(item) <= reconciliation.boundary)
}

fn deduplicate_notifications(mut items: Vec<UserNotificationDto>) -> Vec<UserNotificationDto> {
    items.sort_by(|left, right| {
        right
            .created_at
            .cmp(&left.created_at)
            .then_with(|| right.id.cmp(&left.id))
    });
    let mut seen = HashSet::new();
    items.retain(|item| seen.insert(item.id));
    items
}

fn bounded_badge(count: i64) -> String {
    if count > 99 {
        "99+".to_string()
    } else {
        count.max(0).to_string()
    }
}

#[derive(Clone, Copy)]
enum NotificationKind {
    Deploy,
    Build,
    Shield,
    Warning,
    Evaluation,
}

#[derive(Clone, Debug, PartialEq)]
enum NotificationTarget {
    Route(Route),
    SystemDeploy(String),
}

fn set_root_attr(name: &str, value: &str) {
    if let Some(document) = web_sys::window().and_then(|w| w.document()) {
        if let Some(root) = document.document_element() {
            let _ = root.set_attribute(name, value);
        }
    }
}

fn focus_topbar_bell() {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(document) = web_sys::window().and_then(|w| w.document()) {
            if let Some(element) = document
                .query_selector("[data-testid='topbar-notifications-button']")
                .ok()
                .flatten()
            {
                if let Some(html_element) = element.dyn_ref::<web_sys::HtmlElement>() {
                    let _ = html_element.focus();
                }
            }
        }
    }
}

fn focus_notification_menu() {
    #[cfg(target_arch = "wasm32")]
    if let Some(element) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| {
            document
                .query_selector("[data-testid='topbar-notifications-panel']")
                .ok()
                .flatten()
        })
        .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
    {
        let _ = element.focus();
    }
}

fn focus_notification_settings() {
    #[cfg(target_arch = "wasm32")]
    if let Some(element) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| {
            document
                .query_selector("[data-testid='topbar-notifications-settings-button']")
                .ok()
                .flatten()
        })
        .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
    {
        let _ = element.focus();
    }
}

fn trap_notification_tab(shift: bool) -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        let Some(document) = web_sys::window().and_then(|window| window.document()) else {
            return false;
        };
        let Ok(nodes) = document.query_selector_all(
            "[data-testid='topbar-notifications-panel'] button:not([disabled]), [data-testid='topbar-notifications-panel'] [tabindex='0']",
        ) else {
            return false;
        };
        let focusable = (0..nodes.length())
            .filter_map(|index| nodes.item(index))
            .filter_map(|node| node.dyn_into::<web_sys::HtmlElement>().ok())
            .collect::<Vec<_>>();
        let Some(first) = focusable.first() else {
            return false;
        };
        let Some(last) = focusable.last() else {
            return false;
        };
        let active = document.active_element();
        let at_first = active.as_ref() == Some(first.as_ref());
        let at_last = active.as_ref() == Some(last.as_ref());
        let panel_focused = notification_panel_has_focus();
        if shift && (at_first || panel_focused) {
            let _ = last.focus();
            return true;
        }
        if !shift && at_last {
            let _ = first.focus();
            return true;
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    let _ = shift;
    false
}

fn focus_notification_item(current_id: Option<uuid::Uuid>, direction: isize) {
    #[cfg(target_arch = "wasm32")]
    {
        let Some(document) = web_sys::window().and_then(|window| window.document()) else {
            return;
        };
        let Ok(nodes) = document.query_selector_all("[data-notification-id]") else {
            return;
        };
        let items = (0..nodes.length())
            .filter_map(|index| nodes.item(index))
            .filter_map(|node| node.dyn_into::<web_sys::HtmlElement>().ok())
            .collect::<Vec<_>>();
        if items.is_empty() {
            focus_notification_menu();
            return;
        }
        let current = current_id.and_then(|id| {
            items.iter().position(|item| {
                item.get_attribute("data-notification-id")
                    .is_some_and(|value| value == id.to_string())
            })
        });
        let target = notification_focus_index(items.len(), current, direction);
        if let Some(item) = items.get(target) {
            let _ = item.focus();
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    let _ = (current_id, direction);
}

fn focus_notification_boundary(last: bool) {
    #[cfg(target_arch = "wasm32")]
    {
        let Some(document) = web_sys::window().and_then(|window| window.document()) else {
            return;
        };
        let Ok(nodes) = document.query_selector_all("[data-notification-id]") else {
            return;
        };
        if let Some(element) = notification_boundary_index(nodes.length() as usize, last)
            .and_then(|index| nodes.item(index as u32))
            .and_then(|node| node.dyn_into::<web_sys::HtmlElement>().ok())
        {
            let _ = element.focus();
        } else {
            focus_notification_menu();
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    let _ = last;
}

fn focus_after_notification_dismiss(index: usize, remaining_items: usize) {
    #[cfg(target_arch = "wasm32")]
    {
        if remaining_items == 0 {
            focus_notification_settings();
            return;
        }
        let Some(document) = web_sys::window().and_then(|window| window.document()) else {
            return;
        };
        let Ok(nodes) = document.query_selector_all("[data-notification-id]") else {
            return;
        };
        let target = index.min(remaining_items - 1);
        if let Some(element) = nodes
            .item(target as u32)
            .and_then(|node| node.dyn_into::<web_sys::HtmlElement>().ok())
        {
            let _ = element.focus();
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    let _ = (index, remaining_items);
}

fn dismiss_focus_is_current(
    applied: bool,
    panel_open: bool,
    expected_owner: &NotificationOwner,
    current_owner: Option<&NotificationOwner>,
) -> bool {
    applied && panel_open && current_owner == Some(expected_owner)
}

fn notification_panel_has_focus() -> bool {
    #[cfg(target_arch = "wasm32")]
    return web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.active_element())
        .is_some_and(|element| {
            element.get_attribute("data-testid").as_deref() == Some("topbar-notifications-panel")
        });

    #[cfg(not(target_arch = "wasm32"))]
    false
}

fn notification_boundary_index(item_count: usize, last: bool) -> Option<usize> {
    if item_count == 0 {
        None
    } else if last {
        Some(item_count - 1)
    } else {
        Some(0)
    }
}

fn notification_focus_index(item_count: usize, current: Option<usize>, direction: isize) -> usize {
    if item_count == 0 {
        return 0;
    }
    match (current, direction.is_negative()) {
        (Some(0), true) => item_count - 1,
        (Some(index), true) => index - 1,
        (Some(index), false) => (index + 1) % item_count,
        (None, true) => item_count - 1,
        (None, false) => 0,
    }
}

fn close_notifications(mut notifications_open: Signal<bool>) {
    notifications_open.set(false);
    focus_topbar_bell();
}

fn notification_kind(category: NotificationCategory) -> NotificationKind {
    match category {
        NotificationCategory::DeployFailures => NotificationKind::Deploy,
        NotificationCategory::BuildFailures => NotificationKind::Build,
        NotificationCategory::CriticalCves => NotificationKind::Shield,
        NotificationCategory::PolicyViolations => NotificationKind::Evaluation,
        NotificationCategory::HeartbeatLost => NotificationKind::Warning,
    }
}

fn notification_color(category: NotificationCategory) -> &'static str {
    match category {
        NotificationCategory::DeployFailures
        | NotificationCategory::BuildFailures
        | NotificationCategory::CriticalCves => "var(--cf-policy-red)",
        NotificationCategory::PolicyViolations | NotificationCategory::HeartbeatLost => {
            "var(--cf-policy-amber)"
        }
    }
}

fn notification_target(route: &str) -> Option<NotificationTarget> {
    if let Some(poam_id) = route.strip_prefix("/compliance?poam=") {
        if uuid::Uuid::parse_str(poam_id).is_ok() {
            return Some(NotificationTarget::Route(Route::ComplianceView {
                bundle: String::new(),
                version: String::new(),
                system: String::new(),
                policy: String::new(),
                poam: poam_id.to_string(),
                view: String::new(),
            }));
        }
        return None;
    }

    let (path, query) = route.split_once('?').unwrap_or((route, ""));
    let segments = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if let ["systems", id] = segments.as_slice()
        && uuid::Uuid::parse_str(id).is_ok()
        && query
            .split('&')
            .filter_map(|part| part.split_once('='))
            .any(|(key, value)| key == "tab" && value == "deploy")
    {
        return Some(NotificationTarget::SystemDeploy((*id).to_string()));
    }
    match path {
        "/systems" => Some(NotificationTarget::Route(Route::SystemsView {
            query: String::new(),
        })),
        "/builds" => Some(NotificationTarget::Route(Route::BuildsView {})),
        "/cves" => Some(NotificationTarget::Route(Route::CvesView {
            query: String::new(),
        })),
        "/evaluations" => Some(NotificationTarget::Route(Route::EvaluationsView {})),
        "/profile" => Some(NotificationTarget::Route(Route::ProfileView {})),
        _ => None,
    }
}

fn open_system_deploy(id: &str) {
    if let Some(window) = web_sys::window() {
        let _ = window
            .location()
            .set_href(&format!("/systems/{id}?tab=deploy"));
    }
}

fn relative_time(timestamp: DateTime<Utc>) -> String {
    let delta = Utc::now().signed_duration_since(timestamp);
    if delta.num_minutes() < 1 {
        "now".to_string()
    } else if delta.num_hours() < 1 {
        format!("{}m ago", delta.num_minutes())
    } else if delta.num_days() < 1 {
        format!("{}h ago", delta.num_hours())
    } else {
        format!("{}d ago", delta.num_days())
    }
}

fn notification_accessible_label(item: &UserNotificationDto) -> String {
    let state = if item.read_at.is_some() {
        "Read"
    } else {
        "Unread"
    };
    let title = item.title.trim_end_matches('.');
    let summary = item.summary.trim_end_matches('.');
    format!(
        "{state} notification. {title}. {summary}. Received {}.",
        item.created_at.to_rfc3339(),
    )
}

/// Starts one bounded account notification request.
///
/// A bell-open head request queues one follow-up head refresh when another GET
/// is active. Polls and pagination never overlap an active request.
pub(crate) fn load_account_notifications(
    mut ctx: AccountNotificationsContext,
    owner: NotificationOwner,
    append: bool,
    queue_if_busy: bool,
) {
    let request = if append {
        ctx.feed.write().start_append(&owner)
    } else {
        ctx.feed.write().start_head(&owner, queue_if_busy)
    };
    let Some(request) = request else {
        return;
    };
    run_notification_request(ctx, request);
}

fn run_notification_request(
    mut ctx: AccountNotificationsContext,
    mut request: NotificationRequest,
) {
    spawn(async move {
        loop {
            let cursor = match &request.kind {
                NotificationRequestKind::Head => None,
                NotificationRequestKind::Append(cursor)
                | NotificationRequestKind::Reconcile(cursor) => Some(cursor.clone()),
            };
            let next_request =
                match fetch_user_notifications(Some(NOTIFICATION_PAGE_SIZE), cursor, false).await {
                    Ok(response) => ctx.feed.write().finish_success(&request, response),
                    Err(err) => ctx
                        .feed
                        .write()
                        .finish_error(&request, format!("Could not load notifications: {err}")),
                };
            let Some(next_request) = next_request else {
                break;
            };
            request = next_request;
        }
    });
}

fn mark_notification_read(
    mut ctx: AccountNotificationsContext,
    owner: NotificationOwner,
    id: uuid::Uuid,
) {
    let Some(attempt) = ctx
        .feed
        .write()
        .start_mutation(&owner, NotificationMutation::Read(id))
    else {
        return;
    };
    spawn(async move {
        match mark_user_notification_read(id).await {
            Ok(()) => {
                let request = ctx.feed.write().read_succeeded(&attempt, id);
                if let Some(request) = request {
                    run_notification_request(ctx, request);
                }
            }
            Err(err) => ctx
                .feed
                .write()
                .mutation_failed(&attempt, format!("Could not mark notification read: {err}")),
        }
    });
}

fn dismiss_notification(
    mut ctx: AccountNotificationsContext,
    owner: NotificationOwner,
    id: uuid::Uuid,
    notifications_open: Signal<bool>,
) {
    let focus_owner = owner.clone();
    let focus_index = ctx
        .feed
        .read()
        .items
        .iter()
        .position(|item| item.id == id)
        .unwrap_or(0);
    let Some(attempt) = ctx
        .feed
        .write()
        .start_mutation(&owner, NotificationMutation::Dismiss(id))
    else {
        return;
    };
    spawn(async move {
        match dismiss_user_notification(id).await {
            Ok(()) => {
                let (applied, request) = ctx.feed.write().dismiss_succeeded(&attempt, id);
                if let Some(request) = request {
                    run_notification_request(ctx, request);
                }
                TimeoutFuture::new(0).await;
                let feed = ctx.feed.read();
                if dismiss_focus_is_current(
                    applied,
                    notifications_open(),
                    &focus_owner,
                    feed.owner.as_ref(),
                ) {
                    focus_after_notification_dismiss(focus_index, feed.items.len());
                }
            }
            Err(err) => ctx
                .feed
                .write()
                .mutation_failed(&attempt, format!("Could not dismiss notification: {err}")),
        }
    });
}

fn mark_all_notifications_read(mut ctx: AccountNotificationsContext, owner: NotificationOwner) {
    let Some(attempt) = ctx
        .feed
        .write()
        .start_mutation(&owner, NotificationMutation::MarkAll)
    else {
        return;
    };
    spawn(async move {
        match mark_all_user_notifications_read().await {
            Ok(()) => {
                let request = ctx.feed.write().mark_all_succeeded(&attempt);
                if let Some(request) = request {
                    run_notification_request(ctx, request);
                }
            }
            Err(_) => ctx
                .feed
                .write()
                .mutation_failed(&attempt, "Could not mark notifications read".to_string()),
        }
    });
}

fn retry_notification_action(
    ctx: AccountNotificationsContext,
    owner: NotificationOwner,
    retry: NotificationMutation,
    notifications_open: Signal<bool>,
) {
    match retry {
        NotificationMutation::Read(id) => mark_notification_read(ctx, owner, id),
        NotificationMutation::Dismiss(id) => {
            dismiss_notification(ctx, owner, id, notifications_open)
        }
        NotificationMutation::MarkAll => mark_all_notifications_read(ctx, owner),
    }
}

fn retry_notification_load(
    mut ctx: AccountNotificationsContext,
    owner: NotificationOwner,
    operation: NotificationRequestKind,
    mutation_epoch: u64,
) {
    let request = ctx
        .feed
        .write()
        .retry_request(&owner, operation, mutation_epoch);
    let Some(request) = request else {
        return;
    };
    run_notification_request(ctx, request);
}

/// Renders the current page title, account actions, and notification menu.
///
/// Notification state is scoped to the authenticated user and authentication
/// generation. The component discards late responses after account changes and
/// delegates notification persistence and authorization to server APIs.
#[component]
pub fn TopBar(title: String) -> Element {
    let mut ui_theme = use_context::<Signal<UiTheme>>();
    let nav = navigator();
    let current_route = use_route::<Route>();
    let breadcrumb_override = use_context::<Signal<Option<(String, String)>>>();
    let app_state = use_context::<Signal<AppState>>();
    let auth_context = app_state.read().auth.clone();
    let auth_user_id = auth_context
        .as_ref()
        .and_then(|ctx| ctx.user.as_ref())
        .map(|user| user.id.clone());
    let auth_generation = app_state.read().auth_generation;

    let sidebar_ctx = use_context::<SidebarContext>();
    let mut is_mobile_drawer_open = sidebar_ctx.is_mobile_drawer_open;
    let mut is_collapsed = sidebar_ctx.is_collapsed;

    let prefs_ctx = use_context::<PreferencesContext>();
    let mut density = prefs_ctx.density;
    let mut default_view = prefs_ctx.default_systems_view;
    let save_error = prefs_ctx.save_error;

    let mut tweaks_open = use_signal(|| false);
    let mut notifications_open = use_signal(|| false);
    let notification_ctx = use_context::<AccountNotificationsContext>();
    let notification_owner = auth_user_id
        .clone()
        .map(|user_id| NotificationOwner::new(user_id, auth_generation));
    let (crumb_parent, crumb_current) =
        if let Some((parent, current)) = breadcrumb_override.read().clone() {
            (Some(parent), current)
        } else {
            match &current_route {
                Route::SystemDetailView { id, .. } => (Some("Systems".to_string()), id.clone()),
                Route::EvaluationsCommitView { commit_id } => (
                    Some("Evaluations".to_string()),
                    format!("commit {commit_id}"),
                ),
                _ => (None, title.clone()),
            }
        };

    let toggle_drawer = move |_| {
        is_mobile_drawer_open.set(!is_mobile_drawer_open());
    };

    use_effect(move || {
        let _ = js_sys::eval(
            "(() => { \
                const h = document.querySelector('header'); \
                if (h) { \
                    const b = h.getBoundingClientRect().bottom; \
                    if (b > 0) document.documentElement.style.setProperty('--coach-top', b + 'px'); \
                } \
            })()",
        );
    });

    let open_owner = notification_owner.clone();
    let mark_all_owner = notification_owner.clone();
    let load_retry_owner = notification_owner.clone();
    let mutation_retry_owner = notification_owner.clone();
    let more_owner = notification_owner.clone();

    rsx! {
        header {
            class: "topbar",
            button {
                "data-testid": "mobile-nav-toggle",
                class: "cf-mobile-only inline-flex items-center justify-center p-2 rounded-lg border {theme::surface::CARD_BORDER} {theme::interactive::HOVER_BG} {theme::text::SECONDARY} min-h-[44px] min-w-[44px]",
                onclick: toggle_drawer,
                "aria-label": "Open navigation menu",
                svg {
                    class: "w-6 h-6",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    view_box: "0 0 24 24",
                    path { d: "M4 6h16M4 12h16M4 18h16" }
                }
            }

            div {
                class: "breadcrumbs",
                span { "Fleet" }
                span { class: "sep", "/" }
                if let Some(parent) = crumb_parent.clone() {
                    span { "{parent}" }
                    span { class: "sep", "/" }
                }
                span { class: "crumb-current", "{crumb_current}" }
            }

            div {
                class: "topbar-search",
                svg {
                    class: "w-3.5 h-3.5",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    view_box: "0 0 24 24",
                    path {
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        d: "M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
                    }
                }
                input {
                    class: "input focus-ring w-full",
                    r#type: "search",
                    placeholder: "Search systems, flakes, commits…",
                }
                span {
                    class: "kbd",
                    style: "position: absolute; right: 10px; top: 50%; transform: translateY(-50%);",
                    "⌘K"
                }
            }

            div {
                class: "topbar-notifications-wrap",
                button {
                    "data-testid": "topbar-notifications-button",
                    class: "btn-icon focus-ring topbar-bell",
                    "aria-label": "Notifications ({notification_ctx.feed.read().unread_count} unread)",
                    "aria-expanded": "{notifications_open()}",
                    "aria-haspopup": "dialog",
                    "aria-controls": "topbar-notifications-panel",
                    title: "Notifications",
                    onclick: move |_| {
                        let next_open = !notifications_open();
                        notifications_open.set(next_open);
                        if next_open && let Some(owner) = open_owner.clone() {
                            load_account_notifications(
                                notification_ctx,
                                owner,
                                false,
                                true,
                            );
                            spawn(async move {
                                TimeoutFuture::new(0).await;
                                focus_notification_menu();
                            });
                        }
                    },
                    svg {
                        class: "w-4 h-4",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        view_box: "0 0 24 24",
                        path {
                            d: "M15 17h5l-1.405-1.405A2.032 2.032 0 0118 14.158V11a6.002 6.002 0 00-4-5.659V5a2 2 0 10-4 0v.341C7.67 6.165 6 8.388 6 11v3.159c0 .538-.214 1.055-.595 1.436L4 17h5m6 0v1a3 3 0 11-6 0v-1m6 0H9"
                        }
                    }
                    if notification_ctx.feed.read().unread_count > 0 {
                        span {
                            "data-testid": "topbar-notifications-badge",
                            class: "topbar-bell-badge",
                            aria_hidden: "true",
                            "{bounded_badge(notification_ctx.feed.read().unread_count)}"
                        }
                    }
                    span { role: "status", aria_live: "polite", aria_atomic: "true", class: "sr-only", "{notification_ctx.feed.read().unread_count} unread notifications" }
                }

                if notifications_open() {
                    button {
                        class: "cf-overlay-backdrop",
                        r#type: "button",
                        aria_label: "Close notifications",
                        tabindex: "-1",
                        onclick: move |_| close_notifications(notifications_open),
                    }
                    div {
                        "data-testid": "topbar-notifications-panel",
                        id: "topbar-notifications-panel",
                        class: "notif-panel",
                        role: "dialog",
                        "aria-modal": "true",
                        aria_label: "Notifications",
                        tabindex: "-1",
                        onkeydown: move |evt| {
                            match evt.key() {
                                Key::Escape => close_notifications(notifications_open),
                                Key::Tab => {
                                    if trap_notification_tab(evt.modifiers().shift()) {
                                        evt.prevent_default();
                                        evt.stop_propagation();
                                    }
                                }
                                Key::ArrowDown => {
                                    if notification_panel_has_focus() {
                                        evt.prevent_default();
                                        focus_notification_item(None, 1);
                                    }
                                }
                                Key::ArrowUp => {
                                    if notification_panel_has_focus() {
                                        evt.prevent_default();
                                        focus_notification_item(None, -1);
                                    }
                                }
                                Key::Home => {
                                    if notification_panel_has_focus() {
                                        evt.prevent_default();
                                        focus_notification_boundary(false);
                                    }
                                }
                                Key::End => {
                                    if notification_panel_has_focus() {
                                        evt.prevent_default();
                                        focus_notification_boundary(true);
                                    }
                                }
                                _ => {}
                            }
                        },
                        div {
                            class: "notif-head",
                            strong { "Notifications" }
                            button {
                                "data-testid": "topbar-notifications-mark-read",
                                class: "btn-icon focus-ring",
                                aria_label: "Mark all notifications read",
                                "aria-busy": notification_ctx.feed.read().mutation_pending(&NotificationMutation::MarkAll),
                                disabled: notification_ctx.feed.read().mutation_pending(&NotificationMutation::MarkAll),
                                title: "Mark all read",
                                style: "padding: 4px;",
                                onclick: move |_| {
                                    if let Some(owner) = mark_all_owner.clone() {
                                        mark_all_notifications_read(notification_ctx, owner);
                                    }
                                },
                                svg {
                                    class: "w-3.5 h-3.5",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    view_box: "0 0 24 24",
                                    path { d: "M5 13l4 4L19 7" }
                                }
                                if notification_ctx.feed.read().mutation_pending(&NotificationMutation::MarkAll) {
                                    span { role: "status", aria_live: "polite", class: "sr-only", "Marking all notifications read." }
                                }
                            }
                        }
                        ul {
                            class: "notif-list",
                            if let Some(failure) = notification_ctx.feed.read().load_failure.clone() {
                                li {
                                    role: "alert",
                                    class: "notif-error",
                                    span { "{failure.message}" }
                                    if let Some(owner) = load_retry_owner.clone() {
                                        button {
                                            class: "btn btn-ghost focus-ring xs",
                                            r#type: "button",
                                            onclick: move |_| retry_notification_load(
                                                notification_ctx,
                                                owner.clone(),
                                                failure.operation.clone(),
                                                failure.mutation_epoch.unwrap_or_default(),
                                            ),
                                            "Retry"
                                        }
                                    }
                                }
                            }
                            for failure in notification_ctx.feed.read().mutation_failures.clone() {
                                li {
                                    key: "mutation-error-{failure.operation:?}",
                                    role: "alert",
                                    class: "notif-error",
                                    span { "{failure.message}" }
                                    if let Some(owner) = mutation_retry_owner.clone() {
                                        {
                                        let mutation_pending = notification_ctx.feed.read().mutation_pending(&failure.operation);
                                        rsx! { button {
                                            class: "btn btn-ghost focus-ring xs",
                                            r#type: "button",
                                            "aria-busy": mutation_pending,
                                            disabled: mutation_pending,
                                            onclick: move |_| retry_notification_action(
                                                 notification_ctx,
                                                 owner.clone(),
                                                 failure.operation.clone(),
                                                 notifications_open,
                                            ),
                                            if mutation_pending { "Retrying..." } else { "Retry" }
                                        } }
                                        }
                                    }
                                }
                            }
                            if let Some(notice) = notification_ctx.feed.read().pagination_notice.clone() {
                                li { role: "status", class: "notif-notice", "{notice}" }
                            }
                            if notification_ctx.feed.read().loading && notification_ctx.feed.read().items.is_empty() {
                                li { role: "status", aria_live: "polite", class: "help", style: "padding: 12px;", "Loading notifications..." }
                            } else if notification_ctx.feed.read().items.is_empty() && notification_ctx.feed.read().load_failure.is_none() {
                                li { role: "status", class: "notif-empty", "You're all caught up" }
                            } else {
                                for item in notification_ctx.feed.read().items.clone() {
                                    {
                                    let accessible_label = notification_accessible_label(&item);
                                    let dismiss_owner = notification_owner.clone();
                                    let read_pending = notification_ctx.feed.read().mutation_pending(&NotificationMutation::Read(item.id));
                                    let dismiss_pending = notification_ctx.feed.read().mutation_pending(&NotificationMutation::Dismiss(item.id));
                                    rsx! { li {
                                        key: "notif-{item.id}",
                                        class: "notif-item-row",
                                        button {
                                        class: if item.read_at.is_none() { "notif-item unread focus-ring" } else { "notif-item focus-ring" },
                                        tabindex: "0",
                                        aria_label: "{accessible_label}",
                                        "aria-busy": read_pending,
                                        disabled: read_pending,
                                        "data-notification-id": "{item.id}",
                                        "data-testid": "topbar-notification-item-{item.id}",
                                        onkeydown: {
                                            let item_id = item.id;
                                             move |evt| {
                                                 match evt.key() {
                                                     Key::ArrowDown | Key::ArrowUp => {
                                                         evt.prevent_default();
                                                         evt.stop_propagation();
                                                         focus_notification_item(Some(item_id), if evt.key() == Key::ArrowDown { 1 } else { -1 });
                                                     }
                                                     Key::Home | Key::End => {
                                                         evt.prevent_default();
                                                         evt.stop_propagation();
                                                         focus_notification_boundary(evt.key() == Key::End);
                                                     }
                                                     _ => {}
                                                 }
                                             }
                                        },
                                    onclick: {
                                         let nav = nav.clone();
                                         let target = notification_target(&item.route);
                                         let item_id = item.id;
                                         let mutation_owner = notification_owner.clone();
                                         move |_| {
                                            close_notifications(notifications_open);
                                            if let Some(target) = target.clone() {
                                                match target {
                                                    NotificationTarget::Route(route) => {
                                                        nav.push(route);
                                                    }
                                                    NotificationTarget::SystemDeploy(id) => open_system_deploy(&id),
                                                }
                                            }
                                             if let Some(owner) = mutation_owner.clone() {
                                                 mark_notification_read(notification_ctx, owner, item_id);
                                             }
                                         }
                                    },
                                    span {
                                        class: "notif-icon",
                                        aria_hidden: "true",
                                        style: "color: {notification_color(item.category)}; background: color-mix(in oklab, {notification_color(item.category)} 16%, transparent);",
                                        match notification_kind(item.category) {
                                            NotificationKind::Deploy => rsx!(
                                                svg {
                                                    class: "w-3.5 h-3.5",
                                                    fill: "none",
                                                    stroke: "currentColor",
                                                    stroke_width: "2",
                                                    view_box: "0 0 24 24",
                                                    path { d: "M14.7 6.3a1 1 0 000 1.4l1.6 1.6a1 1 0 001.4 0l3.77-3.77a6 6 0 01-7.94 7.94l-6.91 6.91a2.12 2.12 0 01-3-3l6.91-6.91a6 6 0 017.94-7.94l-3.76 3.76z" }
                                                }
                                            ),
                                            NotificationKind::Build => rsx!(
                                                svg {
                                                    class: "w-3.5 h-3.5",
                                                    fill: "none",
                                                    stroke: "currentColor",
                                                    stroke_width: "2",
                                                    view_box: "0 0 24 24",
                                                    path { d: "M14.7 6.3a1 1 0 000 1.4l1.6 1.6a1 1 0 001.4 0l3.77-3.77a6 6 0 01-7.94 7.94l-6.91 6.91a2.12 2.12 0 01-3-3l6.91-6.91a6 6 0 017.94-7.94l-3.76 3.76z" }
                                                }
                                            ),
                                            NotificationKind::Shield => rsx!(
                                                svg {
                                                    class: "w-3.5 h-3.5",
                                                    fill: "none",
                                                    stroke: "currentColor",
                                                    stroke_width: "2",
                                                    view_box: "0 0 24 24",
                                                    path { d: "M12 3l7 3v6c0 5-3 7.5-7 9-4-1.5-7-4-7-9V6l7-3z" }
                                                }
                                            ),
                                            NotificationKind::Warning => rsx!(
                                                svg {
                                                    class: "w-3.5 h-3.5",
                                                    fill: "none",
                                                    stroke: "currentColor",
                                                    stroke_width: "2",
                                                    view_box: "0 0 24 24",
                                                    path { d: "M12 9v4m0 4h.01" }
                                                    path { d: "M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z" }
                                                }
                                            ),
                                            NotificationKind::Evaluation => rsx!(
                                                svg {
                                                    class: "w-3.5 h-3.5",
                                                    fill: "none",
                                                    stroke: "currentColor",
                                                    stroke_width: "2",
                                                    view_box: "0 0 24 24",
                                                    path { d: "M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2" }
                                                    path { d: "M9 5a2 2 0 002 2h2a2 2 0 002-2" }
                                                    path { d: "M9 12l2 2 4-4" }
                                                }
                                            ),
                                        }
                                    }
                                    span {
                                        style: "min-width: 0; flex: 1;",
                                        span {
                                            class: "notif-title-line",
                                            span {
                                                class: "notif-title",
                                                "{item.title}"
                                            }
                                            if item.read_at.is_none() {
                                                span { class: "notif-unread-marker", aria_hidden: "true" }
                                                span { class: "sr-only", "Unread notification." }
                                            }
                                        }
                                        span {
                                            class: "notif-sub",
                                            "{item.summary}"
                                        }
                                    }
                                    time {
                                        class: "notif-at",
                                        title: "{item.created_at}",
                                        datetime: "{item.created_at.to_rfc3339()}",
                                        "{relative_time(item.created_at)}"
                                    }
                                    }
                                    button {
                                        class: "btn-icon focus-ring notif-dismiss",
                                        r#type: "button",
                                        title: "Dismiss notification",
                                        aria_label: if dismiss_pending { format!("Dismissing {}", item.title) } else { format!("Dismiss {}", item.title) },
                                        "aria-busy": dismiss_pending,
                                        disabled: dismiss_pending,
                                         onkeydown: move |evt| {
                                             match evt.key() {
                                                 Key::ArrowDown | Key::ArrowUp => {
                                                     evt.prevent_default();
                                                     focus_notification_item(Some(item.id), if evt.key() == Key::ArrowDown { 1 } else { -1 });
                                                 }
                                                 Key::Home | Key::End => {
                                                     evt.prevent_default();
                                                     focus_notification_boundary(evt.key() == Key::End);
                                                 }
                                                 Key::Escape => close_notifications(notifications_open),
                                                 _ => {}
                                             }
                                             evt.stop_propagation();
                                         },
                                         onclick: move |evt| {
                                             evt.stop_propagation();
                                              if let Some(owner) = dismiss_owner.clone() {
                                                  dismiss_notification(
                                                      notification_ctx,
                                                      owner,
                                                      item.id,
                                                      notifications_open,
                                                  );
                                              }
                                         },
                                        if dismiss_pending {
                                            span { role: "status", aria_live: "polite", class: "sr-only", "Dismissing notification." }
                                        }
                                        "×"
                                    }
                                } }
                                }
                            }
                            }
                        }
                        div {
                            class: "notif-foot",
                            if notification_ctx.feed.read().next_cursor.is_some() && notification_ctx.feed.read().reconciliation.is_none() {
                                button {
                                    "data-testid": "topbar-notifications-load-more",
                                    class: "btn btn-ghost focus-ring xs",
                                    r#type: "button",
                                    disabled: notification_ctx.feed.read().loading || notification_ctx.feed.read().loading_more,
                                    onclick: move |_| {
                                        if let Some(owner) = more_owner.clone() {
                                            load_account_notifications(notification_ctx, owner, true, false)
                                        }
                                    },
                                    if notification_ctx.feed.read().loading_more { "Loading..." } else { "Load more" }
                                }
                                if notification_ctx.feed.read().loading_more { span { role: "status", aria_live: "polite", class: "sr-only", "Loading more notifications." } }
                            }
                            button {
                                "data-testid": "topbar-notifications-settings-button",
                                class: "btn btn-ghost focus-ring xs",
                                r#type: "button",
                                title: "Notification settings",
                                onclick: move |_| {
                                    notifications_open.set(false);
                                    nav.push(Route::ProfileView {});
                                },
                                "Notification settings"
                            }
                        }
                    }
                }
            }

            button {
                class: "btn-icon focus-ring",
                "aria-label": "Toggle theme",
                title: "Toggle theme",
                onclick: move |_| {
                    let next = ui_theme().toggle();
                    ui_theme.set(next);
                    prefs_ctx.save_update.call(UpdateUserPreferences {
                        theme: Some(preferences::theme_to_preference(next)),
                        ..UpdateUserPreferences::default()
                    });
                },
                if ui_theme() == UiTheme::Dark {
                    svg {
                        class: "w-4 h-4",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        view_box: "0 0 24 24",
                        circle { cx: "12", cy: "12", r: "4" }
                        path {
                            d: "M12 2v2m0 16v2M4.93 4.93l1.41 1.41m11.32 11.32l1.41 1.41M2 12h2m16 0h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41"
                        }
                    }
                } else {
                    svg {
                        class: "w-4 h-4",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        view_box: "0 0 24 24",
                        path {
                            d: "M21 12.79A9 9 0 1111.21 3 7 7 0 0021 12.79z"
                        }
                    }
                }
            }

            button {
                class: "btn-icon focus-ring",
                "aria-label": "Tweaks",
                title: "Tweaks",
                onclick: move |_| {
                    tweaks_open.set(!tweaks_open());
                },
                svg {
                    class: "w-4 h-4",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    view_box: "0 0 24 24",
                    path {
                        d: "M12 6V4m0 2a2 2 0 100 4m0-4a2 2 0 110 4m-6 8a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4m6 6v10m6-2a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4"
                    }
                }
            }

            if tweaks_open() {
                div {
                    style: "position: fixed; inset: 0; z-index: 49;",
                    onclick: move |_| tweaks_open.set(false),
                }
                div {
                    class: "cf-tweaks-menu",
                    div {
                        class: "cf-tweaks-head",
                        strong { "Tweaks" }
                        button {
                            class: "btn-icon focus-ring",
                            "aria-label": "Close tweaks",
                            onclick: move |_| tweaks_open.set(false),
                            svg {
                                class: "w-3.5 h-3.5",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                view_box: "0 0 24 24",
                                path { d: "M6 6l12 12M18 6L6 18" }
                            }
                        }
                    }
                    div {
                        class: "cf-tweaks-body",
                        TweakRow {
                            label: "Theme",
                            options: vec![("dark", "Dark"), ("light", "Light")],
                            value: ui_theme().as_attr().to_string(),
                            on_change: move |value: String| {
                                let next = if value == "light" { UiTheme::Light } else { UiTheme::Dark };
                                ui_theme.set(next);
                                prefs_ctx.save_update.call(UpdateUserPreferences {
                                    theme: Some(preferences::theme_to_preference(next)),
                                    ..UpdateUserPreferences::default()
                                });
                            }
                        }
                        TweakRow {
                            label: "Density",
                            options: vec![("comfortable", "Comfort"), ("compact", "Compact")],
                            value: density(),
                            on_change: move |value: String| {
                                density.set(value.clone());
                                preferences::write_storage(preferences::DENSITY_KEY, &value);
                                set_root_attr("data-density", &value);
                                prefs_ctx.save_update.call(UpdateUserPreferences {
                                    density: Some(preferences::density_from_storage(Some(&value))),
                                    ..UpdateUserPreferences::default()
                                });
                            }
                        }
                        TweakRow {
                            label: "Default view",
                            options: vec![("cards", "Cards"), ("table", "Table")],
                            value: default_view(),
                            on_change: move |value: String| {
                                default_view.set(value.clone());
                                preferences::write_storage(preferences::SYSTEMS_VIEW_KEY, &value);
                                prefs_ctx.save_update.call(UpdateUserPreferences {
                                    default_systems_view: Some(preferences::systems_view_from_storage(Some(&value))),
                                    ..UpdateUserPreferences::default()
                                });
                            }
                        }
                        TweakRow {
                            label: "Sidebar",
                            options: vec![("full", "Full"), ("rail", "Rail")],
                            value: if is_collapsed() { "rail".to_string() } else { "full".to_string() },
                            on_change: move |value: String| {
                                let collapsed = value == "rail";
                                is_collapsed.set(collapsed);
                                preferences::write_storage(
                                    preferences::SIDEBAR_COLLAPSED_KEY,
                                    if collapsed { "true" } else { "false" },
                                );
                                prefs_ctx.save_update.call(UpdateUserPreferences {
                                    sidebar_collapsed: Some(collapsed),
                                    ..UpdateUserPreferences::default()
                                });
                            }
                        }
                        if let Some(error) = save_error() {
                            div {
                                class: "help",
                                style: "color: var(--cf-critical); margin-top: 8px;",
                                "{error}"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_NOTIFICATION_RECONCILIATION_PAGES, MAX_NOTIFICATION_RECONCILIATION_ROWS,
        NotificationFeed, NotificationMutation, NotificationOwner, NotificationRequestKind,
        NotificationTarget, bounded_badge, dismiss_focus_is_current, notification_accessible_label,
        notification_boundary_index, notification_focus_index, notification_target,
    };
    use crate::api::models::{
        NotificationCategory, UserNotificationDto, UserNotificationsResponse,
    };
    use crate::routes::Route;

    fn owner(user_id: &str, auth_generation: u64) -> NotificationOwner {
        NotificationOwner::new(user_id.to_string(), auth_generation)
    }

    fn notification(id: u128, seconds: i64, unread: bool) -> UserNotificationDto {
        UserNotificationDto {
            id: uuid::Uuid::from_u128(id),
            category: NotificationCategory::BuildFailures,
            title: format!("Notification {id}"),
            summary: "Summary".to_string(),
            route: "/builds".to_string(),
            created_at: chrono::DateTime::from_timestamp(seconds, 0).unwrap(),
            read_at: (!unread).then(|| chrono::DateTime::from_timestamp(seconds + 1, 0).unwrap()),
        }
    }

    fn response(
        notifications: Vec<UserNotificationDto>,
        unread_count: i64,
        next_cursor: Option<&str>,
    ) -> UserNotificationsResponse {
        UserNotificationsResponse {
            notifications,
            unread_count,
            next_cursor: next_cursor.map(str::to_string),
        }
    }

    fn feed_with_stale_head() -> (
        NotificationOwner,
        NotificationFeed,
        super::NotificationRequest,
    ) {
        let owner = owner("account-a", 4);
        let mut feed = NotificationFeed::default();
        feed.reset(Some(owner.clone()));
        let initial = feed.start_head(&owner, false).unwrap();
        feed.finish_success(
            &initial,
            response(
                vec![
                    notification(3, 30, true),
                    notification(2, 20, true),
                    notification(1, 10, true),
                ],
                3,
                None,
            ),
        );
        let stale = feed.start_head(&owner, false).unwrap();
        (owner, feed, stale)
    }

    #[test]
    fn notification_route_accepts_only_exact_poam_query() {
        let poam_id = "0198f3f0-e8cc-7e5d-b53d-0f31840c8712";
        assert_eq!(
            notification_target(&format!("/compliance?poam={poam_id}")),
            Some(NotificationTarget::Route(Route::ComplianceView {
                bundle: String::new(),
                version: String::new(),
                system: String::new(),
                policy: String::new(),
                poam: poam_id.to_string(),
                view: String::new(),
            }))
        );
        assert_eq!(
            notification_target(&format!("/compliance?poam={poam_id}&view=summary")),
            None
        );
        assert_eq!(notification_target("/compliance?poam=not-a-uuid"), None);
    }

    #[test]
    fn notification_route_preserves_existing_destinations() {
        assert_eq!(
            notification_target("/systems?state=offline"),
            Some(NotificationTarget::Route(Route::SystemsView {
                query: String::new(),
            }))
        );
        assert_eq!(
            notification_target("/builds"),
            Some(NotificationTarget::Route(Route::BuildsView {}))
        );
        assert_eq!(
            notification_target("/cves"),
            Some(NotificationTarget::Route(Route::CvesView {
                query: String::new(),
            }))
        );
        assert_eq!(
            notification_target("/evaluations"),
            Some(NotificationTarget::Route(Route::EvaluationsView {}))
        );
        assert_eq!(
            notification_target("/profile"),
            Some(NotificationTarget::Route(Route::ProfileView {}))
        );
    }

    #[test]
    fn deployment_notification_parses_exact_system_and_deploy_tab() {
        let id = "26ee295d-7f12-48ae-99b5-2ccf07716782";
        assert_eq!(
            notification_target(&format!("/systems/{id}?notice=pending&tab=deploy")),
            Some(NotificationTarget::SystemDeploy(id.to_string()))
        );
        assert_eq!(
            notification_target("/systems"),
            Some(NotificationTarget::Route(Route::SystemsView {
                query: String::new(),
            }))
        );
        assert_eq!(notification_target("/systems-not-really"), None);
        assert_eq!(
            notification_target(&format!("/systems/{id}?tab=deployment")),
            None
        );
        assert_eq!(notification_target("/systems/not-a-uuid?tab=deploy"), None);
    }

    #[test]
    fn notification_arrow_navigation_enters_and_wraps_the_item_list() {
        assert_eq!(notification_focus_index(3, None, 1), 0);
        assert_eq!(notification_focus_index(3, None, -1), 2);
        assert_eq!(notification_focus_index(3, Some(0), -1), 2);
        assert_eq!(notification_focus_index(3, Some(2), 1), 0);
        assert_eq!(notification_focus_index(3, Some(1), 1), 2);
    }

    #[test]
    fn notification_home_and_end_select_boundaries() {
        assert_eq!(notification_boundary_index(0, false), None);
        assert_eq!(notification_boundary_index(3, false), Some(0));
        assert_eq!(notification_boundary_index(3, true), Some(2));
    }

    #[test]
    fn notification_badge_is_visually_bounded() {
        assert_eq!(bounded_badge(-1), "0");
        assert_eq!(bounded_badge(99), "99");
        assert_eq!(bounded_badge(100), "99+");
    }

    #[test]
    fn stale_same_account_get_cannot_reverse_successful_read() {
        let (owner, mut feed, stale) = feed_with_stale_head();
        let attempt = feed
            .start_mutation(&owner, NotificationMutation::Read(uuid::Uuid::from_u128(3)))
            .unwrap();
        feed.read_succeeded(&attempt, uuid::Uuid::from_u128(3));
        let fresh = feed
            .finish_success(&stale, response(vec![notification(9, 90, true)], 4, None))
            .unwrap();

        assert_eq!(feed.unread_count, 2);
        assert!(feed.items[0].read_at.is_some());
        assert_eq!(fresh.kind, NotificationRequestKind::Head);
        assert!(!feed.items.iter().any(|item| item.id.as_u128() == 9));
    }

    #[test]
    fn stale_same_account_get_cannot_restore_successful_dismissal() {
        let (owner, mut feed, stale) = feed_with_stale_head();
        let attempt = feed
            .start_mutation(
                &owner,
                NotificationMutation::Dismiss(uuid::Uuid::from_u128(2)),
            )
            .unwrap();
        feed.dismiss_succeeded(&attempt, uuid::Uuid::from_u128(2));
        let fresh = feed
            .finish_success(&stale, response(vec![notification(9, 90, true)], 3, None))
            .unwrap();

        assert!(
            !feed
                .items
                .iter()
                .any(|item| item.id == uuid::Uuid::from_u128(2))
        );
        assert_eq!(fresh.kind, NotificationRequestKind::Head);
        assert!(!feed.items.iter().any(|item| item.id.as_u128() == 9));
    }

    #[test]
    fn stale_same_account_get_failure_queues_fresh_head_after_mutation() {
        let (owner, mut feed, stale) = feed_with_stale_head();
        let attempt = feed
            .start_mutation(&owner, NotificationMutation::Read(uuid::Uuid::from_u128(3)))
            .unwrap();
        feed.read_succeeded(&attempt, uuid::Uuid::from_u128(3));

        let fresh = feed
            .finish_error(&stale, "stale failure".to_string())
            .unwrap();

        assert_eq!(fresh.kind, NotificationRequestKind::Head);
        assert!(feed.load_failure.is_none());
        assert!(feed.items[0].read_at.is_some());
        assert_eq!(feed.unread_count, 2);
    }

    #[test]
    fn stale_same_account_get_cannot_reverse_successful_mark_all() {
        let (owner, mut feed, stale) = feed_with_stale_head();
        let attempt = feed
            .start_mutation(&owner, NotificationMutation::MarkAll)
            .unwrap();
        feed.mark_all_succeeded(&attempt);
        let fresh = feed
            .finish_success(
                &stale,
                response(
                    vec![
                        notification(3, 30, true),
                        notification(2, 20, true),
                        notification(1, 10, true),
                    ],
                    3,
                    None,
                ),
            )
            .unwrap();

        assert_eq!(feed.unread_count, 0);
        assert!(feed.items.iter().all(|item| item.read_at.is_some()));
        assert_eq!(fresh.kind, NotificationRequestKind::Head);
    }

    #[test]
    fn failed_mutations_retain_rows_and_expose_the_exact_retry() {
        let (owner, mut feed, stale) = feed_with_stale_head();
        let before = feed.items.clone();
        let operation = NotificationMutation::Dismiss(uuid::Uuid::from_u128(2));
        let attempt = feed.start_mutation(&owner, operation.clone()).unwrap();
        feed.mutation_failed(&attempt, "Could not dismiss notification".to_string());

        assert_eq!(feed.items, before);
        assert_eq!(feed.unread_count, 3);
        assert_eq!(feed.mutation_failures[0].operation, operation,);
        assert!(feed.active_request.as_ref() == Some(&stale));
    }

    #[test]
    fn successful_read_does_not_clear_failed_dismiss_or_load() {
        let (owner, mut feed, head) = feed_with_stale_head();
        assert!(
            feed.finish_error(&head, "head failed".to_string())
                .is_none()
        );
        let dismiss = feed
            .start_mutation(
                &owner,
                NotificationMutation::Dismiss(uuid::Uuid::from_u128(2)),
            )
            .unwrap();
        feed.mutation_failed(&dismiss, "dismiss failed".to_string());
        let read = feed
            .start_mutation(&owner, NotificationMutation::Read(uuid::Uuid::from_u128(3)))
            .unwrap();
        feed.read_succeeded(&read, uuid::Uuid::from_u128(3));

        assert_eq!(feed.load_failure.as_ref().unwrap().message, "head failed");
        assert_eq!(feed.mutation_failures.len(), 1);
        assert_eq!(feed.mutation_failures[0].operation, dismiss.operation);
    }

    #[test]
    fn successful_head_does_not_clear_failed_mutation() {
        let (owner, mut feed, head) = feed_with_stale_head();
        let dismiss = feed
            .start_mutation(
                &owner,
                NotificationMutation::Dismiss(uuid::Uuid::from_u128(2)),
            )
            .unwrap();
        feed.mutation_failed(&dismiss, "dismiss failed".to_string());
        feed.finish_success(
            &head,
            response(
                vec![notification(3, 30, true), notification(2, 20, true)],
                3,
                None,
            ),
        );

        assert_eq!(feed.mutation_failures.len(), 1);
        assert_eq!(feed.mutation_failures[0].operation, dismiss.operation);
    }

    #[test]
    fn double_click_first_success_second_would_fail_starts_one_mutation() {
        let owner = owner("account-a", 1);
        let mut feed = NotificationFeed::default();
        feed.reset(Some(owner.clone()));
        let operation = NotificationMutation::Read(uuid::Uuid::from_u128(1));
        let first = feed.start_mutation(&owner, operation.clone()).unwrap();
        let second = feed.start_mutation(&owner, operation.clone());

        assert!(second.is_none(), "the second API request must not start");
        feed.read_succeeded(&first, uuid::Uuid::from_u128(1));

        assert!(feed.mutation_failures.is_empty());
        assert!(!feed.mutation_pending(&operation));
    }

    #[test]
    fn double_click_first_failure_second_would_succeed_starts_one_mutation() {
        let owner = owner("account-a", 1);
        let mut feed = NotificationFeed::default();
        feed.reset(Some(owner.clone()));
        let operation = NotificationMutation::Dismiss(uuid::Uuid::from_u128(1));
        let first = feed.start_mutation(&owner, operation.clone()).unwrap();
        let second = feed.start_mutation(&owner, operation.clone());

        assert!(second.is_none(), "the second API request must not start");
        feed.mutation_failed(&first, "first request failed".to_string());
        assert_eq!(feed.mutation_failures.len(), 1);
        assert!(!feed.mutation_pending(&operation));
        assert!(feed.start_mutation(&owner, operation).is_some());
    }

    #[test]
    fn independent_row_mutations_can_remain_pending_together() {
        let owner = owner("account-a", 1);
        let mut feed = NotificationFeed::default();
        feed.reset(Some(owner.clone()));

        assert!(
            feed.start_mutation(
                &owner,
                NotificationMutation::Dismiss(uuid::Uuid::from_u128(1)),
            )
            .is_some()
        );
        assert!(
            feed.start_mutation(
                &owner,
                NotificationMutation::Dismiss(uuid::Uuid::from_u128(2)),
            )
            .is_some()
        );
    }

    #[test]
    fn stale_account_dismiss_neither_applies_nor_owns_focus() {
        let account_a = owner("account-a", 1);
        let account_b = owner("account-b", 2);
        let mut feed = NotificationFeed::default();
        feed.reset(Some(account_a.clone()));
        let attempt = feed
            .start_mutation(
                &account_a,
                NotificationMutation::Dismiss(uuid::Uuid::from_u128(1)),
            )
            .unwrap();
        feed.reset(Some(account_b.clone()));

        let (applied, request) = feed.dismiss_succeeded(&attempt, uuid::Uuid::from_u128(1));
        assert!(!applied);
        assert!(request.is_none());
        assert!(!feed.mutation_pending(&attempt.operation));
        assert!(!dismiss_focus_is_current(
            applied,
            true,
            &account_a,
            feed.owner.as_ref(),
        ));
        assert!(dismiss_focus_is_current(
            true,
            true,
            &account_b,
            feed.owner.as_ref(),
        ));
    }

    #[test]
    fn account_switch_fences_response_and_clears_feed() {
        let account_a = owner("account-a", 1);
        let account_b = owner("account-b", 2);
        let mut feed = NotificationFeed::default();
        feed.reset(Some(account_a.clone()));
        let stale = feed.start_head(&account_a, false).unwrap();
        feed.reset(Some(account_b.clone()));

        assert!(
            feed.finish_success(&stale, response(vec![notification(1, 10, true)], 1, None))
                .is_none()
        );
        assert_eq!(feed.owner, Some(account_b));
        assert!(feed.items.is_empty());
        assert_eq!(feed.unread_count, 0);
    }

    #[test]
    fn append_deduplicates_and_uses_composite_order() {
        let owner = owner("account-a", 1);
        let mut feed = NotificationFeed::default();
        feed.reset(Some(owner.clone()));
        let head = feed.start_head(&owner, false).unwrap();
        feed.finish_success(
            &head,
            response(
                vec![notification(3, 30, true), notification(2, 20, true)],
                4,
                Some("page-2"),
            ),
        );
        let append = feed.start_append(&owner).unwrap();
        feed.finish_success(
            &append,
            response(
                vec![notification(2, 20, true), notification(1, 10, true)],
                4,
                None,
            ),
        );

        assert_eq!(
            feed.items
                .iter()
                .map(|item| item.id.as_u128())
                .collect::<Vec<_>>(),
            vec![3, 2, 1]
        );
        assert!(feed.next_cursor.is_none());
    }

    #[test]
    fn append_failure_retries_the_exact_failed_cursor() {
        let owner = owner("account-a", 1);
        let mut feed = NotificationFeed::default();
        feed.reset(Some(owner.clone()));
        let head = feed.start_head(&owner, false).unwrap();
        feed.finish_success(
            &head,
            response(vec![notification(2, 20, true)], 2, Some("page-2")),
        );
        let append = feed.start_append(&owner).unwrap();
        feed.finish_error(&append, "append failed".to_string());

        let failure = feed.load_failure.clone().unwrap();
        assert_eq!(
            failure.operation,
            NotificationRequestKind::Append("page-2".to_string())
        );
        let retry = feed
            .retry_request(&owner, failure.operation, failure.mutation_epoch.unwrap())
            .unwrap();
        assert_eq!(retry.kind, append.kind);
    }

    #[test]
    fn transient_head_failure_does_not_block_poll_recovery() {
        let owner = owner("account-a", 1);
        let mut feed = NotificationFeed::default();
        feed.reset(Some(owner.clone()));
        let failed = feed.start_head(&owner, false).unwrap();
        assert!(feed.start_head(&owner, true).is_none());
        let poll = feed
            .finish_error(&failed, "head failed".to_string())
            .unwrap();

        feed.finish_success(&poll, response(vec![notification(2, 20, true)], 1, None));

        assert!(feed.load_failure.is_none());
        assert_eq!(feed.items[0].id.as_u128(), 2);
    }

    #[test]
    fn transient_append_failure_does_not_block_open_reconciliation() {
        let owner = owner("account-a", 1);
        let mut feed = NotificationFeed::default();
        feed.reset(Some(owner.clone()));
        let head = feed.start_head(&owner, false).unwrap();
        feed.finish_success(
            &head,
            response(
                vec![notification(3, 30, true), notification(2, 20, true)],
                3,
                Some("page-2"),
            ),
        );
        let append = feed.start_append(&owner).unwrap();
        feed.finish_error(&append, "append failed".to_string());

        let refresh = feed.start_head(&owner, true).unwrap();
        let reconcile = feed
            .finish_success(
                &refresh,
                response(vec![notification(4, 40, true)], 4, Some("page-2")),
            )
            .unwrap();
        assert!(matches!(
            feed.load_failure.as_ref().map(|failure| &failure.operation),
            Some(NotificationRequestKind::Append(cursor)) if cursor == "page-2"
        ));
        feed.finish_success(
            &reconcile,
            response(
                vec![notification(3, 30, true), notification(2, 20, true)],
                4,
                Some("page-3"),
            ),
        );

        assert!(feed.load_failure.is_none());
        assert_eq!(feed.next_cursor.as_deref(), Some("page-3"));
    }

    #[test]
    fn authoritative_reconciliation_clears_unreached_failed_append_cursor() {
        let owner = owner("account-a", 1);
        let mut feed = NotificationFeed::default();
        feed.reset(Some(owner.clone()));
        let head = feed.start_head(&owner, false).unwrap();
        feed.finish_success(
            &head,
            response(
                vec![notification(3, 30, true), notification(2, 20, true)],
                3,
                Some("obsolete-append"),
            ),
        );
        let append = feed.start_append(&owner).unwrap();
        feed.finish_error(&append, "append failed".to_string());

        let refresh = feed.start_head(&owner, false).unwrap();
        feed.finish_success(
            &refresh,
            response(
                vec![
                    notification(4, 40, true),
                    notification(3, 30, true),
                    notification(2, 20, true),
                ],
                4,
                Some("authoritative-next"),
            ),
        );

        assert!(feed.load_failure.is_none());
        assert_eq!(feed.next_cursor.as_deref(), Some("authoritative-next"));
    }

    #[test]
    fn head_refresh_removes_middle_row_from_authoritative_range() {
        let owner = owner("account-a", 1);
        let mut feed = NotificationFeed::default();
        feed.reset(Some(owner.clone()));
        let head = feed.start_head(&owner, false).unwrap();
        feed.finish_success(
            &head,
            response(
                vec![
                    notification(5, 50, true),
                    notification(4, 40, true),
                    notification(3, 30, true),
                    notification(2, 20, true),
                    notification(1, 10, true),
                ],
                5,
                Some("old-tail"),
            ),
        );
        let refresh = feed.start_head(&owner, false).unwrap();
        feed.finish_success(
            &refresh,
            response(
                vec![
                    notification(5, 50, true),
                    notification(4, 40, true),
                    notification(2, 20, true),
                    notification(1, 10, true),
                ],
                4,
                Some("new-tail"),
            ),
        );

        assert_eq!(
            feed.items
                .iter()
                .map(|item| item.id.as_u128())
                .collect::<Vec<_>>(),
            vec![5, 4, 2, 1]
        );
        assert_eq!(feed.next_cursor.as_deref(), Some("new-tail"));
    }

    #[test]
    fn head_refresh_removes_oldest_loaded_row() {
        let owner = owner("account-a", 1);
        let mut feed = NotificationFeed::default();
        feed.reset(Some(owner.clone()));
        let head = feed.start_head(&owner, false).unwrap();
        feed.finish_success(
            &head,
            response(
                vec![
                    notification(5, 50, true),
                    notification(4, 40, true),
                    notification(3, 30, true),
                    notification(2, 20, true),
                    notification(1, 10, true),
                ],
                5,
                Some("old-tail"),
            ),
        );
        let refresh = feed.start_head(&owner, false).unwrap();
        let reconcile = feed
            .finish_success(
                &refresh,
                response(
                    vec![notification(6, 60, true), notification(5, 50, true)],
                    5,
                    Some("new-page-2"),
                ),
            )
            .unwrap();
        feed.finish_success(
            &reconcile,
            response(
                vec![
                    notification(4, 40, true),
                    notification(3, 30, true),
                    notification(2, 20, true),
                ],
                5,
                None,
            ),
        );

        assert_eq!(
            feed.items
                .iter()
                .map(|item| item.id.as_u128())
                .collect::<Vec<_>>(),
            vec![6, 5, 4, 3, 2]
        );
        assert!(feed.next_cursor.is_none());
    }

    #[test]
    fn head_refresh_fetches_more_than_one_page_of_new_rows() {
        let owner = owner("account-a", 1);
        let mut feed = NotificationFeed::default();
        feed.reset(Some(owner.clone()));
        let head = feed.start_head(&owner, false).unwrap();
        feed.finish_success(
            &head,
            response(
                vec![
                    notification(3, 30, true),
                    notification(2, 20, true),
                    notification(1, 10, true),
                ],
                3,
                Some("old-tail"),
            ),
        );
        let refresh = feed.start_head(&owner, false).unwrap();
        let reconcile = feed
            .finish_success(
                &refresh,
                response(
                    vec![notification(10, 100, true), notification(9, 90, true)],
                    5,
                    Some("new-page-2"),
                ),
            )
            .unwrap();

        assert_eq!(
            feed.items
                .iter()
                .map(|item| item.id.as_u128())
                .collect::<Vec<_>>(),
            vec![3, 2, 1]
        );
        assert_eq!(
            reconcile.kind,
            NotificationRequestKind::Reconcile("new-page-2".to_string())
        );
        assert!(feed.start_append(&owner).is_none());

        let reconcile = feed
            .finish_success(
                &reconcile,
                response(
                    vec![notification(8, 80, true), notification(7, 70, true)],
                    10,
                    Some("new-page-3"),
                ),
            )
            .unwrap();
        feed.finish_success(
            &reconcile,
            response(
                vec![
                    notification(6, 60, true),
                    notification(5, 50, true),
                    notification(4, 40, true),
                    notification(3, 30, true),
                    notification(2, 20, true),
                    notification(1, 10, true),
                ],
                10,
                Some("new-tail"),
            ),
        );
        assert_eq!(
            feed.items
                .iter()
                .map(|item| item.id.as_u128())
                .collect::<Vec<_>>(),
            vec![10, 9, 8, 7, 6, 5, 4, 3, 2, 1]
        );
        assert_eq!(feed.next_cursor.as_deref(), Some("new-tail"));
    }

    #[test]
    fn exhausted_server_replaces_unreached_old_suffix() {
        let owner = owner("account-a", 1);
        let mut feed = NotificationFeed::default();
        feed.reset(Some(owner.clone()));
        let head = feed.start_head(&owner, false).unwrap();
        feed.finish_success(
            &head,
            response(
                vec![
                    notification(3, 30, true),
                    notification(2, 20, true),
                    notification(1, 10, true),
                ],
                3,
                Some("old-tail"),
            ),
        );
        let refresh = feed.start_head(&owner, false).unwrap();
        let reconcile = feed
            .finish_success(
                &refresh,
                response(
                    vec![notification(4, 40, true), notification(3, 30, true)],
                    2,
                    Some("new-page-2"),
                ),
            )
            .unwrap();
        feed.finish_success(&reconcile, response(Vec::new(), 2, None));

        assert_eq!(
            feed.items
                .iter()
                .map(|item| item.id.as_u128())
                .collect::<Vec<_>>(),
            vec![4, 3]
        );
        assert!(feed.next_cursor.is_none());
        assert!(feed.pagination_notice.is_none());
    }

    #[test]
    fn reconciliation_failure_retries_its_new_head_cursor() {
        let owner = owner("account-a", 1);
        let mut feed = NotificationFeed::default();
        feed.reset(Some(owner.clone()));
        let head = feed.start_head(&owner, false).unwrap();
        feed.finish_success(&head, response(vec![notification(1, 10, true)], 1, None));
        let refresh = feed.start_head(&owner, false).unwrap();
        let reconcile = feed
            .finish_success(
                &refresh,
                response(vec![notification(3, 30, true)], 2, Some("new-page-2")),
            )
            .unwrap();
        feed.finish_error(&reconcile, "bridge failed".to_string());

        assert_eq!(
            feed.load_failure.as_ref().unwrap().operation,
            reconcile.kind
        );
        assert!(feed.reconciliation.is_some());
        assert!(feed.start_append(&owner).is_none());
        let failure = feed.load_failure.clone().unwrap();
        let retry = feed
            .retry_request(&owner, failure.operation, failure.mutation_epoch.unwrap())
            .unwrap();
        assert_eq!(retry.kind, reconcile.kind);
    }

    fn assert_failed_reconciliation_mutation_retry_restarts_at_head(
        operation: NotificationMutation,
    ) {
        let owner = owner("account-a", 1);
        let mut feed = NotificationFeed::default();
        feed.reset(Some(owner.clone()));
        let head = feed.start_head(&owner, false).unwrap();
        feed.finish_success(
            &head,
            response(
                vec![notification(2, 20, true), notification(1, 10, true)],
                2,
                None,
            ),
        );
        let refresh = feed.start_head(&owner, false).unwrap();
        let reconcile = feed
            .finish_success(
                &refresh,
                response(vec![notification(4, 40, true)], 3, Some("new-page-2")),
            )
            .unwrap();
        feed.finish_error(&reconcile, "bridge failed".to_string());
        let stale_failure = feed.load_failure.clone().unwrap();

        let attempt = feed.start_mutation(&owner, operation.clone()).unwrap();
        let fresh = match operation {
            NotificationMutation::Read(id) => feed.read_succeeded(&attempt, id),
            NotificationMutation::Dismiss(id) => feed.dismiss_succeeded(&attempt, id).1,
            NotificationMutation::MarkAll => feed.mark_all_succeeded(&attempt),
        }
        .unwrap();
        assert_eq!(fresh.kind, NotificationRequestKind::Head);
        assert!(feed.reconciliation.is_none());
        assert!(feed.load_failure.is_none());
        feed.finish_success(&fresh, response(vec![notification(8, 80, false)], 0, None));

        let retry = feed
            .retry_request(
                &owner,
                stale_failure.operation,
                stale_failure.mutation_epoch.unwrap(),
            )
            .unwrap();
        assert_eq!(retry.kind, NotificationRequestKind::Head);
        assert!(feed.reconciliation.is_none());
        assert_eq!(
            feed.items
                .iter()
                .map(|item| item.id.as_u128())
                .collect::<Vec<_>>(),
            vec![8]
        );
    }

    #[test]
    fn failed_reconciliation_then_dismiss_retry_restarts_at_head() {
        assert_failed_reconciliation_mutation_retry_restarts_at_head(
            NotificationMutation::Dismiss(uuid::Uuid::from_u128(1)),
        );
    }

    #[test]
    fn failed_reconciliation_then_read_retry_restarts_at_head() {
        assert_failed_reconciliation_mutation_retry_restarts_at_head(NotificationMutation::Read(
            uuid::Uuid::from_u128(1),
        ));
    }

    #[test]
    fn failed_reconciliation_then_mark_all_retry_restarts_at_head() {
        assert_failed_reconciliation_mutation_retry_restarts_at_head(NotificationMutation::MarkAll);
    }

    #[test]
    fn mutation_during_reconciliation_discards_stale_page_and_queues_fresh_head() {
        let owner = owner("account-a", 1);
        let mut feed = NotificationFeed::default();
        feed.reset(Some(owner.clone()));
        let head = feed.start_head(&owner, false).unwrap();
        feed.finish_success(
            &head,
            response(
                vec![notification(2, 20, true), notification(1, 10, true)],
                2,
                None,
            ),
        );
        let refresh = feed.start_head(&owner, false).unwrap();
        let reconcile = feed
            .finish_success(
                &refresh,
                response(
                    vec![notification(4, 40, true), notification(3, 30, true)],
                    4,
                    Some("new-page-2"),
                ),
            )
            .unwrap();
        let dismiss = feed
            .start_mutation(
                &owner,
                NotificationMutation::Dismiss(uuid::Uuid::from_u128(2)),
            )
            .unwrap();
        feed.dismiss_succeeded(&dismiss, uuid::Uuid::from_u128(2));

        let fresh = feed
            .finish_success(
                &reconcile,
                response(vec![notification(9, 90, true)], 99, None),
            )
            .unwrap();
        assert_eq!(
            feed.items
                .iter()
                .map(|item| item.id.as_u128())
                .collect::<Vec<_>>(),
            vec![1]
        );
        assert_eq!(feed.unread_count, 1);
        assert_eq!(fresh.kind, NotificationRequestKind::Head);
        assert!(feed.reconciliation.is_none());
    }

    #[test]
    fn reconciliation_bound_resets_old_pagination_with_truthful_notice() {
        let owner = owner("account-a", 1);
        let mut feed = NotificationFeed::default();
        feed.reset(Some(owner.clone()));
        let head = feed.start_head(&owner, false).unwrap();
        feed.finish_success(
            &head,
            response(vec![notification(1, 1, true)], 1, Some("old-tail")),
        );
        let refresh = feed.start_head(&owner, false).unwrap();
        let mut request = feed
            .finish_success(
                &refresh,
                response(vec![notification(100, 100, true)], 100, Some("new-2")),
            )
            .unwrap();
        for page in 2..=MAX_NOTIFICATION_RECONCILIATION_PAGES {
            let next = feed.finish_success(
                &request,
                response(
                    vec![notification(100 + page as u128, 100 + page as i64, true)],
                    100,
                    Some(&format!("new-{}", page + 1)),
                ),
            );
            if page < MAX_NOTIFICATION_RECONCILIATION_PAGES {
                request = next.unwrap();
            } else {
                assert!(next.is_none());
            }
        }

        assert!(
            feed.items
                .iter()
                .all(|item| item.id != uuid::Uuid::from_u128(1))
        );
        assert_eq!(
            MAX_NOTIFICATION_RECONCILIATION_ROWS,
            super::NOTIFICATION_PAGE_SIZE as usize * MAX_NOTIFICATION_RECONCILIATION_PAGES
        );
        assert_eq!(feed.next_cursor.as_deref(), Some("new-5"));
        assert!(
            feed.pagination_notice
                .as_deref()
                .unwrap()
                .contains("Older loaded history was reset")
        );
    }

    #[test]
    fn bell_open_queues_exactly_one_follow_up_refresh() {
        let owner = owner("account-a", 1);
        let mut feed = NotificationFeed::default();
        feed.reset(Some(owner.clone()));
        let poll = feed.start_head(&owner, false).unwrap();
        assert!(feed.start_head(&owner, true).is_none());
        assert!(feed.start_head(&owner, true).is_none());
        let refresh = feed.finish_success(&poll, response(Vec::new(), 0, None));
        assert!(refresh.is_some());
        assert!(!feed.queued_refresh);
    }

    #[test]
    fn equal_timestamps_use_descending_id_as_the_stable_tiebreaker() {
        let owner = owner("account-a", 1);
        let mut feed = NotificationFeed::default();
        feed.reset(Some(owner.clone()));
        let head = feed.start_head(&owner, false).unwrap();
        feed.finish_success(
            &head,
            response(
                vec![
                    notification(1, 10, true),
                    notification(3, 10, true),
                    notification(2, 10, true),
                ],
                3,
                None,
            ),
        );
        assert_eq!(
            feed.items
                .iter()
                .map(|item| item.id.as_u128())
                .collect::<Vec<_>>(),
            vec![3, 2, 1]
        );
    }

    #[test]
    fn task433_responsive_notification_preserves_server_text_and_timestamp() {
        let created_at = chrono::DateTime::parse_from_rfc3339("2026-08-31T12:34:56Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let notification = UserNotificationDto {
            id: uuid::Uuid::nil(),
            category: NotificationCategory::PolicyViolations,
            title: "POAM-0433 awaiting verification".into(),
            summary: "Platform Security must re-evaluate the finding.".into(),
            route: "/compliance?poam=00000000-0000-0000-0000-000000000433".into(),
            created_at,
            read_at: None,
        };

        assert_eq!(
            notification_accessible_label(&notification),
            "Unread notification. POAM-0433 awaiting verification. Platform Security must re-evaluate the finding. Received 2026-08-31T12:34:56+00:00."
        );

        let css = include_str!("../../../assets/app.css");
        assert!(css.contains("width: min(360px, calc(100dvw - 16px))"));
        assert!(css.contains("max-height: calc(100dvh - var(--coach-top, 64px) - 16px)"));
    }

    #[test]
    fn task433_narrow_shell_uses_overlay_navigation_and_usable_actions() {
        let css = include_str!("../../../assets/app.css");
        assert!(css.contains("@media (max-width: 767px)"));
        assert!(css.contains("grid-template-columns: minmax(0, 1fr)"));
        assert!(css.contains("@media (min-width: 768px)"));
        assert!(css.contains(".topbar-search {\n    display: none;"));
        assert!(css.contains("flex: 0 0 40px"));
        assert!(
            css.contains(":root[data-theme=\"light\"] .sidebar.cf-sidebar-shell.cf-sidebar-bg")
        );
        assert!(css.contains(":root[data-theme=\"light\"] .cf-mobile-drawer.cf-sidebar-bg"));
    }
}

#[component]
fn TweakRow(
    label: String,
    options: Vec<(&'static str, &'static str)>,
    value: String,
    on_change: EventHandler<String>,
) -> Element {
    rsx! {
        div {
            class: "cf-tweaks-row",
            label { "{label}" }
            div {
                class: "cf-tweaks-opts",
                for (option_value, option_label) in options {
                    button {
                        class: if value == option_value { "active" } else { "" },
                        onclick: move |_| on_change.call(option_value.to_string()),
                        "{option_label}"
                    }
                }
            }
        }
    }
}
