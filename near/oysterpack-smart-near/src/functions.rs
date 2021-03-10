pub trait Query {
    type Request;
    type Response;
    type Error;

    fn call(&self, req: Self::Request) -> Result<Self::Response, Self::Error>;
}

pub trait Command {
    type Request;
    type Response;
    type Error;

    fn call(&mut self, req: Self::Request) -> Result<Self::Response, Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_trait() {}
}
