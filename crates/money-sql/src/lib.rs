//! The one mapper between a `NUMERIC(p,s)` column and [`Money`] (`money-storage` `M-10`, `M-37`).
//!
//! sqlx maps `NUMERIC` only to `BigDecimal` and `rust_decimal`, and each rounds silently somewhere, so the
//! column's value crosses the boundary as a [`DbAmount`] and becomes `Money` here, failing loud when it has
//! digits past the currency's minor unit or does not fit an `i64`. `clippy.toml` bans `BigDecimal` in every
//! other crate, and `crates/store/sqlx.toml` makes every `NUMERIC` in a query macro decode as `DbAmount`.

use bigdecimal::BigDecimal;
use bigdecimal::num_bigint::BigInt;
use money::{Currency, Money};
use sqlx::Postgres;
use sqlx::postgres::{PgArgumentBuffer, PgTypeInfo, PgValueRef};

/// The most integer digits an `i64` of minor units can hold at exponent zero.
const MAX_INTEGER_DIGITS: i64 = 19;

/// A `NUMERIC` value in transit. Its field is private: nothing outside this crate can compute with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbAmount(BigDecimal);

/// Why a column value is not an amount in the currency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MapError {
    /// Nonzero digits past the currency's minor unit, e.g. `12.345` for USD.
    #[error("the column holds more decimal places than the currency's minor unit")]
    ExcessPrecision,
    /// The amount does not fit an `i64` of minor units.
    #[error("the column holds an amount out of range")]
    Overflow,
}

impl DbAmount {
    /// The amount at the currency's scale: 1234 minor USD is `12.34`.
    #[must_use]
    pub fn from_money(money: Money) -> Self {
        Self(BigDecimal::new(BigInt::from(money.minor()), i64::from(money.currency().exponent())))
    }

    /// The amount in `currency`'s minor units. Trailing zeros past the minor unit are accepted (`12.3400` in a
    /// `NUMERIC(19,4)` column is 1234 minor USD); any other digit there is refused, never rounded.
    ///
    /// # Errors
    /// [`MapError::ExcessPrecision`] or [`MapError::Overflow`].
    pub fn into_money(self, currency: Currency) -> Result<Money, MapError> {
        let exponent = i64::from(currency.exponent());
        let normalized = self.0.normalized();
        let (_, scale) = normalized.as_bigint_and_exponent();
        if scale > exponent {
            return Err(MapError::ExcessPrecision);
        }
        // Checked before rescaling: a value such as 1e999999 would otherwise be multiplied out in full.
        let digits = i64::try_from(normalized.digits()).map_err(|_| MapError::Overflow)?;
        if digits.checked_sub(scale).is_none_or(|integer_digits| integer_digits > MAX_INTEGER_DIGITS) {
            return Err(MapError::Overflow);
        }
        let (minor, _) = normalized.with_scale(exponent).into_bigint_and_exponent();
        let minor = i64::try_from(&minor).map_err(|_| MapError::Overflow)?;
        Ok(Money::from_minor(minor, currency))
    }
}

impl sqlx::Type<Postgres> for DbAmount {
    fn type_info() -> PgTypeInfo {
        <BigDecimal as sqlx::Type<Postgres>>::type_info()
    }
}

impl<'r> sqlx::Decode<'r, Postgres> for DbAmount {
    fn decode(value: PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        Ok(Self(<BigDecimal as sqlx::Decode<'r, Postgres>>::decode(value)?))
    }
}

impl sqlx::Encode<'_, Postgres> for DbAmount {
    fn encode_by_ref(&self, buf: &mut PgArgumentBuffer) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        <BigDecimal as sqlx::Encode<'_, Postgres>>::encode_by_ref(&self.0, buf)
    }
}

#[cfg(test)]
mod tests {
    use super::{DbAmount, MapError};
    use bigdecimal::BigDecimal;
    use money::{Currency, Money};
    use std::str::FromStr as _;

    fn amount(text: &str) -> DbAmount {
        DbAmount(BigDecimal::from_str(text).unwrap())
    }

    fn usd() -> Currency {
        Currency::from_code("USD").unwrap()
    }

    #[test]
    fn trailing_zeros_past_the_minor_unit_are_accepted() {
        assert_eq!(amount("12.3400").into_money(usd()), Ok(Money::from_minor(1234, usd())));
        assert_eq!(amount("-0.0100").into_money(usd()), Ok(Money::from_minor(-1, usd())));
        assert_eq!(amount("0.0000").into_money(usd()), Ok(Money::zero(usd())));
        assert_eq!(amount("12").into_money(usd()), Ok(Money::from_minor(1200, usd())));
    }

    #[test]
    fn a_digit_past_the_minor_unit_is_refused_not_rounded() {
        assert_eq!(amount("12.3450").into_money(usd()), Err(MapError::ExcessPrecision));
        assert_eq!(amount("0.0001").into_money(usd()), Err(MapError::ExcessPrecision));
        assert_eq!(amount("1.5").into_money(Currency::from_code("JPY").unwrap()), Err(MapError::ExcessPrecision));
    }

    #[test]
    fn an_amount_beyond_i64_minor_units_is_refused() {
        assert_eq!(amount("92233720368547758.08").into_money(usd()), Err(MapError::Overflow));
        assert_eq!(amount("-92233720368547758.08").into_money(usd()), Ok(Money::from_minor(i64::MIN, usd())));
        // Refused before any rescaling: rescaling this one would allocate a billion-digit integer.
        assert_eq!(amount("1e999999999").into_money(usd()), Err(MapError::Overflow));
        assert_eq!(amount("12345678901234567890123").into_money(usd()), Err(MapError::Overflow));
    }

    #[test]
    fn nineteen_integer_digits_fit_and_twenty_do_not() {
        let jpy = Currency::from_code("JPY").unwrap();
        assert_eq!(amount("9223372036854775807").into_money(jpy), Ok(Money::from_minor(i64::MAX, jpy)));
        assert_eq!(amount("9223372036854775808").into_money(jpy), Err(MapError::Overflow));
        assert_eq!(amount("10000000000000000000").into_money(jpy), Err(MapError::Overflow));
    }

    #[test]
    fn from_money_writes_the_currency_scale() {
        assert_eq!(DbAmount::from_money(Money::from_minor(1234, usd())), amount("12.34"));
        let kwd = Currency::from_code("KWD").unwrap();
        assert_eq!(DbAmount::from_money(Money::from_minor(-5, kwd)), amount("-0.005"));
    }
}
