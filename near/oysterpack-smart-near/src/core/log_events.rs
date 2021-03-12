use near_sdk::env;
use std::fmt::{self, Debug, Display, Formatter};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Level {
    INFO,
    WARN,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogEvent(pub Level, pub &'static str);

impl Display for LogEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "[{:?}] [{}]", self.0, self.1)
    }
}

impl LogEvent {
    pub fn log<Msg>(&self, msg: Msg)
    where
        Msg: Display,
    {
        env::log(format!("{} {}", self, msg).as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::{get_logs, test_env};

    #[test]
    fn event_display() {
        test_env::setup();
        LogEvent(Level::INFO, "FOO").log("message");
        println!("{:#?}", get_logs());
    }
}
