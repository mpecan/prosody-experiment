//! The few transcendental functions the synth needs, computed the same way on every platform.
//!
//! `f64::sin` and `f64::exp2` call the platform's maths library, and those differ in the last bit
//! between macOS, Linux and the WebAssembly build (which compiles its own). One bit is enough to
//! change a golden hash, and `docs/player.md` §2 extends invariant I1 from text to audio: the same
//! poem, style and samples give the same samples out, everywhere. So these use only `+`, `*`, `/`
//! and exact rounding, which IEEE 754 defines bit for bit, with range reduction and a polynomial
//! long enough to be accurate to a few units in the last place over the ranges the synth uses.

use std::f64::consts::{FRAC_2_PI, LN_2, LOG2_10};

/// π/2 in two parts (fdlibm's `pio2_1` and `pio2_1t`): the first has its low bits zero, so `k ×`
/// it is exact for the small `k` the synth reaches, and the reduction keeps its precision.
const PIO2_HI: f64 = 1.570_796_326_734_125_6;
const PIO2_LO: f64 = 6.077_100_506_506_192e-11;

/// `2^x`.
#[must_use]
pub fn exp2(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    if x >= 1024.0 {
        return f64::INFINITY;
    }
    if x < -1022.0 {
        return 0.0;
    }
    let whole = x.round();
    // e^y for |y| ≤ ln2/2, by its Taylor series to y^13: the next term is below 4e-18.
    let y = (x - whole) * LN_2;
    let mut sum = 1.0;
    for k in (1..=13_u32).rev() {
        sum = 1.0 + sum * y / f64::from(k);
    }
    // 2^whole, built from its bits: exact, where `powi` would be another library call.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // `whole` is in −1022..=1023 here, so the biased exponent is 1..=2046
    let scale = f64::from_bits(((whole as i64 + 1023) as u64) << 52);
    sum * scale
}

/// `sin x` and `cos x` together.
#[must_use]
pub fn sin_cos(x: f64) -> (f64, f64) {
    if !x.is_finite() {
        return (f64::NAN, f64::NAN);
    }
    // Which quarter turn, then the remainder in −π/4..=π/4.
    let quarter = (x * FRAC_2_PI).round();
    let r = (x - quarter * PIO2_HI) - quarter * PIO2_LO;
    let r2 = r * r;
    // Taylor series: the first term left out is below 1e-19 for |r| ≤ π/4.
    let mut sin = 0.0;
    let mut cos = 0.0;
    for k in (1..=9_u32).rev() {
        let two_k = f64::from(2 * k);
        sin = 1.0 - sin * r2 / (two_k * (two_k + 1.0));
        cos = 1.0 - cos * r2 / ((two_k - 1.0) * two_k);
    }
    let sin = sin * r;
    #[allow(clippy::cast_possible_truncation)]
    // only its remainder mod 4 matters; a saturated cast of an enormous `x` still gives a quarter
    let turn = (quarter as i64).rem_euclid(4);
    match turn {
        0 => (sin, cos),
        1 => (cos, -sin),
        2 => (-sin, -cos),
        _ => (-cos, sin),
    }
}

/// `sin x`.
#[must_use]
pub fn sin(x: f64) -> f64 {
    sin_cos(x).0
}

/// `log2 x` for a positive `x`; `-inf` at 0 and NaN below it. For the compressor's decibels
/// (`dynamics`).
#[must_use]
pub fn log2(x: f64) -> f64 {
    if x.is_nan() || x < 0.0 {
        return f64::NAN;
    }
    if x == 0.0 {
        return f64::NEG_INFINITY;
    }
    if x.is_infinite() {
        return f64::INFINITY;
    }
    // A subnormal is scaled into the normal range first, so its exponent field means what it says.
    let (x, offset) = if x < f64::MIN_POSITIVE {
        (x * exp2(64.0), -64.0)
    } else {
        (x, 0.0)
    };
    let bits = x.to_bits();
    #[allow(clippy::cast_possible_wrap)] // an 11-bit field, far inside i64
    let exponent = ((bits >> 52) & 0x7ff) as i64 - 1023;
    // The mantissa as a number in 1..2, then moved to √½..√2 so the series below converges fast.
    let mut mantissa = f64::from_bits((bits & 0x000f_ffff_ffff_ffff) | 0x3ff0_0000_0000_0000);
    #[allow(clippy::cast_precision_loss)] // −1074..=1023
    let mut whole = exponent as f64 + offset;
    if mantissa > std::f64::consts::SQRT_2 {
        mantissa /= 2.0;
        whole += 1.0;
    }
    // ln m = 2·atanh(s), s = (m − 1)/(m + 1), |s| ≤ 0.172: the series to s^25 is exact to the last
    // bit (the next term is below 1e-20).
    let s = (mantissa - 1.0) / (mantissa + 1.0);
    let s2 = s * s;
    let mut sum = 0.0;
    for k in (0..=12_u32).rev() {
        sum = 1.0 / f64::from(2 * k + 1) + s2 * sum;
    }
    whole + 2.0 * s * sum / LN_2
}

