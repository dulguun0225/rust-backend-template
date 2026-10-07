//! The greeting feature's catalogs.

platform::wire_errors! {
    /// The greeting feature's response codes. One wire string maps to one status across every catalog.
    pub enum GreetingErrorCode {
        /// No greeting has that id.
        NotFound = ("not-found", 404),
    }
}

platform::field_codes! {
    /// The greeting feature's field rules.
    pub enum GreetingFieldCode {
        /// The name is empty or only whitespace.
        Required = "validation.required",
        /// The name is longer than `max` characters.
        TooLong {
            /// The most characters a name may have: `NAME_MAX_CHARS`.
            max: u16,
        } = "validation.too-long",
    }
}
