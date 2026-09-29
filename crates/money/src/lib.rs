//! Money: a whole number of a currency's minor units, `Money { minor: i64, currency }`.
//!
//! Every decimal crate assessed for Rust (rust_decimal, bigdecimal, fastnum, rusty-money) rounds silently
//! somewhere, so amounts are integers and the only division is [`mul_div_round`], which names its rounding
//! mode at every call (`money` `M-7`). There is no operator on [`Money`]: `money * money` is E0369, and every
//! method that can fail returns a `Result` the caller must use. `clippy.toml` bans integer division and
//! `Iterator::sum` everywhere; this crate's own `clippy.toml` exempts only the two `i128` division methods
//! `mul_div_round` needs.
//!
//! In a column an amount is `NUMERIC(p,s)`, read and written by `money_sql::DbAmount` alone (`money-storage`
//! `M-10`, `M-37`); on the wire it is a decimal string ([`Money::to_decimal_string`], [`Money::parse`]).

mod currency;

pub use currency::Currency;

/// Why a money operation refused to produce a value. Nothing here rounds or wraps silently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MoneyError {
    /// The code is not in [`Currency`]'s table.
    #[error("unknown currency")]
    UnknownCurrency,
    /// The two amounts are in different currencies.
    #[error("currency mismatch")]
    CurrencyMismatch,
    /// The result does not fit an `i64` of minor units.
    #[error("amount out of range")]
    Overflow,
    /// The amount has digits past the currency's minor unit.
    #[error("more decimal places than the currency's minor unit")]
    ExcessPrecision,
    /// The text is not a plain decimal: an optional `-`, digits, an optional `.` and digits.
    #[error("not a decimal amount")]
    Malformed,
    /// A ratio with a zero denominator.
    #[error("division by zero")]
    DivisionByZero,
}

/// How a result between two minor units is rounded. Named at every call; there is no default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundingMode {
    /// Away from zero.
    Up,
    /// Toward zero.
    Down,
    /// Toward positive infinity.
    Ceiling,
    /// Toward negative infinity.
    Floor,
    /// To nearest; a tie goes away from zero.
    HalfUp,
    /// To nearest; a tie goes toward zero.
    HalfDown,
    /// To nearest; a tie goes to the even neighbour.
    HalfEven,
}

/// The closed set of occasions on which money is rounded. Extend it as the domain acquires occasions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundingOccasion {
    /// A percentage or formula fee applied to an amount.
    FeePercent,
    /// A tax computed on an amount.
    Tax,
    /// A currency conversion.
    FxConversion,
    /// Splitting an amount across parts.
    Allocation,
}

/// Maps every [`RoundingOccasion`] to a [`RoundingMode`]. One field per occasion, so a new occasion fails to
/// compile until every policy names its mode: no silent default at lookup time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoundingPolicy {
    /// Mode for [`RoundingOccasion::FeePercent`].
    pub fee_percent: RoundingMode,
    /// Mode for [`RoundingOccasion::Tax`].
    pub tax: RoundingMode,
    /// Mode for [`RoundingOccasion::FxConversion`].
    pub fx_conversion: RoundingMode,
    /// Mode for [`RoundingOccasion::Allocation`].
    pub allocation: RoundingMode,
}

impl RoundingPolicy {
    /// Half-up for every occasion, the usual commercial default; override per field.
    #[must_use]
    pub const fn half_up_everywhere() -> Self {
        Self {
            fee_percent: RoundingMode::HalfUp,
            tax: RoundingMode::HalfUp,
            fx_conversion: RoundingMode::HalfUp,
            allocation: RoundingMode::HalfUp,
        }
    }

    /// The mode for an occasion.
    #[must_use]
    pub const fn mode(&self, occasion: RoundingOccasion) -> RoundingMode {
        match occasion {
            RoundingOccasion::FeePercent => self.fee_percent,
            RoundingOccasion::Tax => self.tax,
            RoundingOccasion::FxConversion => self.fx_conversion,
            RoundingOccasion::Allocation => self.allocation,
        }
    }
}

