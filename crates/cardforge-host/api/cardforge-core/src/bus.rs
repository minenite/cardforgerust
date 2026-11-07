//! Typed event bus with Bukkit-style priorities.

use cardforge_api::{Cancellable, Event, EventPriority};
use parking_lot::RwLock;
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

/// Opaque id returned when registering a listener (for later unregister).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ListenerId(u64);

type DynHandler = Arc<dyn Fn(&mut dyn Any) + Send + Sync>;

struct Entry {
    id: ListenerId,
    priority: EventPriority,
    plugin: String,
    handler: DynHandler,
}

/// Dispatches events to registered listeners in priority order.
#[derive(Default)]
pub struct EventBus {
    next_id: RwLock<u64>,
    by_type: RwLock<HashMap<TypeId, Vec<Entry>>>,
}

impl EventBus {
    /// Empty bus.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a listener for `E`. `plugin` is the owning plugin id (for logs).
    pub fn listen<E, F>(&self, plugin: impl Into<String>, priority: EventPriority, handler: F) -> ListenerId
    where
        E: Event + 'static,
        F: Fn(&mut E) + Send + Sync + 'static,
    {
        let id = {
            let mut n = self.next_id.write();
            *n += 1;
            ListenerId(*n)
        };
        let handler: DynHandler = Arc::new(move |any: &mut dyn Any| {
            if let Some(event) = any.downcast_mut::<E>() {
                handler(event);
            }
        });
        let entry = Entry {
            id,
            priority,
            plugin: plugin.into(),
            handler,
        };
        let mut map = self.by_type.write();
        let list = map.entry(TypeId::of::<E>()).or_default();
        list.push(entry);
        list.sort_by_key(|e| e.priority);
        id
    }

    /// Remove a previously registered listener.
    pub fn unlisten(&self, id: ListenerId) -> bool {
        let mut map = self.by_type.write();
        let mut removed = false;
        for list in map.values_mut() {
            let before = list.len();
            list.retain(|e| e.id != id);
            if list.len() != before {
                removed = true;
            }
        }
        removed
    }

    /// Fire an event to every listener for its type, in priority order.
    pub fn call<E: Event + 'static>(&self, event: &mut E) {
        let type_id = TypeId::of::<E>();
        let handlers: Vec<(String, DynHandler)> = {
            let map = self.by_type.read();
            map.get(&type_id)
                .map(|list| {
                    list.iter()
                        .map(|e| (e.plugin.clone(), Arc::clone(&e.handler)))
                        .collect()
                })
                .unwrap_or_default()
        };
        log::trace!(
            "cardforge event {} → {} listeners",
            event.name(),
            handlers.len()
        );
        for (plugin, handler) in handlers {
            log::trace!("cardforge event {} → plugin `{plugin}`", event.name());
            handler(event);
        }
    }

    /// Fire a cancellable event; returns whether it ended cancelled.
    pub fn call_cancellable<E: Event + Cancellable + 'static>(&self, event: &mut E) -> bool {
        self.call(event);
        event.is_cancelled()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cardforge_api::{Player, PlayerChatEvent, PlayerJoinEvent};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use uuid::Uuid;

    #[test]
    fn priorities_run_low_to_high() {
        let bus = EventBus::new();
        let order = Arc::new(parking_lot::Mutex::new(Vec::new()));
        for (prio, label) in [
            (EventPriority::High, "high"),
            (EventPriority::Lowest, "lowest"),
            (EventPriority::Normal, "normal"),
        ] {
            let order = Arc::clone(&order);
            bus.listen::<PlayerJoinEvent, _>("t", prio, move |_| {
                order.lock().push(label);
            });
        }
        let mut event = PlayerJoinEvent {
            player: Player::named(Uuid::nil(), "Ada"),
            join_message: None,
        };
        bus.call(&mut event);
        assert_eq!(order.lock().as_slice(), ["lowest", "normal", "high"]);
    }

    #[test]
    fn cancellable_chat() {
        let bus = EventBus::new();
        let hits = Arc::new(AtomicUsize::new(0));
        let hits2 = Arc::clone(&hits);
        bus.listen::<PlayerChatEvent, _>("filter", EventPriority::High, move |e| {
            hits2.fetch_add(1, Ordering::SeqCst);
            if e.message.contains("bad") {
                e.set_cancelled(true);
            }
        });
        let mut ok = PlayerChatEvent::new(Player::named(Uuid::nil(), "Ada"), "hello");
        assert!(!bus.call_cancellable(&mut ok));
        let mut bad = PlayerChatEvent::new(Player::named(Uuid::nil(), "Ada"), "bad word");
        assert!(bus.call_cancellable(&mut bad));
        assert_eq!(hits.load(Ordering::SeqCst), 2);
    }
}
