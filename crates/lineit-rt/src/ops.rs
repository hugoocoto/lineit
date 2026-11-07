//! Arithmetic with no surprises: integers wrap instead of panicking, and
//! dividing by zero gives 0. All `#[inline]`, so compiled code is a plain
//! instruction or two.

#[inline]
pub fn int_div(a: i64, b: i64) -> i64 {
    if b == 0 { 0 } else { a.wrapping_div(b) }
}

/// The remainder has the sign of the divisor, as in Lua and Python:
/// `-1 % 3` is 2.
#[inline]
pub fn int_mod(a: i64, b: i64) -> i64 {
    if b == 0 {
        0
    } else {
        let r = a.wrapping_rem(b);
        if r != 0 && (r < 0) != (b < 0) { r + b } else { r }
    }
}

#[inline]
pub fn float_mod(a: f64, b: f64) -> f64 {
    if b == 0.0 {
        0.0
    } else {
        let r = a % b;
        if r != 0.0 && (r < 0.0) != (b < 0.0) { r + b } else { r }
    }
}

/// A negative exponent gives 0 (1 for a base of 1, ±1 for -1).
#[inline]
pub fn int_pow(a: i64, b: i64) -> i64 {
    if b < 0 {
        match a {
            1 => 1,
            -1 => {
                if b % 2 == 0 {
                    1
                } else {
                    -1
                }
            }
            _ => 0,
        }
    } else {
        a.wrapping_pow(b.min(u32::MAX as i64) as u32)
    }
}

#[inline]
pub fn int_clamp(v: i64, lo: i64, hi: i64) -> i64 {
    if lo > hi { lo } else { v.clamp(lo, hi) }
}

#[inline]
pub fn float_clamp(v: f64, lo: f64, hi: f64) -> f64 {
    if lo > hi || lo.is_nan() || hi.is_nan() { lo } else { v.clamp(lo, hi) }
}

/// Saturates: NaN is 0, out of range is the closest end.
#[inline]
pub fn float_to_int(f: f64) -> i64 {
    f as i64
}

pub fn str_to_int(s: &str) -> i64 {
    let t = s.trim();
    crate::value::parse_int(t).or_else(|| crate::value::parse_float(t).map(float_to_int)).unwrap_or(0)
}

pub fn str_to_float(s: &str) -> f64 {
    crate::value::parse_float(s.trim()).unwrap_or(0.0)
}

pub fn str_to_bool(s: &str) -> bool {
    matches!(s.trim(), "true" | "1" | "yes")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn division() {
        assert_eq!(int_div(7, 2), 3);
        assert_eq!(int_div(7, 0), 0);
        assert_eq!(int_div(i64::MIN, -1), i64::MIN);
        assert_eq!(int_mod(-1, 3), 2);
        assert_eq!(int_mod(1, -3), -2);
        assert_eq!(int_mod(5, 0), 0);
        assert_eq!(float_mod(-1.0, 3.0), 2.0);
        assert_eq!(int_pow(2, 10), 1024);
        assert_eq!(int_pow(2, -1), 0);
        assert_eq!(str_to_int(" 42 "), 42);
        assert_eq!(str_to_int("2.9"), 2);
        assert_eq!(str_to_int("x"), 0);
    }
}