/// `round(amount × numerator ÷ denominator)` with an explicit mode. The product is exact in `i128` and rounded
/// once, on the division. `None` when the denominator is zero or the product overflows `i128`.
#[must_use]
pub fn mul_div_round(amount: i128, numerator: i128, denominator: i128, mode: RoundingMode) -> Option<i128> {
    if denominator == 0 {
        return None;
    }
    let product = amount.checked_mul(numerator)?;
    let (p, d) =
        if denominator < 0 { (product.checked_neg()?, denominator.checked_neg()?) } else { (product, denominator) };
    let quotient = p.checked_div(d)?;
    let remainder = p.checked_rem(d)?;
    if remainder == 0 {
        return Some(quotient);
    }
    let positive = p > 0;
    let away = if positive { quotient.checked_add(1)? } else { quotient.checked_sub(1)? };
    let twice = remainder.checked_abs()?.checked_mul(2)?;
    let rounded = match mode {
        RoundingMode::Down => quotient,
        RoundingMode::Up => away,
        RoundingMode::Floor => {
            if positive {
                quotient
            } else {
                away
            }
        }
        RoundingMode::Ceiling => {
            if positive {
                away
            } else {
                quotient
            }
        }
        RoundingMode::HalfUp => {
            if twice >= d {
                away
            } else {
                quotient
            }
        }
        RoundingMode::HalfDown => {
            if twice > d {
                away
            } else {
                quotient
            }
        }
        RoundingMode::HalfEven => match twice.cmp(&d) {
            core::cmp::Ordering::Greater => away,
            core::cmp::Ordering::Less => quotient,
            core::cmp::Ordering::Equal => {
                if quotient.checked_rem(2)? == 0 {
                    quotient
                } else {
                    away
                }
            }
        },
    };
    Some(rounded)
}

/// An amount: a whole number of `currency`'s minor units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Money {
    minor: i64,
    currency: Currency,
}

impl Money {
    /// `minor` minor units of `currency`: `Money::from_minor(1234, usd)` is 12.34 USD.
    #[must_use]
    pub const fn from_minor(minor: i64, currency: Currency) -> Self {
        Self { minor, currency }
    }

    /// Zero in `currency`.
    #[must_use]
    pub const fn zero(currency: Currency) -> Self {
        Self { minor: 0, currency }
    }

