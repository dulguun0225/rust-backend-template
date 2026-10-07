//! The compile-checked wire-code catalogs. Every error response carries a `code` from an enum declared with
//! [`wire_errors!`](crate::wire_errors), and every field error inside a `validation.failed` carries a `code`
//! from an enum declared with [`field_codes!`](crate::field_codes). One declaration produces the enum, its
//! wire strings, its statuses and the rows the committed catalog snapshot is built from, so no variant can
//! exist without its row. Wire strings are immutable once shipped: clients branch on them.
//!
//! A code declares its params — what the caller is told is allowed, such as the maximum length or the members
//! a type declares — as the fields of its variant, each of a [`Param`] type. The variant cannot be written
//! without them, so a code is never raised without its declared params, and the snapshot lists each code's
//! param names and types. A field code's params travel in its entry; a response code's on the problem itself.

/// A response-level error code: a stable machine string plus its HTTP status.
pub trait WireError: Copy + core::fmt::Debug + Send + Sync + 'static {
    /// The stable machine code, e.g. `not-found`.
    fn wire(self) -> &'static str;
    /// The HTTP status, 400 to 599, checked when the catalog compiles.
    fn status(self) -> u16;
    /// The declared params with the values this instance carries, in declaration order.
    fn params(self) -> Vec<(&'static str, ParamValue)>;
}

/// A field-level validation code. Never a response on its own, so it carries no status: it is always an
/// entry of a `validation.failed` problem.
pub trait FieldCode: Copy + core::fmt::Debug + Send + Sync + 'static {
    /// The stable machine code, e.g. `validation.required`.
    fn wire(self) -> &'static str;
    /// The declared params with the values this instance carries, in declaration order.
    fn params(self) -> Vec<(&'static str, ParamValue)>;
}

/// A code's param value as it goes on the wire: a JSON integer, a JSON string or a JSON array of strings.
/// Every string is `&'static`, so no param is ever built from the value sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ParamValue {
    /// A JSON integer.
    Integer(i64),
    /// A JSON string, fixed in the source.
    Text(&'static str),
    /// A JSON array of strings, fixed in the source: the members or values a type declares.
    List(&'static [&'static str]),
}

/// A type a code's param may have, with the JSON type the snapshot lists for it.
pub trait Param: Copy {
    /// The JSON type of the param on the wire: `integer`, `string` or `array`.
    const WIRE_TYPE: &'static str;
    /// The value as it goes on the wire.
    fn value(self) -> ParamValue;
}

macro_rules! integer_params {
    ($($t:ty),+) => {$(
        impl Param for $t {
            const WIRE_TYPE: &'static str = "integer";
            fn value(self) -> ParamValue {
                ParamValue::Integer(i64::from(self))
            }
        }
    )+};
}

integer_params!(i8, i16, i32, i64, u8, u16, u32);

impl Param for &'static str {
    const WIRE_TYPE: &'static str = "string";
    fn value(self) -> ParamValue {
        ParamValue::Text(self)
    }
}

impl Param for &'static [&'static str] {
    const WIRE_TYPE: &'static str = "array";
    fn value(self) -> ParamValue {
        ParamValue::List(self)
    }
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
    /// The code's declared params, `(name, JSON type)` in declaration order.
    pub params: &'static [(&'static str, &'static str)],
}

impl CatalogRow {
    /// The snapshot line: `wire -> status (Enum::Variant) {name: type, ...}`, `-` for a field code's status and
    /// `{}` when the code declares no params.
    #[must_use]
    pub fn line(&self) -> String {
        let status = self.status.map_or_else(|| "-".to_owned(), |s| s.to_string());
        let listed: Vec<String> = self.params.iter().map(|(name, kind)| format!("{name}: {kind}")).collect();
        format!("{} -> {} ({}) {{{}}}", self.wire, status, self.entry, listed.join(", "))
    }
}

/// A catalog enum: its name and every row it declares.
pub trait Catalog {
    /// The enum's name.
    const NAME: &'static str;
    /// One row per variant, in declaration order.
    fn rows() -> Vec<CatalogRow>;
}