/// An amplitude factor in decibels: `20·log10(x)`.
#[must_use]
pub fn decibels(x: f64) -> f64 {
    20.0 * log2(x) / LOG2_10
}

/// The amplitude factor of `db` decibels: `10^(db/20)`.
#[must_use]
pub fn from_decibels(db: f64) -> f64 {
    exp2(db / 20.0 * LOG2_10)
}

/// `e^x`, for one-pole smoothing coefficients (`dynamics`).
#[must_use]
pub fn exp(x: f64) -> f64 {
    exp2(x / LN_2)
}

/// `tanh x`, for the soft clip (`master`).
#[must_use]
pub fn tanh(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    let magnitude = x.abs();
    if magnitude > 20.0 {
        return x.signum();
    }
    // e^{−2|x|} = 2^{−2|x|/ln2}.
    let e = exp2(-2.0 * magnitude / LN_2);
    let t = (1.0 - e) / (1.0 + e);
    if x < 0.0 { -t } else { t }
}

#[cfg(test)]
mod tests {
    use super::{decibels, exp, exp2, from_decibels, log2, sin, sin_cos, tanh};

    #[test]
    fn log2_at_known_points() {
        // Powers of two are exact; √2 is a half; 10 and 3 to the last few bits.
        assert!(close(log2(1.0), 0.0, 0.0));
        assert!(close(log2(1024.0), 10.0, 0.0));
        assert!(close(log2(0.125), -3.0, 0.0));
        assert!(close(log2(std::f64::consts::SQRT_2), 0.5, 1e-15));
        assert!(close(log2(10.0), std::f64::consts::LOG2_10, 1e-14));
        assert!(close(log2(3.0), 1.584_962_500_721_156, 1e-15));
        // A subnormal: 2^-1070.
        assert!(close(log2(f64::from_bits(16)), -1070.0, 1e-12));
        assert!(log2(0.0).is_infinite() && log2(-1.0).is_nan());
    }

    #[test]
    fn decibels_both_ways() {
        // Half the amplitude is −6.0206 dB; −20 dB is a tenth; e^1 = 2.71828.
        assert!(close(decibels(0.5), -6.020_599_913_279_624, 1e-13));
        assert!(close(decibels(1.0), 0.0, 0.0));
        assert!(close(from_decibels(-20.0), 0.1, 1e-15));
        assert!(close(from_decibels(6.020_599_913_279_624), 2.0, 1e-14));
        assert!(close(exp(1.0), std::f64::consts::E, 1e-15));
    }

    fn close(a: f64, b: f64, tolerance: f64) -> bool {
        (a - b).abs() <= tolerance
    }

    #[test]
    fn exp2_at_known_points() {
        // Powers of two are exact; √2 and 2^(1/12) (a semitone) to the last few bits.
        assert!(close(exp2(0.0), 1.0, 0.0));
        assert!(close(exp2(10.0), 1024.0, 0.0));
        assert!(close(exp2(-3.0), 0.125, 0.0));
        assert!(close(exp2(0.5), std::f64::consts::SQRT_2, 1e-15));
        assert!(close(exp2(1.0 / 12.0), 1.059_463_094_359_295_3, 1e-15));
        assert!(close(exp2(-1.0 / 12.0), 0.943_874_312_681_693_5, 1e-15));
        assert!(exp2(2000.0).is_infinite());
        assert!(close(exp2(-2000.0), 0.0, 0.0));
        assert!(exp2(f64::NAN).is_nan());
    }

    #[test]
    fn sin_and_cos_at_known_points() {
        let pi = std::f64::consts::PI;
        assert!(close(sin(0.0), 0.0, 0.0));
        assert!(close(sin(pi / 6.0), 0.5, 1e-15));
        assert!(close(sin(pi / 2.0), 1.0, 1e-15));
        assert!(close(sin(pi), 0.0, 1e-15));
        assert!(close(sin(-pi / 2.0), -1.0, 1e-15));
        assert!(close(sin(1.5 * pi), -1.0, 1e-15));
        let (s, c) = sin_cos(pi / 3.0);
        assert!(close(s, 3.0_f64.sqrt() / 2.0, 1e-15));
        assert!(close(c, 0.5, 1e-15));
        // A whole turn, then some: the phase the oscillators feed in never goes past 2π.
        assert!(close(sin(2.0 * pi + pi / 6.0), 0.5, 1e-14));
        assert!(sin(f64::INFINITY).is_nan());
    }

    #[test]
    fn tanh_at_known_points() {
        assert!(close(tanh(0.0), 0.0, 0.0));
        // tanh(ln 3 / 2) = (3 − 1)/(3 + 1) = 0.5.
        assert!(close(tanh(3.0_f64.ln() / 2.0), 0.5, 1e-15));
        assert!(close(tanh(-(3.0_f64.ln()) / 2.0), -0.5, 1e-15));
        assert!(close(tanh(50.0), 1.0, 0.0));
        assert!(close(tanh(-50.0), -1.0, 0.0));
    }
}
