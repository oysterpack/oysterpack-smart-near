use near_sdk::env;
use std::fmt::{self, Display, Formatter};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ErrCode(pub &'static str);

impl Display for ErrCode {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "[ERR] [{}]", self.0)
    }
}

impl ErrCode {
    /// constructs an [`Error`] using this [`ErrCode`] and the specified message
    pub fn error<Msg: Display>(&self, msg: Msg) -> Error<Msg> {
        Error(*self, msg)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Error<Msg>(pub ErrCode, pub Msg)
where
    Msg: Display;

impl<Msg> Display for Error<Msg>
where
    Msg: Display,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.0, self.1)
    }
}

impl<Msg> Error<Msg>
where
    Msg: Display,
{
    pub fn panic(&self) {
        env::panic(self.to_string().as_bytes())
    }

    pub fn assert<F>(&self, check: F)
    where
        F: FnOnce() -> bool,
    {
        if !check() {
            self.panic();
        }
    }
}

/// Error that can be defined as a constant, i.e., the error message is constant
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ErrorConst(pub ErrCode, pub &'static str);

impl Display for ErrorConst {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.0, self.1)
    }
}

impl ErrorConst {
    pub fn panic(&self) {
        env::panic(self.to_string().as_bytes())
    }

    pub fn assert<F>(&self, check: F)
    where
        F: FnOnce() -> bool,
    {
        if !check() {
            self.panic();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oysterpack_smart_near_test::*;
    use regex::Regex;

    #[test]
    fn error_display() {
        const ERR: ErrCode = ErrCode("INVALID_ACCOUNT_ID");

        println!("{}", Error(ERR, "BOOM".to_string()));
        println!("{}", ErrorConst(ERR, "BOOM"));
        println!("{}", ERR.error("BOOM"));
    }

    #[test]
    fn err_display_format() {
        let err_fmt_regex = Regex::new(r"\[ERR] \[\w+] \w+").unwrap();

        const ERR: ErrCode = ErrCode("INVALID_ACCOUNT_ID");
        let err = ERR.error("BOOM");
        println!("{}", ERR.error("BOOM"));

        assert!(err_fmt_regex.is_match(&err.to_string()));
    }

    #[test]
    #[should_panic(expected = "BOOM")]
    fn error_panic() {
        let context = new_context("bob");
        testing_env!(context);

        const ERR_CODE: ErrCode = ErrCode("INVALID_ACCOUNT_ID");
        let err: Error<String> = ERR_CODE.error("BOOM".to_string());
        err.panic();
    }

    #[test]
    #[should_panic(expected = "BOOM")]
    fn error_const_panic() {
        let context = new_context("bob");
        testing_env!(context);

        const ERR_CODE: ErrCode = ErrCode("INVALID_ACCOUNT_ID");
        const ERR: ErrorConst = ErrorConst(ERR_CODE, "BOOM");
        ERR.panic();
    }
}
