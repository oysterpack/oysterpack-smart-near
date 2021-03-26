use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env,
    serde::{Deserialize, Serialize},
};
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

    pub fn assert<F, Msg, MsgF>(&self, check: F, msg: MsgF)
    where
        F: FnOnce() -> bool,
        Msg: Display,
        MsgF: FnOnce() -> Msg,
    {
        if !check() {
            self.error(msg()).panic();
        }
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

    pub fn log(&self) {
        env::log(self.to_string().as_bytes())
    }
}

impl<Msg, T> Into<Result<T, Err>> for Error<Msg>
where
    Msg: Display,
{
    fn into(self) -> Result<T, Err> {
        Err(Err {
            code: self.0 .0.to_string(),
            msg: self.1.to_string(),
        })
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

    /// uses the supplied message instead of the preset message
    pub fn assert_with_message<F, MsgF, Msg>(&self, check: F, msg: MsgF)
    where
        F: FnOnce() -> bool,
        MsgF: FnOnce() -> Msg,
        Msg: Display,
    {
        self.0.assert(check, msg)
    }
}

#[derive(BorshSerialize, BorshDeserialize, Deserialize, Serialize, Debug, Clone, Eq, PartialEq)]
#[serde(crate = "near_sdk::serde")]
pub struct Err {
    pub code: String,
    pub msg: String,
}

impl Display for Err {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "[ERR] [{}] {}", self.code, self.msg)
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

    #[test]
    #[should_panic(expected = "BOOM")]
    fn error_code_assert() {
        let context = new_context("bob");
        testing_env!(context);

        const ERR: ErrCode = ErrCode("INVALID_ACCOUNT_ID");

        ERR.assert(|| false, || "BOOM");
    }

    #[test]
    fn into_result() {
        const ERR: ErrCode = ErrCode("INVALID_ACCOUNT_ID");
        fn foo() -> Result<u128, Err> {
            ERR.error("invalid").into()
        }

        match foo() {
            Ok(_) => panic!("expected Err"),
            Err(err) => println!("{}", err),
        }
    }
}
