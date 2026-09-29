//! Properties of Money and mul_div_round (`money` `M-1`, `M-3`, `M-4`, `M-7`, `M-24`): exact arithmetic in
//! one currency, a cross-currency operation refused, the wire form round-trips, excess precision refused,
//! and every rounding mode rounds once, to a neighbour of the exact quotient, in its stated direction.

#![cfg(test)]

use money::{Currency, Money, MoneyError, RoundingMode, mul_div_round};
use proptest::prelude::*;

const CODES: [&str; 5] = ["USD", "JPY", "KWD", "CLF", "EUR"];

fn currency() -> impl Strategy<Value = Currency> {
    proptest::sample::select(CODES.to_vec()).prop_map(|code| Currency::from_code(code).unwrap())
}

fn money() -> impl Strategy<Value = Money> {
    (any::<i64>(), currency()).prop_map(|(minor, c)| Money::from_minor(minor, c))
}

fn mode() -> impl Strategy<Value = RoundingMode> {
    proptest::sample::select(vec![
        RoundingMode::Up,
        RoundingMode::Down,
        RoundingMode::Ceiling,
        RoundingMode::Floor,
        RoundingMode::HalfUp,
        RoundingMode::HalfDown,
        RoundingMode::HalfEven,
    ])
}

proptest! {
    #[test]
    fn addition_is_exact_and_undone_by_subtraction(a in money(), b in any::<i64>()) {
        let b = Money::from_minor(b, a.currency());
        match a.checked_add(b) {
            Ok(sum) => {
                prop_assert_eq!(sum.checked_sub(b), Ok(a));
                prop_assert_eq!(b.checked_add(a), Ok(sum));
                prop_assert_eq!(Some(sum.minor()), a.minor().checked_add(b.minor()));
            }
            Err(e) => {
                prop_assert_eq!(e, MoneyError::Overflow);
                prop_assert_eq!(a.minor().checked_add(b.minor()), None);
            }
        }
    }

    #[test]
    fn a_cross_currency_operation_is_refused(minor_a: i64, minor_b: i64) {
        let usd = Money::from_minor(minor_a, Currency::from_code("USD").unwrap());
        let eur = Money::from_minor(minor_b, Currency::from_code("EUR").unwrap());
        prop_assert_eq!(usd.checked_add(eur), Err(MoneyError::CurrencyMismatch));
        prop_assert_eq!(usd.checked_sub(eur), Err(MoneyError::CurrencyMismatch));
    }

    #[test]
    fn the_wire_form_round_trips(m in money()) {
        prop_assert_eq!(Money::parse(&m.to_decimal_string(), m.currency()), Ok(m));
    }

    #[test]
    fn a_digit_past_the_minor_unit_is_refused_never_rounded(m in money(), extra in 1_u8..=9) {
        let text = m.to_decimal_string();
        let longer = if m.currency().exponent() == 0 { format!("{text}.{extra}") } else { format!("{text}{extra}") };
        prop_assert_eq!(Money::parse(&longer, m.currency()), Err(MoneyError::ExcessPrecision));
    }

    #[test]
    fn mul_div_round_lands_on_a_neighbour_of_the_exact_quotient_in_its_direction(
        amount in any::<i64>(),
        numerator in -1_000_000_i64..1_000_000,
        denominator in (1_i64..10_000_000).prop_union(-10_000_000_i64..-1),
        mode in mode(),
    ) {
        let (amount, numerator, denominator) = (i128::from(amount), i128::from(numerator), i128::from(denominator));
        let rounded = mul_div_round(amount, numerator, denominator, mode).unwrap();
        // Normalise to a positive denominator: the exact quotient is p / d.
        let sign = denominator.signum();
        let p = amount.checked_mul(numerator).unwrap().checked_mul(sign).unwrap();
        let d = denominator.checked_mul(sign).unwrap();
        // diff = rounded·d − p: zero when exact, and |diff| < d, since the result is a neighbour of p / d.
        let diff = rounded.checked_mul(d).unwrap().checked_sub(p).unwrap();
        prop_assert!(diff.abs() < d);
        let twice = diff.abs().checked_mul(2).unwrap();
        let away_from_zero = diff != 0 && (diff > 0) == (p > 0);
        let toward_zero = diff != 0 && !away_from_zero;
        match mode {
            RoundingMode::Down => prop_assert!(!away_from_zero),
            RoundingMode::Up => prop_assert!(!toward_zero),
            RoundingMode::Floor => prop_assert!(diff <= 0),
            RoundingMode::Ceiling => prop_assert!(diff >= 0),
            RoundingMode::HalfUp => {
                prop_assert!(twice <= d);
                if twice == d { prop_assert!(away_from_zero); }
            }
            RoundingMode::HalfDown => {
                prop_assert!(twice <= d);
                if twice == d { prop_assert!(toward_zero); }
            }
            RoundingMode::HalfEven => {
                prop_assert!(twice <= d);
                if twice == d { prop_assert_eq!(rounded.checked_rem(2), Some(0)); }
            }
        }
    }

    #[test]
    fn a_ratio_of_one_is_the_identity(m in money(), n in 1_i64..1_000_000, mode in mode()) {
        prop_assert_eq!(m.times_ratio(n, n, mode), Ok(m));
    }
}
