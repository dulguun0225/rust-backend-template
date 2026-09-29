//! ISO 4217 currencies and their minor-unit exponents. The table is a starting set, not the standard's full
//! list: a service adds the codes it trades, with the exponent ISO 4217 publishes, in one commit that says why.

use crate::MoneyError;

/// A currency: its ISO 4217 code and the number of decimal places of its minor unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Currency {
    code: &'static str,
    exponent: u8,
}

/// `(code, exponent)`, ISO 4217.
const TABLE: &[(&str, u8)] = &[
    ("AED", 2),
    ("AUD", 2),
    ("BHD", 3),
    ("BRL", 2),
    ("CAD", 2),
    ("CHF", 2),
    ("CLF", 4),
    ("CLP", 0),
    ("CNY", 2),
    ("CZK", 2),
    ("DKK", 2),
    ("EUR", 2),
    ("GBP", 2),
    ("HKD", 2),
    ("HUF", 2),
    ("IDR", 2),
    ("ILS", 2),
    ("INR", 2),
    ("IQD", 3),
    ("ISK", 0),
    ("JOD", 3),
    ("JPY", 0),
    ("KRW", 0),
    ("KWD", 3),
    ("KZT", 2),
    ("LYD", 3),
    ("MNT", 2),
    ("MXN", 2),
    ("NOK", 2),
    ("NZD", 2),
    ("OMR", 3),
    ("PLN", 2),
    ("SEK", 2),
    ("SGD", 2),
    ("THB", 2),
    ("TND", 3),
    ("TRY", 2),
    ("USD", 2),
    ("UYW", 4),
    ("VND", 0),
    ("ZAR", 2),
];

impl Currency {
    /// The currency with this ISO 4217 code, upper case.
    ///
    /// # Errors
    /// [`MoneyError::UnknownCurrency`] for a code the table does not hold.
    pub fn from_code(code: &str) -> Result<Self, MoneyError> {
        TABLE
            .iter()
            .find(|(c, _)| *c == code)
            .map(|&(code, exponent)| Self { code, exponent })
            .ok_or(MoneyError::UnknownCurrency)
    }

    /// The ISO 4217 code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.code
    }

    /// Decimal places of the minor unit: 2 for USD, 0 for JPY, 3 for KWD.
    #[must_use]
    pub const fn exponent(&self) -> u8 {
        self.exponent
    }
}

#[cfg(test)]
mod tests {
    use super::{Currency, TABLE};
    use crate::MoneyError;

    #[test]
    fn the_table_is_sorted_unique_upper_case_and_at_most_four_places() {
        assert!(TABLE.windows(2).all(|pair| pair[0].0 < pair[1].0));
        assert!(
            TABLE.iter().all(|(code, exponent)| code.len() == 3
                && code.bytes().all(|b| b.is_ascii_uppercase())
                && *exponent <= 4)
        );
    }

    #[test]
    fn an_unknown_or_lower_case_code_is_refused() {
        assert_eq!(Currency::from_code("usd"), Err(MoneyError::UnknownCurrency));
        assert_eq!(Currency::from_code("XXX"), Err(MoneyError::UnknownCurrency));
        assert_eq!(Currency::from_code("KWD").map(|c| c.exponent()), Ok(3));
    }
}