    /// Parses a plain decimal string, the wire form (`money` `M-12`): an optional `-`, digits, and an optional
    /// `.` with at most the currency's minor-unit digits. Excess precision is refused, never rounded.
    ///
    /// # Errors
    /// [`MoneyError::Malformed`], [`MoneyError::ExcessPrecision`] or [`MoneyError::Overflow`].
    pub fn parse(text: &str, currency: Currency) -> Result<Self, MoneyError> {
        let (negative, unsigned) = match text.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, text),
        };
        let (whole, fraction) = match unsigned.split_once('.') {
            Some((whole, fraction)) if !fraction.is_empty() => (whole, fraction),
            Some(_) => return Err(MoneyError::Malformed),
            None => (unsigned, ""),
        };
        if whole.is_empty()
            || !whole.bytes().all(|b| b.is_ascii_digit())
            || !fraction.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(MoneyError::Malformed);
        }
        let exponent = usize::from(currency.exponent());
        if fraction.len() > exponent {
            return Err(MoneyError::ExcessPrecision);
        }
        let padding = exponent.checked_sub(fraction.len()).ok_or(MoneyError::ExcessPrecision)?;
        // Accumulated with the sign applied digit by digit, so i64::MIN, which has no positive twin, parses.
        let mut minor: i64 = 0;
        for digit in whole.bytes().chain(fraction.bytes()).chain(core::iter::repeat_n(b'0', padding)) {
            let value = i64::from(digit.checked_sub(b'0').ok_or(MoneyError::Malformed)?);
            let shifted = minor.checked_mul(10).ok_or(MoneyError::Overflow)?;
            minor = if negative { shifted.checked_sub(value) } else { shifted.checked_add(value) }
                .ok_or(MoneyError::Overflow)?;
        }
        Ok(Self { minor, currency })
    }

    /// The amount in minor units.
    #[must_use]
    pub const fn minor(&self) -> i64 {
        self.minor
    }

    /// The currency.
    #[must_use]
    pub const fn currency(&self) -> Currency {
        self.currency
    }

    /// True below zero.
    #[must_use]
    pub const fn is_negative(&self) -> bool {
        self.minor < 0
    }

    /// True at zero.
    #[must_use]
    pub const fn is_zero(&self) -> bool {
        self.minor == 0
    }

    /// The sum of two amounts in the same currency.
    ///
    /// # Errors
    /// [`MoneyError::CurrencyMismatch`] or [`MoneyError::Overflow`].
    pub fn checked_add(self, other: Self) -> Result<Self, MoneyError> {
        self.same_currency(other)?;
        let minor = self.minor.checked_add(other.minor).ok_or(MoneyError::Overflow)?;
        Ok(Self { minor, ..self })
    }

    /// The difference of two amounts in the same currency.
    ///
    /// # Errors
    /// [`MoneyError::CurrencyMismatch`] or [`MoneyError::Overflow`].
    pub fn checked_sub(self, other: Self) -> Result<Self, MoneyError> {
        self.same_currency(other)?;
        let minor = self.minor.checked_sub(other.minor).ok_or(MoneyError::Overflow)?;
        Ok(Self { minor, ..self })
    }

    /// The negated amount.
    ///
    /// # Errors
    /// [`MoneyError::Overflow`] for the one `i64` that has no negation.
    pub fn checked_neg(self) -> Result<Self, MoneyError> {
        let minor = self.minor.checked_neg().ok_or(MoneyError::Overflow)?;
        Ok(Self { minor, ..self })
    }

    /// The amount times a whole number; exact, so no rounding is named.
    ///
    /// # Errors
    /// [`MoneyError::Overflow`].
    pub fn checked_mul(self, factor: i64) -> Result<Self, MoneyError> {
        let minor = self.minor.checked_mul(factor).ok_or(MoneyError::Overflow)?;
        Ok(Self { minor, ..self })
    }

    /// `round(amount × numerator ÷ denominator)` in minor units, rounded once with `mode`; daily interest is
    /// `times_ratio(rate_basis_points, 365 * 10_000, mode)`.
    ///
    /// # Errors
    /// [`MoneyError::DivisionByZero`] or [`MoneyError::Overflow`].
    pub fn times_ratio(self, numerator: i64, denominator: i64, mode: RoundingMode) -> Result<Self, MoneyError> {
        if denominator == 0 {
            return Err(MoneyError::DivisionByZero);
        }
        let rounded = mul_div_round(i128::from(self.minor), i128::from(numerator), i128::from(denominator), mode)
            .ok_or(MoneyError::Overflow)?;
        let minor = i64::try_from(rounded).map_err(|_| MoneyError::Overflow)?;
        Ok(Self { minor, ..self })
    }

    /// The wire form: a plain decimal string with exactly the currency's minor-unit digits, e.g. `-12.30`.
    #[must_use]
    pub fn to_decimal_string(&self) -> String {
        let exponent = usize::from(self.currency.exponent());
        let digits = format!("{:0>width$}", self.minor.unsigned_abs(), width = exponent.saturating_add(1));
        let sign = if self.minor < 0 { "-" } else { "" };
        match digits.split_at_checked(digits.len().saturating_sub(exponent)) {
            Some((whole, fraction)) if !fraction.is_empty() => format!("{sign}{whole}.{fraction}"),
            Some(_) | None => format!("{sign}{digits}"),
        }
    }

    fn same_currency(self, other: Self) -> Result<(), MoneyError> {
        if self.currency == other.currency { Ok(()) } else { Err(MoneyError::CurrencyMismatch) }
    }
}

impl core::fmt::Display for Money {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} {}", self.to_decimal_string(), self.currency.code())
    }
}

#[cfg(test)]
mod tests {
    use super::{Currency, Money, MoneyError, RoundingMode, RoundingOccasion, RoundingPolicy, mul_div_round};

    fn usd() -> Currency {
        Currency::from_code("USD").unwrap()
    }

