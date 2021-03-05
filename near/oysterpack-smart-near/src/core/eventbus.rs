//! Provides support for an [`EventBus'] for stateless [`EventHandler`] functions

use lazy_static::lazy_static;
use std::collections::HashMap;

lazy_static! {
    pub static ref EVENT_BUS: EventBus = EventBus;
}

/// stateless event handler function
pub type EventHandler<T> = fn(&T);

/// There is 1 [`EventBus`] per event type.
/// This maps each [`EventBus`] to its event handlers
pub struct EventHandlers<T: Event + ?Sized>(HashMap<&'static EventBus, Vec<EventHandler<T>>>);

impl<T: Event + ?Sized> EventHandlers<T> {
    pub fn new() -> EventHandlers<T> {
        EventHandlers(HashMap::new())
    }

    fn register_handler(&mut self, bus: &'static EventBus, f: fn(&T)) {
        let vec = self.0.entry(bus).or_insert_with(Vec::new);
        vec.push(f);
    }

    #[inline]
    fn post(&self, bus: &EventBus, event: &T) {
        self.0
            .get(bus)
            .iter()
            .flat_map(|x| x.iter())
            .for_each(|f| f(event))
    }
}

pub trait Event {
    /// enables access to the [`EventHandlers`] for this event type
    /// - used to get access to registered handlers
    fn handlers<F>(f: F)
    where
        F: FnOnce(&EventHandlers<Self>);

    /// enables mutable access to the [`EventHandlers`] for this event type
    /// - used to register handlers for this event type
    fn handlers_mut<F>(f: F)
    where
        F: FnOnce(&mut EventHandlers<Self>);
}

#[derive(PartialEq, Eq, Hash)]
pub struct EventBus;

impl EventBus {
    /// registers an event handler
    pub fn register<T>(&'static self, f: EventHandler<T>)
    where
        T: Event,
    {
        T::handlers_mut(|x| x.register_handler(self, f))
    }

    /// post event and run registered event handlers
    pub fn post<T>(&self, event: &T)
    where
        T: Event,
    {
        T::handlers(|x| x.post(self, event))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use lazy_static::lazy_static;
    use std::sync::RwLock;

    #[derive(Debug)]
    struct CountEvent(i32);

    #[derive(Debug)]
    struct StringEvent(String);

    // define events
    lazy_static! {
        static ref COUNT_EVENTS: RwLock<EventHandlers<CountEvent>> =
            RwLock::new(EventHandlers::new());
        static ref STRING_EVENTS: RwLock<EventHandlers<StringEvent>> =
            RwLock::new(EventHandlers::new());
    }

    // TODO: create macro for this boilerplate code
    impl Event for CountEvent {
        fn handlers<F>(f: F)
        where
            F: FnOnce(&EventHandlers<Self>),
        {
            f(&*COUNT_EVENTS.read().unwrap())
        }

        fn handlers_mut<F>(f: F)
        where
            F: FnOnce(&mut EventHandlers<Self>),
        {
            f(&mut *COUNT_EVENTS.write().unwrap())
        }
    }

    impl Event for StringEvent {
        fn handlers<F>(f: F)
        where
            F: FnOnce(&EventHandlers<Self>),
        {
            f(&*STRING_EVENTS.read().unwrap())
        }

        fn handlers_mut<F>(f: F)
        where
            F: FnOnce(&mut EventHandlers<Self>),
        {
            f(&mut *STRING_EVENTS.write().unwrap())
        }
    }

    fn count_event_handler_1(e: &CountEvent) {
        println!("count_event_handler_1 {:?}", e);
    }

    fn count_event_handler_2(e: &CountEvent) {
        println!("count_event_handler_2 {:?}", e);
    }

    fn string_event_handler(e: &StringEvent) {
        println!("string_event_handler {:?}", e);
    }

    #[test]
    fn eventbus() {
        EVENT_BUS.register(count_event_handler_1);
        EVENT_BUS.register(string_event_handler);
        EVENT_BUS.post(&CountEvent(1));

        EVENT_BUS.register(count_event_handler_2);
        EVENT_BUS.post(&CountEvent(2));
        EVENT_BUS.post(&CountEvent(3));

        EVENT_BUS.post(&StringEvent("hello".to_string()));
    }
}
