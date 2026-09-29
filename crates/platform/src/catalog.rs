//! The compile-checked wire-code catalogs. Every error response carries a `code` from an enum declared with
//! [`wire_errors!`](crate::wire_errors), and every field error inside a `validation.failed` carries a `code`
//! from an enum declared with [`field_codes!`](crate::field_codes). One declaration produces the enum, its
//! wire strings, its statuses and the rows the committed catalog snapshot is built from, so no variant can
//! exist without its row. Wire strings are immutable once shipped: clients branch on them.

/// A response-level error code: a stable machine string plus its HTTP status.
pub trait WireError: Copy + core::fmt::Debug + Send + Sync + 'static {
    /// The stable machine code, e.g. `not-found`.
    fn wire(self) -> &'static str;
    /// The HTTP status, 400 to 599, checked when the catalog compiles.
    fn status(self) -> u16;
}

/// A field-level validation code. Never a response on its own, so it carries no status: it is always an
/// entry of a `validation.failed` problem.
pub trait FieldCode: Copy + core::fmt::Debug + Send + Sync + 'static {
    /// The stable machine code, e.g. `validation.required`.
    fn wire(self) -> &'static str;
}

/// One row of the catalog snapshot.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CatalogRow {
    /// The wire string.
    pub wire: &'static str,
    /// The status, or `None` for a field code.
    pub status: Option<u16>,
    /// `Enum::Variant`, naming where the row is declared.
    pub entry: &'static str,
}

impl CatalogRow {
    /// The snapshot line: `wire -> status (Enum::Variant)`, `-` for a field code's status.
    #[must_use]
    pub fn line(&self) -> String {
        let status = self.status.map_or_else(|| "-".to_owned(), |s| s.to_string());
        format!("{} -> {} ({})", self.wire, status, self.entry)
    }
}

/// A catalog enum: its name and every row it declares.
pub trait Catalog {
    /// The enum's name.
    const NAME: &'static str;
    /// One row per variant, in declaration order.
    fn rows() -> Vec<CatalogRow>;
}

/// Declares a response-level error catalog. Each variant is `Name = ("wire-code", status)`.
///
/// ```
/// platform::wire_errors! {
///     /// The example feature's codes.
///     pub enum ExampleErrorCode {
///         /// No such example.
///         NotFound = ("not-found", 404),
///     }
/// }
/// ```
#[macro_export]
macro_rules! wire_errors {
    ($(#[$meta:meta])* $vis:vis enum $name:ident { $($(#[$vmeta:meta])* $variant:ident = ($wire:literal, $status:literal)),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        $vis enum $name { $($(#[$vmeta])* $variant),+ }

        const _: () = { $(assert!($status >= 400 && $status <= 599, "an error status is 4xx or 5xx");)+ };

        impl $crate::catalog::WireError for $name {
            fn wire(self) -> &'static str { match self { $(Self::$variant => $wire),+ } }
            fn status(self) -> u16 { match self { $(Self::$variant => $status),+ } }
        }

        impl $crate::catalog::Catalog for $name {
            const NAME: &'static str = stringify!($name);
            fn rows() -> ::std::vec::Vec<$crate::catalog::CatalogRow> {
                ::std::vec![$($crate::catalog::CatalogRow {
                    wire: $wire,
                    status: ::core::option::Option::Some($status),
                    entry: concat!(stringify!($name), "::", stringify!($variant)),
                }),+]
            }
        }

        impl $crate::log::Code for $name {
            fn code(&self) -> &'static str { $crate::catalog::WireError::wire(*self) }
        }
    };
}

/// Declares a field-level validation catalog. Each variant is `Name = "wire-code"`.
#[macro_export]
macro_rules! field_codes {
    ($(#[$meta:meta])* $vis:vis enum $name:ident { $($(#[$vmeta:meta])* $variant:ident = $wire:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        $vis enum $name { $($(#[$vmeta])* $variant),+ }

        impl $crate::catalog::FieldCode for $name {
            fn wire(self) -> &'static str { match self { $(Self::$variant => $wire),+ } }
        }

        impl $crate::catalog::Catalog for $name {
            const NAME: &'static str = stringify!($name);
            fn rows() -> ::std::vec::Vec<$crate::catalog::CatalogRow> {
                ::std::vec![$($crate::catalog::CatalogRow {
                    wire: $wire,
                    status: ::core::option::Option::None,
                    entry: concat!(stringify!($name), "::", stringify!($variant)),
                }),+]
            }
        }

        impl $crate::log::Code for $name {
            fn code(&self) -> &'static str { $crate::catalog::FieldCode::wire(*self) }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::{Catalog, CatalogRow, FieldCode, WireError};

    crate::wire_errors! {
        enum Probe {
            Gone = ("probe.gone", 410),
            Broken = ("probe.broken", 500),
        }
    }

    crate::field_codes! {
        enum ProbeField {
            Short = "probe.short",
        }
    }

    #[test]
    fn a_catalog_declares_its_wire_strings_statuses_and_rows_once() {
        assert_eq!(Probe::Gone.wire(), "probe.gone");
        assert_eq!(Probe::Broken.status(), 500);
        assert_eq!(ProbeField::Short.wire(), "probe.short");
        assert_eq!(Probe::NAME, "Probe");
        let lines: Vec<String> = Probe::rows().iter().chain(ProbeField::rows().iter()).map(CatalogRow::line).collect();
        assert_eq!(
            lines,
            [
                "probe.gone -> 410 (Probe::Gone)",
                "probe.broken -> 500 (Probe::Broken)",
                "probe.short -> - (ProbeField::Short)"
            ]
        );
    }
}