    #[test]
    fn the_wire_form_has_the_currency_digits() {
        let jpy = Currency::from_code("JPY").unwrap();
        let kwd = Currency::from_code("KWD").unwrap();
        assert_eq!(Money::from_minor(-1230, usd()).to_decimal_string(), "-12.30");
        assert_eq!(Money::from_minor(5, usd()).to_decimal_string(), "0.05");
        assert_eq!(Money::from_minor(0, usd()).to_decimal_string(), "0.00");
        assert_eq!(Money::from_minor(-7, jpy).to_decimal_string(), "-7");
        assert_eq!(Money::from_minor(1, kwd).to_decimal_string(), "0.001");
        assert_eq!(Money::from_minor(i64::MIN, usd()).to_decimal_string(), "-92233720368547758.08");
        assert_eq!(Money::from_minor(1230, usd()).to_string(), "12.30 USD");
    }

    #[test]
    fn parsing_refuses_what_is_not_a_plain_decimal() {
        for bad in ["", "-", ".5", "5.", "+5", " 5", "5 ", "1e3", "1,5", "--1", "0x10", "1.2.3", "٣"] {
            assert_eq!(Money::parse(bad, usd()), Err(MoneyError::Malformed), "{bad:?}");
        }
        assert_eq!(Money::parse("1.234", usd()), Err(MoneyError::ExcessPrecision));
        assert_eq!(Money::parse("92233720368547758.08", usd()), Err(MoneyError::Overflow));
        assert_eq!(Money::parse("-92233720368547758.08", usd()).map(|m| m.minor()), Ok(i64::MIN));
        assert_eq!(Money::parse("12.3", usd()).map(|m| m.minor()), Ok(1230));
        assert_eq!(Money::parse("-0.01", usd()).map(|m| m.minor()), Ok(-1));
    }

    #[test]
    fn a_zero_denominator_is_refused() {
        assert_eq!(mul_div_round(1, 1, 0, RoundingMode::HalfUp), None);
        assert_eq!(
            Money::from_minor(1, usd()).times_ratio(1, 0, RoundingMode::HalfUp),
            Err(MoneyError::DivisionByZero)
        );
    }

    #[test]
    fn each_mode_rounds_the_five_tie_cases() {
        // amount 5, 15, 25, -5, -25 divided by 10: exact results 0.5, 1.5, 2.5, -0.5, -2.5
        let cases: [(i128, [i128; 7]); 5] = [
            (5, [1, 0, 1, 0, 1, 0, 0]),
            (15, [2, 1, 2, 1, 2, 1, 2]),
            (25, [3, 2, 3, 2, 3, 2, 2]),
            (-5, [-1, 0, 0, -1, -1, 0, 0]),
            (-25, [-3, -2, -2, -3, -3, -2, -2]),
        ];
        let modes = [
            RoundingMode::Up,
            RoundingMode::Down,
            RoundingMode::Ceiling,
            RoundingMode::Floor,
            RoundingMode::HalfUp,
            RoundingMode::HalfDown,
            RoundingMode::HalfEven,
        ];
        for (amount, expected) in cases {
            for (mode, want) in modes.iter().zip(expected) {
                assert_eq!(mul_div_round(amount, 1, 10, *mode), Some(want), "{amount} / 10 {mode:?}");
            }
        }
    }

    #[test]
    fn daily_interest_rounds_once() {
        // 1,000,000.00 at 5.25% for one day: 1e8 minor × 525 / (365 × 10,000) = 14383.56… → 14384 half-up
        let principal = Money::from_minor(100_000_000, usd());
        assert_eq!(principal.times_ratio(525, 3_650_000, RoundingMode::HalfUp).map(|m| m.minor()), Ok(14_384));
        assert_eq!(principal.times_ratio(525, 3_650_000, RoundingMode::Down).map(|m| m.minor()), Ok(14_383));
    }

    #[test]
    fn a_policy_names_a_mode_for_every_occasion() {
        let policy = RoundingPolicy { tax: RoundingMode::HalfEven, ..RoundingPolicy::half_up_everywhere() };
        assert_eq!(policy.mode(RoundingOccasion::Tax), RoundingMode::HalfEven);
        assert_eq!(policy.mode(RoundingOccasion::FeePercent), RoundingMode::HalfUp);
        assert_eq!(policy.mode(RoundingOccasion::FxConversion), RoundingMode::HalfUp);
        assert_eq!(policy.mode(RoundingOccasion::Allocation), RoundingMode::HalfUp);
    }
}
