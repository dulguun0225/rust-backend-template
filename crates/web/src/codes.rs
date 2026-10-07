//! The cross-cutting catalogs: errors raised below any single feature. Each feature declares its own
//! `*ErrorCode` and `*FieldCode` beside its handlers. The committed snapshot
//! (`snapshots/error-catalog.txt`, `crates/api/tests/catalog.rs`) turns any change into a reviewable diff.

platform::wire_errors! {
    /// Errors the platform raises for every endpoint.
    pub enum ApiErrorCode {
        /// The body is missing, is not JSON, or is not a JSON object; `detail` says which.
        MalformedBody = ("validation.malformed-body", 400),
        /// One or more field errors, each an entry of `errors`.
        ValidationFailed = ("validation.failed", 400),
        /// A request the router refused before any handler: a path segment of the wrong type, a bad query.
        BadRequest = ("validation.bad-request", 400),
        /// No such route, or no such resource.
        NotFound = ("not-found", 404),
        /// The route exists; the method does not.
        MethodNotAllowed = ("request.method-not-allowed", 405),
        /// The body is larger than the service accepts.
        PayloadTooLarge = ("request.too-large", 413),
        /// A body that is not `application/json`.
        UnsupportedMediaType = ("request.unsupported-media-type", 415),
        /// An unexpected failure; the response carries only `incidentId`, the request's correlation id.
        Internal = ("platform.internal", 500),
    }
}

platform::field_codes! {
    /// Field errors the strict body reader records for every body-taking endpoint.
    pub enum ApiFieldCode {
        /// A member the operation's request type does not declare; refused, never ignored.
        UnknownField = "validation.unknown-field",
        /// A top-level member named after one of the route's path variables: an identifier travels in the
        /// path only, so the body never carries it, whatever its value.
        IdentifierInPath = "validation.identifier-in-path",
        /// A member whose JSON type is not the declared one; `detail` says the same in words.
        WrongType {
            /// The declared JSON type: `string`, `boolean`, `integer` or `number`.
            expected: &'static str,
        } = "validation.wrong-type",
        /// A member of the right JSON type holding a value the type refuses, such as a malformed UUID.
        InvalidValue = "validation.invalid-value",
        /// A required member is absent.
        Required = "validation.required",
        /// A member appears twice in one object, at any depth; the pointer names the repeated member.
        DuplicateMember = "validation.duplicate-member",
    }
}
