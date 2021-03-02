use near_sdk::PromiseOrValue;

/// Models a query function from a Request to a Response. Query functions perform read-only actions,
/// i.e., contract state must not be modified and remote calls are not permitted.
//
// The Query trait is a simplified interface making it easy to write network applications in a
// modular and reusable way, decoupled from the underlying protocol.
//
// It is one of OysterPack SMART's fundamental abstractions.
pub trait Query<Request> {
    type Response;
    type Error;

    fn view(&self, req: Request) -> Result<Self::Response, Self::Error>;
}

/// Models a command function from a Request to a Response.
//
// The Command trait is a simplified interface making it easy to write network applications in a
// modular and reusable way, decoupled from the underlying protocol.
//
// It is one of OysterPack SMART's fundamental abstractions.
pub trait Command<Request> {
    type Response;
    type Error;

    fn call(&mut self, req: Request) -> PromiseOrValue<Result<Self::Response, Self::Error>>;
}

/// Decorates a Service, transforming either the request or the response.
///
/// Often, many of the pieces needed for writing network applications can be reused across multiple services.
/// The Layer trait can be used to write reusable components that can be applied to very different kinds of services;
/// for example, it can be applied to services operating on different protocols, and to both the client
/// and server side of a network transaction.
pub trait Layer<S> {
    /// The wrapped service
    type Service;

    /// Wrap the given service with the middleware, returning a new service that has been decorated with the middleware.
    fn layer(&self, inner: S) -> Self::Service;
}
