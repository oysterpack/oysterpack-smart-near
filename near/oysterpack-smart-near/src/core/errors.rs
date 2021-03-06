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

/// Error that can be defined as a constant, i.e., the error message is constant
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ErrorConst(pub ErrCode, pub &'static str);

impl Display for ErrorConst {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.0, self.1)
    }
}

#[cfg(test)]
mod test {
    use super::*;
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
}
