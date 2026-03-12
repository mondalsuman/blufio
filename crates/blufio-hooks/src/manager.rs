// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! HookManager: EventBus subscriber that dispatches hooks in priority order.
//!
//! Subscribes to the EventBus via a reliable mpsc channel and executes
//! matching hook commands using [`execute_hook`]. Hooks are stored in a
//! [`BTreeMap`] keyed by priority (lower number = higher priority) for
//! deterministic dispatch ordering.

use std::collections::{BTreeMap, HashSet};
use std::sync::atomic::AtomicU32;
use std::sync::Arc;
use std::time::Duration;

use blufio_bus::events::{BusEvent, HookEvent, new_event_id, now_timestamp};
use blufio_bus::EventBus;
use blufio_config::model::{HookConfig, HookDefinition};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::executor::execute_hook;
use crate::recursion::RecursionGuard;

/// Maps hook lifecycle event names (used in TOML config) to BusEvent type strings.
///
/// These are the 7 lifecycle events that are driven by EventBus events. Hooks
/// subscribed to these events are dispatched automatically when the matching
/// event flows through the bus.
pub const LIFECYCLE_EVENT_MAP: &[(&str, &str)] = &[
    ("session_created", "session.created"),
    ("session_closed", "session.closed"),
    ("pre_compaction", "compaction.started"),
    ("post_compaction", "compaction.completed"),
    ("degradation_changed", "resilience.degradation_level_changed"),
    ("config_reloaded", "config.reloaded"),
    ("memory_extracted", "memory.created"),
];

/// Direct lifecycle events that are NOT EventBus-driven.
///
/// These are called directly by serve.rs at specific lifecycle points
/// (before/after startup and shutdown) via [`HookManager::execute_lifecycle_hooks`].
const DIRECT_LIFECYCLE_EVENTS: &[&str] = &[
    "pre_start",
    "post_start",
    "pre_shutdown",
    "post_shutdown",
];

/// EventBus subscriber that dispatches hooks in priority order.
///
/// Hooks are organized in a [`BTreeMap<u32, Vec<HookDefinition>>`] where the
/// key is the priority (lower = higher priority). When an event arrives, all
/// hooks matching the event type string are dispatched in priority order.
///
/// A shared [`RecursionGuard`] prevents infinite loops when hook execution
/// triggers events that would themselves trigger hooks (e.g., `config_reloaded`
/// hooks modifying the config file).
pub struct HookManager {
    /// Hooks organized by priority (BTreeMap iterates in ascending key order).
    hooks: BTreeMap<u32, Vec<HookDefinition>>,
    /// Shared recursion depth counter across all hook dispatches.
    recursion_counter: Arc<AtomicU32>,
    /// Maximum recursion depth before hooks are skipped.
    max_depth: u32,
    /// Default timeout for hook execution.
    default_timeout: Duration,
    /// Restricted PATH for hook shell commands.
    allowed_path: String,
}

impl HookManager {
    /// Create a new HookManager from the given config.
    ///
    /// Only enabled hook definitions are added to the dispatch map.
    /// Event names in hook definitions are resolved from TOML lifecycle
    /// names (e.g., `session_created`) to EventBus type strings
    /// (e.g., `session.created`) for EventBus-driven hooks.
    pub fn new(config: &HookConfig) -> Self {
        let mut hooks: BTreeMap<u32, Vec<HookDefinition>> = BTreeMap::new();
        for def in &config.definitions {
            if def.enabled {
                hooks.entry(def.priority).or_default().push(def.clone());
            }
        }
        Self {
            hooks,
            recursion_counter: Arc::new(AtomicU32::new(0)),
            max_depth: config.max_recursion_depth,
            default_timeout: Duration::from_secs(config.default_timeout_secs),
            allowed_path: config.allowed_path.clone(),
        }
    }

    /// Run the event processing loop.
    ///
    /// Receives events from the reliable mpsc channel and dispatches
    /// matching hooks. Runs until the channel closes or the
    /// [`CancellationToken`] is cancelled.
    pub async fn run(
        &self,
        mut rx: mpsc::Receiver<BusEvent>,
        event_bus: Arc<EventBus>,
        cancel: CancellationToken,
    ) {
        loop {
            tokio::select! {
                Some(event) = rx.recv() => {
                    self.handle_event(&event, &event_bus).await;
                }
                _ = cancel.cancelled() => {
                    info!("hook manager shutting down");
                    break;
                }
            }
        }
    }

