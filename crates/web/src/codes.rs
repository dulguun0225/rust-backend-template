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
        /// A 400 the framework raised that no input code describes. A path variable, query parameter or header
        /// that does not convert is an entry of `validation.failed`, never this.
        BadRequest = ("validation.bad-request", 400),
        /// No such route, or no such resource.
        NotFound = ("not-found", 404),
        /// The route exists; the method does not.
        MethodNotAllowed = ("request.method-not-allowed", 405),
        /// The body is larger than the service reads.
        PayloadTooLarge {
            /// The most bytes a request body may have: the configured `REQUEST_BODY_MAX_BYTES`.
            max: u32,
        } = ("request.too-large", 413),
        /// A body that is not `application/json`.
        UnsupportedMediaType = ("request.unsupported-media-type", 415),
        /// An unexpected failure; the response carries only `incidentId`, the request's correlation id.
        Internal = ("platform.internal", 500),
    }
}

platform::field_codes! {
    /// Field errors the strict readers record: the body's members, and the path variables, query parameters
    /// and headers an operation declares.
    pub enum ApiFieldCode {
        /// A body member, or a query parameter, the operation does not declare; refused, never ignored.
        UnknownField {
            /// The members the enclosing object's type declares, or the operation's query parameters, sorted.
            allowed: &'static [&'static str],
        } = "validation.unknown-field",
        /// A top-level member named after one of the route's path variables: an identifier travels in the
        /// path only, so the body never carries it, whatever its value.
        IdentifierInPath = "validation.identifier-in-path",
        /// A member whose JSON type is not the declared one; `detail` says the same in words.
        WrongType {
            /// The declared JSON type: `string`, `boolean`, `integer` or `number`.
            expected: &'static str,
        } = "validation.wrong-type",
        /// Text, or a member of the right JSON type, that does not parse as its declared type or format, empty
        /// and blank included, such as a UUID not in its 36-character form; `detail` says the same in words.
        InvalidValue {
            /// The schema's `format`, else its `type`, e.g. `uuid` or `int32`.
            expected: &'static str,
        } = "validation.invalid-value",
        /// A value outside its declared enumeration.
        UnknownValue {
            /// The values the enumeration declares, sorted.
            allowed: &'static [&'static str],
        } = "validation.unknown-value",
        /// A required member, query parameter or header is absent: its name is not in the request.
        Required = "validation.required",
        /// A member appears twice in one object, at any depth, or a query parameter or single-valued header is
        /// given twice.
        DuplicateMember = "validation.duplicate-member",
    }
}
