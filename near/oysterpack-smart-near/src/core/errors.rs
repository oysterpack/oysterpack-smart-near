use std::fmt::{self, Display, Formatter};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ErrCode(pub &'static str);

impl Display for ErrCode {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "[{}]", self.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Error(pub ErrCode, pub String);

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.0, self.1)
    }
}

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

    const ERR_1: ErrCode = ErrCode("ERR_1");

    #[test]
    fn error_display() {
        println!("{}", Error(ERR_1, "BOOM".to_string()));
        println!("{}", ErrorConst(ERR_1, "BOOM"));
    }
}