    /// Handle a single event from the EventBus.
    ///
    /// Iterates over all hooks in BTreeMap priority order (ascending) and
    /// dispatches hooks whose event name matches the incoming event type.
    /// Hook event names use TOML lifecycle names, so we need to resolve
    /// the EventBus type string to check for matches.
    async fn handle_event(&self, event: &BusEvent, event_bus: &EventBus) {
        let event_type = event.event_type_string();
        let stdin_json = match serde_json::to_string(event) {
            Ok(json) => json,
            Err(e) => {
                warn!(error = %e, "failed to serialize event for hook stdin");
                return;
            }
        };

        // Iterate BTreeMap in priority order (ascending = lower number first).
        for hooks in self.hooks.values() {
            for hook in hooks {
                // Check if this hook matches the event: resolve the hook's
                // TOML event name to EventBus type string for comparison.
                let hook_bus_event = resolve_event_name(&hook.event);
                if hook_bus_event == event_type {
                    self.dispatch_hook(hook, &stdin_json, event_type, event_bus)
                        .await;
                }
            }
        }
    }

    /// Execute lifecycle hooks that are not EventBus-driven.
    ///
    /// Called directly by serve.rs for `pre_start`, `post_start`,
    /// `pre_shutdown`, and `post_shutdown` events. Creates a synthetic
    /// JSON payload for the hook's stdin.
    pub async fn execute_lifecycle_hooks(
        &self,
        lifecycle_event: &str,
        event_bus: &EventBus,
    ) {
        let stdin_json = serde_json::json!({
            "lifecycle_event": lifecycle_event,
            "timestamp": now_timestamp(),
        })
        .to_string();

        for hooks in self.hooks.values() {
            for hook in hooks {
                if hook.event == lifecycle_event {
                    self.dispatch_hook(hook, &stdin_json, lifecycle_event, event_bus)
                        .await;
                }
            }
        }
    }

    /// Dispatch a single hook with recursion guard and event emission.
    async fn dispatch_hook(
        &self,
        hook: &HookDefinition,
        stdin_json: &str,
        trigger_event: &str,
        event_bus: &EventBus,
    ) {
        // Recursion guard
        let _guard = match RecursionGuard::try_enter(
            self.recursion_counter.clone(),
            self.max_depth,
        ) {
            Some(g) => g,
            None => {
                warn!(
                    hook = %hook.name,
                    depth = RecursionGuard::depth(&self.recursion_counter),
                    max = self.max_depth,
                    "hook recursion limit exceeded, skipping"
                );
                // Emit skipped event
                event_bus
                    .publish(BusEvent::Hook(HookEvent::Completed {
                        event_id: new_event_id(),
                        timestamp: now_timestamp(),
                        hook_name: hook.name.clone(),
                        trigger_event: trigger_event.to_string(),
                        status: "skipped".into(),
                        duration_ms: 0,
                        stdout: None,
                    }))
                    .await;
                return;
            }
        };

        // Emit triggered event
        event_bus
            .publish(BusEvent::Hook(HookEvent::Triggered {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                hook_name: hook.name.clone(),
                trigger_event: trigger_event.to_string(),
                priority: hook.priority,
            }))
            .await;

        let timeout = if hook.timeout_secs > 0 {
            Duration::from_secs(hook.timeout_secs)
        } else {
            self.default_timeout
        };
        let start = std::time::Instant::now();

        match execute_hook(&hook.command, stdin_json, timeout, &self.allowed_path).await {
            Ok(result) => {
                event_bus
                    .publish(BusEvent::Hook(HookEvent::Completed {
                        event_id: new_event_id(),
                        timestamp: now_timestamp(),
                        hook_name: hook.name.clone(),
                        trigger_event: trigger_event.to_string(),
                        status: "success".into(),
                        duration_ms: start.elapsed().as_millis() as u64,
                        stdout: result.stdout,
                    }))
                    .await;
            }
            Err(crate::executor::HookError::Timeout(_)) => {
                warn!(hook = %hook.name, "hook execution timed out");
                event_bus
                    .publish(BusEvent::Hook(HookEvent::Completed {
                        event_id: new_event_id(),
                        timestamp: now_timestamp(),
                        hook_name: hook.name.clone(),
                        trigger_event: trigger_event.to_string(),
                        status: "timeout".into(),
                        duration_ms: start.elapsed().as_millis() as u64,
                        stdout: None,
                    }))
                    .await;
            }
            Err(e) => {
                warn!(hook = %hook.name, error = %e, "hook execution failed");
                event_bus
                    .publish(BusEvent::Hook(HookEvent::Completed {
                        event_id: new_event_id(),
                        timestamp: now_timestamp(),
                        hook_name: hook.name.clone(),
                        trigger_event: trigger_event.to_string(),
                        status: "failed".into(),
                        duration_ms: start.elapsed().as_millis() as u64,
                        stdout: None,
                    }))
                    .await;
            }
        }
    }
}

