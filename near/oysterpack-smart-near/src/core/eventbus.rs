//! Provides support for an eventbus for stateless [`EventHandler`] functions

/// Every [`Event`] type manages its own handlers
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

/// post event and run registered event handlers
pub fn post<T>(event: &T)
where
    T: Event,
{
    T::handlers(|x| x.post(event))
}

/// registers an event handler
pub fn register<T>(f: EventHandler<T>)
where
    T: Event,
{
    T::handlers_mut(|x| x.register_handler(f))
}

/// stateless event handler function
pub type EventHandler<T> = fn(&T);

/// Used to store registered event handlers
pub struct EventHandlers<T: Event + ?Sized>(Vec<EventHandler<T>>);

impl<T: Event + ?Sized> EventHandlers<T> {
    pub fn new() -> EventHandlers<T> {
        EventHandlers(vec![])
    }

    fn register_handler(&mut self, f: fn(&T)) {
        self.0.push(f);
    }

    #[inline]
    fn post(&self, event: &T) {
        self.0.iter().for_each(|f| f(event))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use lazy_static::lazy_static;
    use std::sync::Mutex;

    #[derive(Debug)]
    struct CountEvent(i32);

    #[derive(Debug)]
    struct StringEvent(String);

    type CountEventHandlers = EventHandlers<CountEvent>;
    type StringEventHandlers = EventHandlers<StringEvent>;

    fn create_count_handlers() -> CountEventHandlers {
        EventHandlers::new()
    }

    fn create_string_event_handlers() -> StringEventHandlers {
        EventHandlers::new()
    }

    // define events
    lazy_static! {
        static ref COUNT_EVENTS: Mutex<CountEventHandlers> = Mutex::new(create_count_handlers());
        static ref STRING_EVENTS: Mutex<StringEventHandlers> =
            Mutex::new(create_string_event_handlers());
    }

    // TODO: create macro for this boilerplate code
    impl Event for CountEvent {
        fn handlers<F>(f: F)
        where
            F: FnOnce(&EventHandlers<Self>),
        {
            f(&*COUNT_EVENTS.lock().unwrap())
        }

        fn handlers_mut<F>(f: F)
        where
            F: FnOnce(&mut EventHandlers<Self>),
        {
            f(&mut *COUNT_EVENTS.lock().unwrap())
        }
    }

    impl Event for StringEvent {
        fn handlers<F>(f: F)
        where
            F: FnOnce(&EventHandlers<Self>),
        {
            f(&*STRING_EVENTS.lock().unwrap())
        }

        fn handlers_mut<F>(f: F)
        where
            F: FnOnce(&mut EventHandlers<Self>),
        {
            f(&mut *STRING_EVENTS.lock().unwrap())
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
        register(count_event_handler_1);
        register(string_event_handler);
        post(&CountEvent(1));

        register(count_event_handler_2);
        post(&CountEvent(2));
        post(&CountEvent(3));

        post(&StringEvent("hello".to_string()));
    }
}