/// Declares a response-level error catalog. Each variant is `Name = ("wire-code", status)`, or, when the code
/// tells the caller what is allowed, `Name { param: Type, ... } = ("wire-code", status)` with each `Type` a
/// [`Param`]; the params travel on the problem itself.
///
/// ```
/// platform::wire_errors! {
///     /// The example feature's codes.
///     pub enum ExampleErrorCode {
///         /// No such example.
///         NotFound = ("not-found", 404),
///         /// The upload is larger than `max` bytes.
///         TooLarge { max: u32 } = ("example.too-large", 413),
///     }
/// }
/// use platform::catalog::{ParamValue, WireError};
/// assert_eq!(ExampleErrorCode::TooLarge { max: 9 }.params(), [("max", ParamValue::Integer(9))]);
/// ```
#[macro_export]
macro_rules! wire_errors {
    (
        $(#[$meta:meta])* $vis:vis enum $name:ident {
            $(
                $(#[$vmeta:meta])* $variant:ident
                $({ $($(#[$pmeta:meta])* $param:ident : $ty:ty),+ $(,)? })?
                = ($wire:literal, $status:literal)
            ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        $vis enum $name { $($(#[$vmeta])* $variant $({ $($(#[$pmeta])* $param: $ty),+ })?),+ }

        const _: () = { $(assert!($status >= 400 && $status <= 599, "an error status is 4xx or 5xx");)+ };

        impl $crate::catalog::WireError for $name {
            fn wire(self) -> &'static str { match self { $(Self::$variant { .. } => $wire),+ } }
            fn status(self) -> u16 { match self { $(Self::$variant { .. } => $status),+ } }
            fn params(self) -> ::std::vec::Vec<(&'static str, $crate::catalog::ParamValue)> {
                match self {
                    $(Self::$variant { $($($param),+)? } => ::std::vec![
                        $($((stringify!($param), $crate::catalog::Param::value($param))),+)?
                    ]),+
                }
            }
        }

        impl $crate::catalog::Catalog for $name {
            const NAME: &'static str = stringify!($name);
            fn rows() -> ::std::vec::Vec<$crate::catalog::CatalogRow> {
                ::std::vec![$($crate::catalog::CatalogRow {
                    wire: $wire,
                    status: ::core::option::Option::Some($status),
                    entry: concat!(stringify!($name), "::", stringify!($variant)),
                    params: const {
                        &[$($((stringify!($param), <$ty as $crate::catalog::Param>::WIRE_TYPE)),+)?]
                    },
                }),+]
            }
        }

        impl $crate::log::Code for $name {
            fn code(&self) -> &'static str { $crate::catalog::WireError::wire(*self) }
        }
    };
}

/// Declares a field-level validation catalog. Each variant is `Name = "wire-code"`, or, when the code tells
/// the caller what is allowed, `Name { param: Type, ... } = "wire-code"` with each `Type` a [`Param`]. A
/// param's name is its wire name, so it is one lower-case word (`crates/api/tests/api/catalog.rs` holds that).
///
/// ```
/// platform::field_codes! {
///     /// The example feature's field rules.
///     pub enum ExampleFieldCode {
///         /// The member is absent.
///         Required = "validation.required",
///         /// The text is longer than `max` characters.
///         TooLong { max: u16 } = "validation.too-long",
///     }
/// }
/// use platform::catalog::{FieldCode, ParamValue};
/// assert_eq!(ExampleFieldCode::TooLong { max: 100 }.params(), [("max", ParamValue::Integer(100))]);
/// ```
///
/// A variant that declares params is a struct variant, so naming it without them does not compile (E0533,
/// or E0063 for a param left out).
#[macro_export]
macro_rules! field_codes {
    (
        $(#[$meta:meta])* $vis:vis enum $name:ident {
            $(
                $(#[$vmeta:meta])* $variant:ident
                $({ $($(#[$pmeta:meta])* $param:ident : $ty:ty),+ $(,)? })?
                = $wire:literal
            ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        $vis enum $name { $($(#[$vmeta])* $variant $({ $($(#[$pmeta])* $param: $ty),+ })?),+ }

        impl $crate::catalog::FieldCode for $name {
            fn wire(self) -> &'static str { match self { $(Self::$variant { .. } => $wire),+ } }
            fn params(self) -> ::std::vec::Vec<(&'static str, $crate::catalog::ParamValue)> {
                match self {
                    $(Self::$variant { $($($param),+)? } => ::std::vec![
                        $($((stringify!($param), $crate::catalog::Param::value($param))),+)?
                    ]),+
                }
            }
        }

        impl $crate::catalog::Catalog for $name {
            const NAME: &'static str = stringify!($name);
            fn rows() -> ::std::vec::Vec<$crate::catalog::CatalogRow> {
                ::std::vec![$($crate::catalog::CatalogRow {
                    wire: $wire,
                    status: ::core::option::Option::None,
                    entry: concat!(stringify!($name), "::", stringify!($variant)),
                    params: const {
                        &[$($((stringify!($param), <$ty as $crate::catalog::Param>::WIRE_TYPE)),+)?]
                    },
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
    use super::{Catalog, CatalogRow, FieldCode, Param, ParamValue, WireError};

    crate::wire_errors! {
        enum Probe {
            Gone = ("probe.gone", 410),
            Broken = ("probe.broken", 500),
            Big { max: u32 } = ("probe.big", 413),
        }
    }

    crate::field_codes! {
        enum ProbeField {
            Short = "probe.short",
            Long { max: u16, unit: &'static str } = "probe.long",
            Unknown { allowed: &'static [&'static str] } = "probe.unknown",
        }
    }

    #[test]
    fn a_catalog_declares_its_wire_strings_statuses_and_rows_once() {
        assert_eq!(Probe::Gone.wire(), "probe.gone");
        assert_eq!(Probe::Broken.status(), 500);
        assert_eq!((Probe::Big { max: 1 }.wire(), Probe::Big { max: 1 }.status()), ("probe.big", 413));
        assert_eq!(ProbeField::Short.wire(), "probe.short");
        assert_eq!(ProbeField::Long { max: 3, unit: "characters" }.wire(), "probe.long");
        assert_eq!(Probe::NAME, "Probe");
        assert_eq!(ProbeField::NAME, "ProbeField");
        let lines: Vec<String> = Probe::rows().iter().chain(ProbeField::rows().iter()).map(CatalogRow::line).collect();
        assert_eq!(
            lines,
            [
                "probe.gone -> 410 (Probe::Gone) {}",
                "probe.broken -> 500 (Probe::Broken) {}",
                "probe.big -> 413 (Probe::Big) {max: integer}",
                "probe.short -> - (ProbeField::Short) {}",
                "probe.long -> - (ProbeField::Long) {max: integer, unit: string}",
                "probe.unknown -> - (ProbeField::Unknown) {allowed: array}",
            ]
        );
    }

    #[test]
    fn a_code_carries_the_params_it_declares() {
        assert_eq!(Probe::Gone.params(), []);
        assert_eq!(Probe::Big { max: 7 }.params(), [("max", ParamValue::Integer(7))]);
        assert_eq!(ProbeField::Unknown { allowed: &["a", "b"] }.params(), [("allowed", ParamValue::List(&["a", "b"]))]);
        assert_eq!(ProbeField::Short.params(), []);
        assert_eq!(
            ProbeField::Long { max: 3, unit: "characters" }.params(),
            [("max", ParamValue::Integer(3)), ("unit", ParamValue::Text("characters"))]
        );
    }

    #[test]
    fn every_param_type_states_its_wire_type_and_value() {
        assert_eq!((i8::WIRE_TYPE, (-8_i8).value()), ("integer", ParamValue::Integer(-8)));
        assert_eq!((i16::WIRE_TYPE, (-16_i16).value()), ("integer", ParamValue::Integer(-16)));
        assert_eq!((i32::WIRE_TYPE, (-32_i32).value()), ("integer", ParamValue::Integer(-32)));
        assert_eq!((i64::WIRE_TYPE, i64::MIN.value()), ("integer", ParamValue::Integer(i64::MIN)));
        assert_eq!((u8::WIRE_TYPE, 8_u8.value()), ("integer", ParamValue::Integer(8)));
        assert_eq!((u16::WIRE_TYPE, 16_u16.value()), ("integer", ParamValue::Integer(16)));
        assert_eq!((u32::WIRE_TYPE, u32::MAX.value()), ("integer", ParamValue::Integer(4_294_967_295)));
        assert_eq!((<&str>::WIRE_TYPE, "x".value()), ("string", ParamValue::Text("x")));
        let list: &'static [&'static str] = &["x"];
        assert_eq!((<&[&str]>::WIRE_TYPE, list.value()), ("array", ParamValue::List(&["x"])));
    }
}