/// Resolve a hook event name from TOML lifecycle format to EventBus type string.
///
/// For EventBus-driven events (e.g., `session_created` -> `session.created`),
/// returns the mapped string. For direct lifecycle events and unknown names,
/// returns the original name unchanged.
fn resolve_event_name(event_name: &str) -> &str {
    for &(toml_name, bus_name) in LIFECYCLE_EVENT_MAP {
        if event_name == toml_name {
            return bus_name;
        }
    }
    // Direct lifecycle events or unknown names: return as-is
    event_name
}

/// Validate that all hook event names in the config are recognized.
///
/// Returns a list of warning messages for unknown event names. An empty
/// return means all events are valid.
pub fn validate_hook_events(config: &HookConfig) -> Vec<String> {
    let valid_events: HashSet<&str> = LIFECYCLE_EVENT_MAP
        .iter()
        .map(|(k, _)| *k)
        .chain(DIRECT_LIFECYCLE_EVENTS.iter().copied())
        .collect();

    config
        .definitions
        .iter()
        .filter(|d| d.enabled && !valid_events.contains(d.event.as_str()))
        .map(|d| format!("unknown hook event '{}' in hook '{}'", d.event, d.name))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config(definitions: Vec<HookDefinition>) -> HookConfig {
        HookConfig {
            enabled: true,
            max_recursion_depth: 3,
            default_timeout_secs: 30,
            allowed_path: "/usr/bin:/usr/local/bin:/bin".to_string(),
            definitions,
        }
    }

    fn make_hook(name: &str, event: &str, priority: u32, enabled: bool) -> HookDefinition {
        HookDefinition {
            name: name.to_string(),
            event: event.to_string(),
            command: "echo test".to_string(),
            priority,
            timeout_secs: 30,
            enabled,
        }
    }

    #[test]
    fn new_builds_btreemap_from_config_definitions() {
        let config = make_config(vec![
            make_hook("hook-a", "session_created", 10, true),
            make_hook("hook-b", "session_closed", 20, true),
            make_hook("hook-c", "session_created", 10, true),
        ]);

        let mgr = HookManager::new(&config);

        // Two priority levels: 10 and 20
        assert_eq!(mgr.hooks.len(), 2);
        // Priority 10 has 2 hooks
        assert_eq!(mgr.hooks.get(&10).unwrap().len(), 2);
        // Priority 20 has 1 hook
        assert_eq!(mgr.hooks.get(&20).unwrap().len(), 1);
    }

    #[test]
    fn new_preserves_priority_order() {
        let config = make_config(vec![
            make_hook("high-priority", "session_created", 5, true),
            make_hook("low-priority", "session_closed", 100, true),
            make_hook("medium-priority", "config_reloaded", 50, true),
        ]);

        let mgr = HookManager::new(&config);

        // BTreeMap should iterate in key order: 5, 50, 100
        let priorities: Vec<u32> = mgr.hooks.keys().copied().collect();
        assert_eq!(priorities, vec![5, 50, 100]);
    }

    #[test]
    fn disabled_hooks_excluded_from_btreemap() {
        let config = make_config(vec![
            make_hook("enabled-hook", "session_created", 10, true),
            make_hook("disabled-hook", "session_closed", 20, false),
            make_hook("another-enabled", "config_reloaded", 30, true),
        ]);

        let mgr = HookManager::new(&config);

        // Only 2 enabled hooks
        let total: usize = mgr.hooks.values().map(|v| v.len()).sum();
        assert_eq!(total, 2);

        // No priority 20 entry (disabled hook)
        assert!(mgr.hooks.get(&20).is_none());
    }

    #[test]
    fn validate_hook_events_returns_empty_for_valid_events() {
        let config = make_config(vec![
            make_hook("hook-a", "session_created", 10, true),
            make_hook("hook-b", "session_closed", 20, true),
            make_hook("hook-c", "pre_start", 5, true),
            make_hook("hook-d", "post_shutdown", 100, true),
            make_hook("hook-e", "config_reloaded", 50, true),
            make_hook("hook-f", "memory_extracted", 50, true),
            make_hook("hook-g", "pre_compaction", 50, true),
            make_hook("hook-h", "post_compaction", 50, true),
            make_hook("hook-i", "degradation_changed", 50, true),
            make_hook("hook-j", "post_start", 50, true),
            make_hook("hook-k", "pre_shutdown", 50, true),
        ]);

        let errors = validate_hook_events(&config);
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    #[test]
    fn validate_hook_events_returns_error_for_unknown_events() {
        let config = make_config(vec![
            make_hook("good-hook", "session_created", 10, true),
            make_hook("bad-hook", "nonexistent_event", 20, true),
            make_hook("another-bad", "foo.bar", 30, true),
        ]);

        let errors = validate_hook_events(&config);
        assert_eq!(errors.len(), 2);
        assert!(errors[0].contains("nonexistent_event"));
        assert!(errors[0].contains("bad-hook"));
        assert!(errors[1].contains("foo.bar"));
        assert!(errors[1].contains("another-bad"));
    }

    #[test]
    fn validate_hook_events_ignores_disabled_hooks() {
        let config = make_config(vec![
            make_hook("disabled-bad", "nonexistent_event", 10, false),
            make_hook("enabled-good", "session_created", 20, true),
        ]);

        let errors = validate_hook_events(&config);
        assert!(errors.is_empty());
    }

    #[test]
    fn lifecycle_event_map_contains_all_seven_bus_events() {
        assert_eq!(LIFECYCLE_EVENT_MAP.len(), 7);

        let expected_toml_names = [
            "session_created",
            "session_closed",
            "pre_compaction",
            "post_compaction",
            "degradation_changed",
            "config_reloaded",
            "memory_extracted",
        ];

        for name in &expected_toml_names {
            assert!(
                LIFECYCLE_EVENT_MAP.iter().any(|(k, _)| k == name),
                "missing TOML event name: {}",
                name
            );
        }
    }

    #[test]
    fn resolve_event_name_maps_toml_to_bus_strings() {
        assert_eq!(resolve_event_name("session_created"), "session.created");
        assert_eq!(resolve_event_name("session_closed"), "session.closed");
        assert_eq!(resolve_event_name("pre_compaction"), "compaction.started");
        assert_eq!(resolve_event_name("post_compaction"), "compaction.completed");
        assert_eq!(
            resolve_event_name("degradation_changed"),
            "resilience.degradation_level_changed"
        );
        assert_eq!(resolve_event_name("config_reloaded"), "config.reloaded");
        assert_eq!(resolve_event_name("memory_extracted"), "memory.created");
    }

    #[test]
    fn resolve_event_name_returns_direct_events_as_is() {
        assert_eq!(resolve_event_name("pre_start"), "pre_start");
        assert_eq!(resolve_event_name("post_start"), "post_start");
        assert_eq!(resolve_event_name("pre_shutdown"), "pre_shutdown");
        assert_eq!(resolve_event_name("post_shutdown"), "post_shutdown");
    }

    #[test]
    fn resolve_event_name_returns_unknown_as_is() {
        assert_eq!(resolve_event_name("unknown_event"), "unknown_event");
    }
}
