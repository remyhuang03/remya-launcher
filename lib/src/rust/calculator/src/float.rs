use core::num::FpCategory;

use half::{bf16, f16};
use num_bigint::{BigInt, BigUint};
use num_rational::BigRational;
use num_traits::{One, Signed, Zero};
use serde_json::{Map, Value, json};

use crate::protocol::{Candidate, Diagnostic, Format};

const MAX_QUERY_BYTES: usize = 4096;
const MAX_EXPONENT_MAGNITUDE: i64 = 1_000_000_000;
const APPROX_SIG_DIGITS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FloatFormat {
    Fp32,
    Fp16,
    Bf16,
}

impl FloatFormat {
    const fn id(self) -> &'static str {
        match self {
            Self::Fp32 => "fp32",
            Self::Fp16 => "fp16",
            Self::Bf16 => "bf16",
        }
    }

    const fn display_name(self) -> &'static str {
        match self {
            Self::Fp32 => "FP32",
            Self::Fp16 => "FP16",
            Self::Bf16 => "BF16",
        }
    }

    const fn description(self) -> &'static str {
        match self {
            Self::Fp32 => "IEEE 754 binary32 (1 sign / 8 exponent / 23 fraction, bias 127)",
            Self::Fp16 => "IEEE 754 binary16 (1 sign / 5 exponent / 10 fraction, bias 15)",
            Self::Bf16 => "bfloat16 (1 sign / 8 exponent / 7 fraction, bias 127)",
        }
    }

    const fn total_bits(self) -> u8 {
        match self {
            Self::Fp32 => 32,
            Self::Fp16 | Self::Bf16 => 16,
        }
    }

    const fn exp_bits(self) -> u8 {
        match self {
            Self::Fp32 | Self::Bf16 => 8,
            Self::Fp16 => 5,
        }
    }

    const fn frac_bits(self) -> u8 {
        match self {
            Self::Fp32 => 23,
            Self::Fp16 => 10,
            Self::Bf16 => 7,
        }
    }

    const fn bias(self) -> i32 {
        match self {
            Self::Fp32 | Self::Bf16 => 127,
            Self::Fp16 => 15,
        }
    }

    const fn min_normal_exp(self) -> i32 {
        1 - self.bias()
    }

    const fn max_normal_exp(self) -> i32 {
        self.bias()
    }

    const fn precision(self) -> u32 {
        self.frac_bits() as u32 + 1
    }

    const fn hex_width(self) -> usize {
        (self.total_bits() as usize) / 4
    }

    const fn ordered_with_alternatives(self) -> [FloatFormat; 3] {
        match self {
            Self::Fp32 => [Self::Fp32, Self::Fp16, Self::Bf16],
            Self::Fp16 => [Self::Fp16, Self::Fp32, Self::Bf16],
            Self::Bf16 => [Self::Bf16, Self::Fp32, Self::Fp16],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TargetFormat {
    Supported(FloatFormat),
    Unsupported {
        raw: String,
        canonical: &'static str,
        is_fp8: bool,
    },
}

#[derive(Debug, Clone)]
struct NormalizedQuery {
    text: String,
    /// Maps each byte offset in `text` (length `text.len() + 1`) to a byte offset in original `query`.
    orig_byte_map: Vec<usize>,
}

fn normalize_query(query: &str) -> NormalizedQuery {
    let mut text = String::with_capacity(query.len());
    let mut orig_byte_map = Vec::with_capacity(query.len() + 1);

    for (byte_idx, ch) in query.char_indices() {
        let mapped = match ch {
            '\u{2212}' => '-',
            '\u{00A0}' | '\u{3000}' => ' ',
            '\u{FF01}'..='\u{FF5E}' => char::from_u32(ch as u32 - 0xFEE0).unwrap_or(ch),
            _ => ch,
        };
        let mut buf = [0u8; 4];
        let encoded = mapped.encode_utf8(&mut buf);
        for _ in 0..encoded.len() {
            orig_byte_map.push(byte_idx);
        }
        text.push_str(encoded);
    }
    orig_byte_map.push(query.len());

    NormalizedQuery {
        text,
        orig_byte_map,
    }
}

fn utf16_span(query: &str, orig_start: usize, orig_end: usize) -> Value {
    let start_utf16 = query[..orig_start].encode_utf16().count();
    let end_utf16 = query[..orig_end].encode_utf16().count();
    json!({
        "startUtf16": start_utf16,
        "endUtf16": end_utf16,
    })
}

fn match_format_target(raw_target: &str) -> Option<TargetFormat> {
    let compact: String = raw_target
        .chars()
        .filter(|c| !c.is_ascii_whitespace() && *c != '_' && *c != '-')
        .flat_map(|c| c.to_lowercase())
        .collect();

    match compact.as_str() {
        "fp32" | "f32" | "binary32" | "float32" => Some(TargetFormat::Supported(FloatFormat::Fp32)),
        "fp16" | "f16" | "binary16" | "float16" => Some(TargetFormat::Supported(FloatFormat::Fp16)),
        "bf16" | "bfloat16" => Some(TargetFormat::Supported(FloatFormat::Bf16)),
        "fp8" | "f8" => Some(TargetFormat::Unsupported {
            raw: raw_target.trim().to_string(),
            canonical: "fp8",
            is_fp8: true,
        }),
        "fp8e4m3fn" | "e4m3fn" | "fp8e4m3" | "e4m3" => Some(TargetFormat::Unsupported {
            raw: raw_target.trim().to_string(),
            canonical: "fp8_e4m3fn",
            is_fp8: true,
        }),
        "fp8e5m2" | "e5m2" => Some(TargetFormat::Unsupported {
            raw: raw_target.trim().to_string(),
            canonical: "fp8_e5m2",
            is_fp8: true,
        }),
        "fp64" | "f64" | "binary64" | "float64" => Some(TargetFormat::Unsupported {
            raw: raw_target.trim().to_string(),
            canonical: "fp64",
            is_fp8: false,
        }),
        _ => None,
    }
}

struct ParsedQuery<'a> {
    lhs: &'a str,
    lhs_orig_start: usize,
    lhs_orig_end: usize,
    target: TargetFormat,
    target_orig_start: usize,
    target_orig_end: usize,
}

fn split_as_clause<'a>(norm: &'a NormalizedQuery) -> Option<ParsedQuery<'a>> {
    let s = norm.text.as_str();
    let trimmed_end = s.trim_end_matches(|c: char| c.is_ascii_whitespace() || c.is_control());
    let trimmed_start_offset = s.len()
        - s.trim_start_matches(|c: char| c.is_ascii_whitespace() || c.is_control())
            .len();
    if trimmed_start_offset >= trimmed_end.len() {
        return None;
    }

    // Collect ASCII-whitespace-delimited word slices as (byte_start, byte_end, slice).
    let mut words: Vec<(usize, usize, &str)> = Vec::new();
    let mut in_word = false;
    let mut word_start = 0;
    for (idx, ch) in trimmed_end.char_indices() {
        let is_ws = ch == ' ' || ch == '\t';
        if is_ws {
            if in_word {
                words.push((word_start, idx, &trimmed_end[word_start..idx]));
                in_word = false;
            }
        } else if !in_word {
            word_start = idx;
            in_word = true;
        }
    }
    if in_word {
        words.push((
            word_start,
            trimmed_end.len(),
            &trimmed_end[word_start..trimmed_end.len()],
        ));
    }

    // Find the last `"as"` keyword token that is followed by a recognized float format target.
    for as_idx in (0..words.len()).rev() {
        if !words[as_idx].2.eq_ignore_ascii_case("as") {
            continue;
        }
        if as_idx + 1 >= words.len() {
            continue;
        }
        let target_start = words[as_idx + 1].0;
        let target_end = words[words.len() - 1].1;
        let raw_target = &trimmed_end[target_start..target_end];
        if let Some(target) = match_format_target(raw_target) {
            let lhs_start = trimmed_start_offset;
            let lhs_end = if as_idx == 0 {
                lhs_start
            } else {
                words[as_idx - 1].1
            };
            let lhs = &trimmed_end[lhs_start..lhs_end];
            return Some(ParsedQuery {
                lhs,
                lhs_orig_start: norm.orig_byte_map[lhs_start],
                lhs_orig_end: norm.orig_byte_map[lhs_end],
                target,
                target_orig_start: norm.orig_byte_map[target_start],
                target_orig_end: norm.orig_byte_map[target_end],
            });
        }
    }

    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ParsedInput {
    Zero {
        negative: bool,
        normalized_literal: String,
    },
    Finite {
        negative: bool,
        /// Non-empty ASCII digit string with leading and trailing zeros stripped.
        digits: String,
        /// Mathematical value = (-1)^negative * int(digits) * 10^exp10.
        exp10: i64,
        /// Base-10 order of magnitude: exp10 + digits.len() - 1.
        mag_exp10: i64,
        normalized_literal: String,
    },
    Infinity {
        negative: bool,
        normalized_literal: String,
    },
    Nan {
        negative: bool,
        normalized_literal: String,
    },
}

impl ParsedInput {
    fn normalized_literal(&self) -> &str {
        match self {
            Self::Zero {
                normalized_literal, ..
            }
            | Self::Finite {
                normalized_literal, ..
            }
            | Self::Infinity {
                normalized_literal, ..
            }
            | Self::Nan {
                normalized_literal, ..
            } => normalized_literal,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParseErrorKind {
    Incomplete,
    InvalidSyntax,
    ResourceLimit,
}

fn strip_underscores_strict(s: &str) -> Result<String, ParseErrorKind> {
    if s.is_empty() {
        return Ok(String::new());
    }
    let bytes = s.as_bytes();
    if bytes.first() == Some(&b'_') || bytes.last() == Some(&b'_') {
        return Err(ParseErrorKind::InvalidSyntax);
    }
    let mut out = String::with_capacity(s.len());
    let mut prev_underscore = false;
    for &b in bytes {
        if b == b'_' {
            if prev_underscore {
                return Err(ParseErrorKind::InvalidSyntax);
            }
            prev_underscore = true;
        } else if b.is_ascii_digit() {
            prev_underscore = false;
            out.push(b as char);
        } else {
            return Err(ParseErrorKind::InvalidSyntax);
        }
    }
    Ok(out)
}

fn parse_decimal_input(raw_lhs: &str) -> Result<ParsedInput, ParseErrorKind> {
    let s = raw_lhs.trim();
    if s.is_empty() {
        return Err(ParseErrorKind::Incomplete);
    }

    let (negative, rest) = if let Some(r) = s.strip_prefix('-') {
        (true, r.trim_start())
    } else if let Some(r) = s.strip_prefix('+') {
        (false, r.trim_start())
    } else {
        (false, s)
    };

    if rest.is_empty() {
        return Err(ParseErrorKind::Incomplete);
    }

    let lower = rest.to_ascii_lowercase();
    match lower.as_str() {
        "inf" | "infinity" => {
            let normalized_literal = if negative {
                "-Infinity".to_string()
            } else {
                "Infinity".to_string()
            };
            return Ok(ParsedInput::Infinity {
                negative,
                normalized_literal,
            });
        }
        "nan" | "qnan" | "snan" => {
            let normalized_literal = if negative {
                "-NaN".to_string()
            } else {
                "NaN".to_string()
            };
            return Ok(ParsedInput::Nan {
                negative,
                normalized_literal,
            });
        }
        _ => {}
    }

    // Split optional exponent ('e' or 'E').
    let (mantissa_str, exp_part) = match rest.find(['e', 'E']) {
        Some(idx) => (&rest[..idx], Some(&rest[idx + 1..])),
        None => (rest, None),
    };

    // Parse mantissa as optional int part and optional '.' frac part.
    let (int_raw, frac_raw, has_dot) = match mantissa_str.find('.') {
        Some(dot_idx) => {
            let left = &mantissa_str[..dot_idx];
            let right = &mantissa_str[dot_idx + 1..];
            if right.contains('.') {
                return Err(ParseErrorKind::InvalidSyntax);
            }
            (left, right, true)
        }
        None => (mantissa_str, "", false),
    };

    if int_raw.is_empty() && frac_raw.is_empty() {
        return Err(ParseErrorKind::Incomplete);
    }

    let int_digits = strip_underscores_strict(int_raw)?;
    let frac_digits = strip_underscores_strict(frac_raw)?;

    if int_digits.is_empty() && frac_digits.is_empty() {
        return Err(ParseErrorKind::InvalidSyntax);
    }

    // Parse optional exponent.
    let exp_val: i64 = if let Some(exp_str) = exp_part {
        if exp_str.is_empty() {
            return Err(ParseErrorKind::Incomplete);
        }
        let (exp_neg, exp_digits_raw) = if let Some(r) = exp_str.strip_prefix('-') {
            (true, r)
        } else if let Some(r) = exp_str.strip_prefix('+') {
            (false, r)
        } else {
            (false, exp_str)
        };
        if exp_digits_raw.is_empty() {
            return Err(ParseErrorKind::Incomplete);
        }
        let exp_digits = strip_underscores_strict(exp_digits_raw)?;
        let trimmed_exp = exp_digits.trim_start_matches('0');
        let abs_exp: i64 = if trimmed_exp.is_empty() {
            0
        } else {
            match trimmed_exp.parse::<i64>() {
                Ok(v) if v <= MAX_EXPONENT_MAGNITUDE => v,
                _ => return Err(ParseErrorKind::ResourceLimit),
            }
        };
        if exp_neg { -abs_exp } else { abs_exp }
    } else {
        0
    };

    // Construct canonical normalized display literal.
    let mut normalized_literal = String::new();
    if negative {
        normalized_literal.push('-');
    }
    if int_digits.is_empty() {
        normalized_literal.push('0');
    } else {
        let stripped = int_digits.trim_start_matches('0');
        if stripped.is_empty() {
            normalized_literal.push('0');
        } else {
            normalized_literal.push_str(stripped);
        }
    }
    if has_dot && !frac_digits.is_empty() {
        normalized_literal.push('.');
        normalized_literal.push_str(&frac_digits);
    }
    if exp_part.is_some() {
        normalized_literal.push('e');
        normalized_literal.push_str(&exp_val.to_string());
    }

    // Combine int + frac digits and compute exact base-10 representation.
    let mut all_digits = String::with_capacity(int_digits.len() + frac_digits.len());
    all_digits.push_str(&int_digits);
    all_digits.push_str(&frac_digits);

    let trimmed_leading = all_digits.trim_start_matches('0');
    if trimmed_leading.is_empty() {
        let zero_lit = if negative { "-0" } else { "0" };
        return Ok(ParsedInput::Zero {
            negative,
            normalized_literal: zero_lit.to_string(),
        });
    }

    let trailing_zeros = trimmed_leading.len() - trimmed_leading.trim_end_matches('0').len();
    let digits = trimmed_leading[..trimmed_leading.len() - trailing_zeros].to_string();

    let exp10 = exp_val
        .checked_sub(frac_digits.len() as i64)
        .and_then(|v| v.checked_add(trailing_zeros as i64))
        .ok_or(ParseErrorKind::ResourceLimit)?;
    let mag_exp10 = exp10
        .checked_add(digits.len() as i64 - 1)
        .ok_or(ParseErrorKind::ResourceLimit)?;

    Ok(ParsedInput::Finite {
        negative,
        digits,
        exp10,
        mag_exp10,
        normalized_literal,
    })
}

/// Computes `floor(log2(p / q))` for positive `BigUint` values `p` and `q`.
fn floor_log2_ratio(p: &BigUint, q: &BigUint) -> i32 {
    let bp = p.bits() as i64;
    let bq = q.bits() as i64;
    let d = bp - bq;
    let cmp = if d >= 0 {
        p.cmp(&(q << (d as usize)))
    } else {
        (p << ((-d) as usize)).cmp(q)
    };
    if cmp.is_ge() {
        d as i32
    } else {
        (d - 1) as i32
    }
}

/// Rounds positive exact rational `p / q` to IEEE binary format `(stored_exp, stored_frac, overflowed)`
/// using single-step round-to-nearest, ties-to-even (RTNE) with exact guard and sticky bits.
fn round_positive_rational_rtne(p: &BigUint, q: &BigUint, fmt: FloatFormat) -> (u32, u32, bool) {
    let e = floor_log2_ratio(p, q);
    let e_min = fmt.min_normal_exp();
    let e_max = fmt.max_normal_exp();
    let prec = fmt.precision() as i32;

    if e > e_max {
        return ((1u32 << fmt.exp_bits()) - 1, 0, true);
    }
    if e < e_min - prec {
        return (0, 0, false);
    }

    let is_normal = e >= e_min;
    let anchor_exp = if is_normal { e } else { e_min };
    // Scale by 2^k so integer quotient represents units of 1 target ULP.
    let k = (prec - 1) - anchor_exp;

    let (numer, denom) = if k >= 0 {
        (p << (k as usize), q.clone())
    } else {
        (p.clone(), q << ((-k) as usize))
    };

    let quot = &numer / &denom;
    let rem = &numer % &denom;
    let q_u64 = quot.to_u64_digits().into_iter().next().unwrap_or(0);

    // Extract guard (fractional bit >= 0.5) and sticky (remaining bits nonzero).
    let two_rem: BigUint = &rem << 1usize;
    let (guard, sticky) = match two_rem.cmp(&denom) {
        core::cmp::Ordering::Less => (false, !rem.is_zero()),
        core::cmp::Ordering::Equal => (true, false),
        core::cmp::Ordering::Greater => (true, true),
    };

    let round_up = guard && (sticky || (q_u64 & 1 == 1));
    let rounded_m = (q_u64 as u32) + u32::from(round_up);
    let implicit_bit = 1u32 << fmt.frac_bits();

    if is_normal {
        if rounded_m == (implicit_bit << 1) {
            let next_e = e + 1;
            if next_e > e_max {
                ((1u32 << fmt.exp_bits()) - 1, 0, true)
            } else {
                ((next_e + fmt.bias()) as u32, 0, false)
            }
        } else {
            ((e + fmt.bias()) as u32, rounded_m - implicit_bit, false)
        }
    } else if rounded_m == implicit_bit {
        // Subnormal carried up into the minimum normal value.
        (1, 0, false)
    } else {
        (0, rounded_m, false)
    }
}

fn verify_with_primitive(
    fmt: FloatFormat,
    bits: u32,
    expected_category: FpCategory,
    negative: bool,
) {
    let (actual_cat, actual_neg, roundtrip_bits) = match fmt {
        FloatFormat::Fp32 => {
            let v = f32::from_bits(bits);
            (v.classify(), v.is_sign_negative(), v.to_bits())
        }
        FloatFormat::Fp16 => {
            let v = f16::from_bits(bits as u16);
            (v.classify(), v.is_sign_negative(), u32::from(v.to_bits()))
        }
        FloatFormat::Bf16 => {
            let v = bf16::from_bits(bits as u16);
            (v.classify(), v.is_sign_negative(), u32::from(v.to_bits()))
        }
    };
    debug_assert_eq!(roundtrip_bits, bits);
    debug_assert_eq!(actual_cat, expected_category);
    debug_assert_eq!(actual_neg, negative);
}

fn format_unsigned_rational(numer: &BigUint, denom: &BigUint) -> String {
    if numer.is_zero() {
        return "0".to_string();
    }

    // Check if denominator = 2^p2 * 5^p5 (exact terminating decimal).
    let p2 = denom.trailing_zeros().unwrap_or(0) as usize;
    let mut d_odd = denom >> p2;
    let five = BigUint::from(5u32);
    let mut p5 = 0usize;
    while (&d_odd % &five).is_zero() {
        d_odd /= &five;
        p5 += 1;
    }

    if d_odd.is_one() {
        let k = p2.max(p5);
        if k <= 512 {
            let mut scaled = numer.clone();
            if k > p2 {
                scaled <<= k - p2;
            }
            if k > p5 {
                scaled *= five.pow((k - p5) as u32);
            }
            let s = scaled.to_str_radix(10);
            if k == 0 {
                return s;
            }
            if s.len() <= k {
                let mut out = String::with_capacity(k + 2);
                out.push_str("0.");
                for _ in 0..(k - s.len()) {
                    out.push('0');
                }
                out.push_str(&s);
                return trim_decimal_trailing_zeros(out);
            }
            let split = s.len() - k;
            let mut out = String::with_capacity(s.len() + 1);
            out.push_str(&s[..split]);
            out.push('.');
            out.push_str(&s[split..]);
            return trim_decimal_trailing_zeros(out);
        }
    }

    // Non-terminating decimal: format to APPROX_SIG_DIGITS with RTNE.
    format_approx_decimal(numer, denom, APPROX_SIG_DIGITS)
}

fn trim_decimal_trailing_zeros(mut s: String) -> String {
    if let Some(dot_idx) = s.find('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.len() == dot_idx + 1 {
            s.pop();
        }
    }
    s
}

fn format_approx_decimal(numer: &BigUint, denom: &BigUint, sig_digits: usize) -> String {
    let ten = BigUint::from(10u32);
    let int_q = numer / denom;
    let rem = numer % denom;

    if !int_q.is_zero() {
        let int_str = int_q.to_str_radix(10);
        if int_str.len() >= sig_digits {
            let two_rem: BigUint = &rem << 1usize;
            let round_up = match two_rem.cmp(denom) {
                core::cmp::Ordering::Greater => true,
                core::cmp::Ordering::Equal => (&int_q % BigUint::from(2u32)).is_one(),
                core::cmp::Ordering::Less => false,
            };
            let rounded = if round_up { int_q + 1u32 } else { int_q };
            return rounded.to_str_radix(10);
        }
        let frac_len = sig_digits - int_str.len();
        let scale = ten.pow(frac_len as u32);
        let scaled = &rem * &scale;
        let mut frac_q = &scaled / denom;
        let frac_r = &scaled % denom;
        let two_r: BigUint = &frac_r << 1usize;
        let round_up = match two_r.cmp(denom) {
            core::cmp::Ordering::Greater => true,
            core::cmp::Ordering::Equal => (&frac_q % BigUint::from(2u32)).is_one(),
            core::cmp::Ordering::Less => false,
        };
        if round_up {
            frac_q += 1u32;
        }
        let (final_int, final_frac) = if frac_q >= scale {
            (int_q + 1u32, BigUint::zero())
        } else {
            (int_q, frac_q)
        };
        if final_frac.is_zero() {
            return final_int.to_str_radix(10);
        }
        let frac_raw = final_frac.to_str_radix(10);
        let mut out = final_int.to_str_radix(10);
        out.push('.');
        for _ in 0..(frac_len - frac_raw.len()) {
            out.push('0');
        }
        out.push_str(&frac_raw);
        return trim_decimal_trailing_zeros(out);
    }

    // 0 < numer / denom < 1
    let mut lz = 0usize;
    let mut cur = numer.clone();
    while &cur * &ten < *denom {
        cur *= &ten;
        lz += 1;
    }
    let scale = ten.pow((lz + sig_digits) as u32);
    let scaled = numer * &scale;
    let mut q = &scaled / denom;
    let r = &scaled % denom;
    let two_r: BigUint = &r << 1usize;
    let round_up = match two_r.cmp(denom) {
        core::cmp::Ordering::Greater => true,
        core::cmp::Ordering::Equal => (&q % BigUint::from(2u32)).is_one(),
        core::cmp::Ordering::Less => false,
    };
    if round_up {
        q += 1u32;
    }
    let carry_threshold = ten.pow(sig_digits as u32);
    if q >= carry_threshold {
        if lz == 0 {
            return "1".to_string();
        }
        lz -= 1;
        q /= &ten;
    }
    let q_str = q.to_str_radix(10);
    let mut out = String::with_capacity(2 + lz + q_str.len());
    out.push_str("0.");
    for _ in 0..lz {
        out.push('0');
    }
    out.push_str(&q_str);
    trim_decimal_trailing_zeros(out)
}

fn format_rational_signed(rat: &BigRational, explicit_plus: bool) -> String {
    if rat.is_zero() {
        return "0".to_string();
    }
    let mag_str = format_unsigned_rational(rat.numer().magnitude(), rat.denom().magnitude());
    if rat.is_negative() {
        format!("-{mag_str}")
    } else if explicit_plus {
        format!("+{mag_str}")
    } else {
        mag_str
    }
}

fn format_shifted_decimal(negative: bool, digits: &str, exp10: i64, mag_exp10: i64) -> String {
    let sign = if negative { "-" } else { "+" };
    if mag_exp10 >= -200 && exp10 < 0 {
        let frac_pos = (-exp10) as usize;
        if digits.len() <= frac_pos {
            let mut out = String::with_capacity(sign.len() + 2 + frac_pos);
            out.push_str(sign);
            out.push_str("0.");
            for _ in 0..(frac_pos - digits.len()) {
                out.push('0');
            }
            out.push_str(digits);
            return out;
        }
    }
    let mut out = String::with_capacity(sign.len() + digits.len() + 16);
    out.push_str(sign);
    let mut chars = digits.chars();
    if let Some(first) = chars.next() {
        out.push(first);
    }
    let rest: String = chars.collect();
    if !rest.is_empty() {
        out.push('.');
        out.push_str(&rest);
    }
    out.push('e');
    out.push_str(&mag_exp10.to_string());
    out
}

#[derive(Debug, Clone)]
struct RoundedFloat {
    format: FloatFormat,
    #[cfg_attr(not(test), allow(dead_code))]
    bits: u32,
    sign_bit: &'static str,
    exp_bits_str: String,
    frac_bits_str: String,
    bits_hex: String,
    bits_binary: String,
    unbiased_exp: Option<i32>,
    significand: Option<String>,
    stored_decimal: String,
    abs_error: Option<String>,
    rel_error: Option<String>,
    rel_error_pct: Option<String>,
    classification: &'static str,
    exact: bool,
    overflowed: bool,
    underflowed_to_zero: bool,
}

impl RoundedFloat {
    fn to_json_value(&self, include_tag: bool) -> Value {
        let bits_hex = format!("0x{:0width$X}", self.bits, width = self.format.hex_width());
        let mut map = Map::new();
        if include_tag {
            map.insert("tag".to_string(), json!("float_bits"));
        }
        map.insert("format".to_string(), json!(self.format.id()));
        map.insert("bitsHex".to_string(), json!(bits_hex));
        map.insert("bitsBinary".to_string(), json!(self.bits_binary));
        map.insert("signBit".to_string(), json!(self.sign_bit));
        map.insert("exponentBits".to_string(), json!(self.exp_bits_str));
        map.insert("fractionBits".to_string(), json!(self.frac_bits_str));
        map.insert(
            "unbiasedExponent".to_string(),
            match self.unbiased_exp {
                Some(e) => json!(e),
                None => Value::Null,
            },
        );
        map.insert(
            "significand".to_string(),
            match &self.significand {
                Some(s) => json!(s),
                None => Value::Null,
            },
        );
        map.insert("storedDecimal".to_string(), json!(self.stored_decimal));

        if let Some(abs_err) = &self.abs_error {
            map.insert("absoluteError".to_string(), json!(abs_err));
        }
        if let Some(rel_err) = &self.rel_error {
            map.insert("relativeError".to_string(), json!(rel_err));
        }
        if let Some(rel_pct) = &self.rel_error_pct {
            map.insert("relativeErrorPercent".to_string(), json!(rel_pct));
        }

        let mut err_obj = Map::new();
        if let Some(abs_err) = &self.abs_error {
            err_obj.insert("absolute".to_string(), json!(abs_err));
        }
        if let Some(rel_err) = &self.rel_error {
            err_obj.insert("relative".to_string(), json!(rel_err));
        }
        if let Some(rel_pct) = &self.rel_error_pct {
            err_obj.insert("relativePercent".to_string(), json!(rel_pct));
        }
        map.insert("error".to_string(), Value::Object(err_obj));

        map.insert("classification".to_string(), json!(self.classification));
        map.insert(
            "roundingMode".to_string(),
            json!("round_to_nearest_ties_to_even"),
        );
        map.insert("exact".to_string(), json!(self.exact));
        Value::Object(map)
    }
}

fn build_bit_strings(
    fmt: FloatFormat,
    negative: bool,
    stored_exp: u32,
    stored_frac: u32,
) -> (u32, &'static str, String, String, String, String) {
    let sign_u32 = u32::from(negative);
    let bits = (sign_u32 << (fmt.exp_bits() + fmt.frac_bits()))
        | (stored_exp << fmt.frac_bits())
        | stored_frac;
    let sign_bit = if negative { "1" } else { "0" };
    let exp_w = fmt.exp_bits() as usize;
    let frac_w = fmt.frac_bits() as usize;
    let total_w = fmt.total_bits() as usize;
    let exp_bits_str = format!("{stored_exp:0exp_w$b}");
    let frac_bits_str = format!("{stored_frac:0frac_w$b}");
    let bits_hex = format!("0x{bits:0hex_w$X}", hex_w = fmt.hex_width());
    let bits_binary = format!("{bits:0total_w$b}");
    (
        bits,
        sign_bit,
        exp_bits_str,
        frac_bits_str,
        bits_hex,
        bits_binary,
    )
}

fn round_input_to_format(input: &ParsedInput, fmt: FloatFormat) -> RoundedFloat {
    let max_exp_field = (1u32 << fmt.exp_bits()) - 1;

    match input {
        ParsedInput::Nan { negative, .. } => {
            let quiet_frac = 1u32 << (fmt.frac_bits() - 1);
            let (bits, sign_bit, exp_bits_str, frac_bits_str, bits_hex, bits_binary) =
                build_bit_strings(fmt, *negative, max_exp_field, quiet_frac);
            verify_with_primitive(fmt, bits, FpCategory::Nan, *negative);
            RoundedFloat {
                format: fmt,
                bits,
                sign_bit,
                exp_bits_str,
                frac_bits_str,
                bits_hex,
                bits_binary,
                unbiased_exp: None,
                significand: None,
                stored_decimal: "NaN".to_string(),
                abs_error: None,
                rel_error: None,
                rel_error_pct: None,
                classification: "nan",
                exact: false,
                overflowed: false,
                underflowed_to_zero: false,
            }
        }
        ParsedInput::Infinity { negative, .. } => {
            let (bits, sign_bit, exp_bits_str, frac_bits_str, bits_hex, bits_binary) =
                build_bit_strings(fmt, *negative, max_exp_field, 0);
            verify_with_primitive(fmt, bits, FpCategory::Infinite, *negative);
            let classification = if *negative {
                "negative_infinity"
            } else {
                "infinity"
            };
            let stored_decimal = if *negative { "-Infinity" } else { "Infinity" };
            RoundedFloat {
                format: fmt,
                bits,
                sign_bit,
                exp_bits_str,
                frac_bits_str,
                bits_hex,
                bits_binary,
                unbiased_exp: None,
                significand: None,
                stored_decimal: stored_decimal.to_string(),
                abs_error: None,
                rel_error: None,
                rel_error_pct: None,
                classification,
                exact: true,
                overflowed: false,
                underflowed_to_zero: false,
            }
        }
        ParsedInput::Zero { negative, .. } => {
            let (bits, sign_bit, exp_bits_str, frac_bits_str, bits_hex, bits_binary) =
                build_bit_strings(fmt, *negative, 0, 0);
            verify_with_primitive(fmt, bits, FpCategory::Zero, *negative);
            let classification = if *negative { "negative_zero" } else { "zero" };
            let stored_decimal = if *negative { "-0" } else { "0" };
            RoundedFloat {
                format: fmt,
                bits,
                sign_bit,
                exp_bits_str,
                frac_bits_str,
                bits_hex,
                bits_binary,
                unbiased_exp: None,
                significand: Some("0".to_string()),
                stored_decimal: stored_decimal.to_string(),
                abs_error: Some("0".to_string()),
                rel_error: None,
                rel_error_pct: None,
                classification,
                exact: true,
                overflowed: false,
                underflowed_to_zero: false,
            }
        }
        ParsedInput::Finite {
            negative,
            digits,
            exp10,
            mag_exp10,
            ..
        } => {
            if *mag_exp10 >= 40 {
                let (bits, sign_bit, exp_bits_str, frac_bits_str, bits_hex, bits_binary) =
                    build_bit_strings(fmt, *negative, max_exp_field, 0);
                verify_with_primitive(fmt, bits, FpCategory::Infinite, *negative);
                let classification = if *negative {
                    "negative_infinity"
                } else {
                    "infinity"
                };
                let stored_decimal = if *negative { "-Infinity" } else { "Infinity" };
                return RoundedFloat {
                    format: fmt,
                    bits,
                    sign_bit,
                    exp_bits_str,
                    frac_bits_str,
                    bits_hex,
                    bits_binary,
                    unbiased_exp: None,
                    significand: None,
                    stored_decimal: stored_decimal.to_string(),
                    abs_error: None,
                    rel_error: None,
                    rel_error_pct: None,
                    classification,
                    exact: false,
                    overflowed: true,
                    underflowed_to_zero: false,
                };
            }

            if *mag_exp10 <= -50 {
                let (bits, sign_bit, exp_bits_str, frac_bits_str, bits_hex, bits_binary) =
                    build_bit_strings(fmt, *negative, 0, 0);
                verify_with_primitive(fmt, bits, FpCategory::Zero, *negative);
                let classification = if *negative { "negative_zero" } else { "zero" };
                let stored_decimal = if *negative { "-0" } else { "0" };
                let abs_err = format_shifted_decimal(!*negative, digits, *exp10, *mag_exp10);
                return RoundedFloat {
                    format: fmt,
                    bits,
                    sign_bit,
                    exp_bits_str,
                    frac_bits_str,
                    bits_hex,
                    bits_binary,
                    unbiased_exp: None,
                    significand: Some("0".to_string()),
                    stored_decimal: stored_decimal.to_string(),
                    abs_error: Some(abs_err),
                    rel_error: Some("-1".to_string()),
                    rel_error_pct: Some("-100".to_string()),
                    classification,
                    exact: false,
                    overflowed: false,
                    underflowed_to_zero: true,
                };
            }

            // Build exact rational representation of the decimal literal.
            let d_val = BigUint::parse_bytes(digits.as_bytes(), 10)
                .expect("validated ASCII decimal digits");
            let ten = BigUint::from(10u32);
            let (p, q) = if *exp10 >= 0 {
                (d_val * ten.pow(*exp10 as u32), BigUint::one())
            } else {
                (d_val, ten.pow((-*exp10) as u32))
            };

            let (stored_exp, stored_frac, overflowed) = round_positive_rational_rtne(&p, &q, fmt);
            let (bits, sign_bit, exp_bits_str, frac_bits_str, bits_hex, bits_binary) =
                build_bit_strings(fmt, *negative, stored_exp, stored_frac);

            if overflowed {
                verify_with_primitive(fmt, bits, FpCategory::Infinite, *negative);
                let classification = if *negative {
                    "negative_infinity"
                } else {
                    "infinity"
                };
                let stored_decimal = if *negative { "-Infinity" } else { "Infinity" };
                return RoundedFloat {
                    format: fmt,
                    bits,
                    sign_bit,
                    exp_bits_str,
                    frac_bits_str,
                    bits_hex,
                    bits_binary,
                    unbiased_exp: None,
                    significand: None,
                    stored_decimal: stored_decimal.to_string(),
                    abs_error: None,
                    rel_error: None,
                    rel_error_pct: None,
                    classification,
                    exact: false,
                    overflowed: true,
                    underflowed_to_zero: false,
                };
            }

            let mag_in = BigRational::new(BigInt::from(p), BigInt::from(q));
            let input_rat = if *negative { -mag_in } else { mag_in };

            if stored_exp == 0 && stored_frac == 0 {
                verify_with_primitive(fmt, bits, FpCategory::Zero, *negative);
                let classification = if *negative { "negative_zero" } else { "zero" };
                let stored_decimal = if *negative { "-0" } else { "0" };
                let abs_err = -&input_rat;
                return RoundedFloat {
                    format: fmt,
                    bits,
                    sign_bit,
                    exp_bits_str,
                    frac_bits_str,
                    bits_hex,
                    bits_binary,
                    unbiased_exp: None,
                    significand: Some("0".to_string()),
                    stored_decimal: stored_decimal.to_string(),
                    abs_error: Some(format_rational_signed(&abs_err, true)),
                    rel_error: Some("-1".to_string()),
                    rel_error_pct: Some("-100".to_string()),
                    classification,
                    exact: false,
                    overflowed: false,
                    underflowed_to_zero: true,
                };
            }

            let is_subnormal = stored_exp == 0;
            let (expected_cat, classification, unbiased, mantissa_int) = if is_subnormal {
                (
                    FpCategory::Subnormal,
                    "subnormal",
                    fmt.min_normal_exp(),
                    stored_frac,
                )
            } else {
                (
                    FpCategory::Normal,
                    "normal",
                    (stored_exp as i32) - fmt.bias(),
                    (1u32 << fmt.frac_bits()) | stored_frac,
                )
            };
            verify_with_primitive(fmt, bits, expected_cat, *negative);

            let sig_rat = BigRational::new(
                BigInt::from(mantissa_int),
                BigInt::from(1u64 << fmt.frac_bits()),
            );
            let significand = Some(format_rational_signed(&sig_rat, false));

            let shift = unbiased - (fmt.frac_bits() as i32);
            let mag_num = BigInt::from(mantissa_int);
            let mag_stored = if shift >= 0 {
                BigRational::from_integer(mag_num << (shift as usize))
            } else {
                BigRational::new(mag_num, BigInt::from(1u32) << ((-shift) as usize))
            };
            let stored_rat = if *negative { -mag_stored } else { mag_stored };
            let stored_decimal = format_rational_signed(&stored_rat, false);

            let abs_err = &stored_rat - &input_rat;
            let exact = abs_err.is_zero();
            let rel_err = &abs_err / &input_rat;
            let rel_err_pct = &rel_err * BigInt::from(100u32);

            RoundedFloat {
                format: fmt,
                bits,
                sign_bit,
                exp_bits_str,
                frac_bits_str,
                bits_hex,
                bits_binary,
                unbiased_exp: Some(unbiased),
                significand,
                stored_decimal,
                abs_error: Some(format_rational_signed(&abs_err, true)),
                rel_error: Some(format_rational_signed(&rel_err, true)),
                rel_error_pct: Some(format_rational_signed(&rel_err_pct, true)),
                classification,
                exact,
                overflowed: false,
                underflowed_to_zero: false,
            }
        }
    }
}

fn build_candidate(
    query: &str,
    parsed_query: &ParsedQuery<'_>,
    parsed_input: &ParsedInput,
    target: FloatFormat,
) -> Candidate {
    let ordered = target.ordered_with_alternatives();
    let results: Vec<RoundedFloat> = ordered
        .iter()
        .map(|&fmt| round_input_to_format(parsed_input, fmt))
        .collect();

    let primary = &results[0];
    let mut primary_value = primary.to_json_value(true);
    if let Some(obj) = primary_value.as_object_mut() {
        obj.insert(
            "inputDecimal".to_string(),
            json!(parsed_input.normalized_literal()),
        );
        let alts: Vec<Value> = results
            .iter()
            .skip(1)
            .map(|res| res.to_json_value(false))
            .collect();
        obj.insert("alternatives".to_string(), Value::Array(alts));
    }

    let id = format!("float-{}", target.id());
    let normalized = format!("{} as {}", parsed_input.normalized_literal(), target.id());
    let interpretation = format!(
        "{} as {} ({}, {})",
        parsed_input.normalized_literal(),
        target.display_name(),
        primary.stored_decimal,
        primary.bits_hex
    );

    let mut candidate = Candidate::new(
        id,
        "float_format",
        &normalized,
        &interpretation,
        primary_value,
    );

    // Format rows for Alt+1..5 and Tab cycling: primary + alternative IEEE formats, then stored decimal & binary bits.
    for res in &results {
        candidate.formats.push(Format::new(
            res.format.id(),
            format!(
                "{} · {} ({})",
                res.format.display_name(),
                res.stored_decimal,
                res.bits_hex
            ),
            &res.bits_hex,
        ));
    }
    candidate.formats.push(Format::new(
        "stored_decimal",
        format!("Stored · {}", primary.stored_decimal),
        &primary.stored_decimal,
    ));
    candidate.formats.push(Format::new(
        "bits_binary",
        format!(
            "Bits · {} {} {}",
            primary.sign_bit, primary.exp_bits_str, primary.frac_bits_str
        ),
        &primary.bits_binary,
    ));

    let input_assumption = match parsed_input {
        ParsedInput::Infinity { .. } | ParsedInput::Nan { .. } => {
            "Special IEEE 754 floating-point literal".to_string()
        }
        _ => {
            "Decimal literal interpreted as exact rational (no intermediate float double rounding)"
                .to_string()
        }
    };
    candidate.assumptions.push(input_assumption);
    candidate
        .assumptions
        .push("Rounding mode: round-to-nearest, ties-to-even (RTNE)".to_string());
    candidate.assumptions.push(target.description().to_string());

    if primary.overflowed {
        candidate.assumptions.push(
            "Finite input exceeded target max finite threshold and rounded to IEEE infinity"
                .to_string(),
        );
        let mut warn = Diagnostic::warning("numeric_overflow");
        warn.parameters = json!({
            "format": target.id(),
            "input": parsed_input.normalized_literal(),
            "result": primary.stored_decimal,
        });
        warn.span = Some(utf16_span(
            query,
            parsed_query.lhs_orig_start,
            parsed_query.lhs_orig_end,
        ));
        candidate.diagnostics.push(warn);
    } else if primary.underflowed_to_zero {
        candidate.assumptions.push(
            "Finite nonzero input underflowed below half minimum subnormal to signed zero"
                .to_string(),
        );
    }

    candidate.provenance = Some(json!({
        "kind": "ieee754_rational_rounder",
        "version": "1",
        "format": target.id(),
        "roundingMode": "round_to_nearest_ties_to_even",
    }));

    candidate
}

pub(crate) fn preview(query: &str) -> Option<Result<Vec<Candidate>, Diagnostic>> {
    let norm = normalize_query(query);
    let parsed_query = split_as_clause(&norm)?;

    if query.len() > MAX_QUERY_BYTES {
        let mut diag = Diagnostic::error("resource_limit");
        diag.parameters = json!({
            "limitBytes": MAX_QUERY_BYTES,
            "actualBytes": query.len(),
        });
        diag.span = Some(json!({
            "startUtf16": 0,
            "endUtf16": query.encode_utf16().count(),
        }));
        return Some(Err(diag));
    }

    if query.chars().any(char::is_control) {
        let mut diag = Diagnostic::error("invalid_syntax");
        diag.parameters = json!({ "reason": "control_character" });
        diag.span = Some(json!({
            "startUtf16": 0,
            "endUtf16": query.encode_utf16().count(),
        }));
        return Some(Err(diag));
    }

    let target = match &parsed_query.target {
        TargetFormat::Supported(fmt) => *fmt,
        TargetFormat::Unsupported {
            raw,
            canonical,
            is_fp8,
        } => {
            let mut diag = Diagnostic::error("unsupported_format");
            diag.parameters = json!({
                "format": raw,
                "canonicalFormat": canonical,
                "deferred": is_fp8,
                "supportedFormats": ["fp32", "fp16", "bf16"],
            });
            diag.span = Some(utf16_span(
                query,
                parsed_query.target_orig_start,
                parsed_query.target_orig_end,
            ));
            return Some(Err(diag));
        }
    };

    let parsed_input = match parse_decimal_input(parsed_query.lhs) {
        Ok(input) => input,
        Err(kind) => {
            let code = match kind {
                ParseErrorKind::Incomplete => "incomplete_input",
                ParseErrorKind::InvalidSyntax => "invalid_syntax",
                ParseErrorKind::ResourceLimit => "resource_limit",
            };
            let mut diag = Diagnostic::error(code);
            diag.parameters = json!({
                "targetFormat": target.id(),
                "input": parsed_query.lhs,
            });
            diag.span = Some(utf16_span(
                query,
                parsed_query.lhs_orig_start,
                parsed_query.lhs_orig_end,
            ));
            return Some(Err(diag));
        }
    };

    Some(Ok(vec![build_candidate(
        query,
        &parsed_query,
        &parsed_input,
        target,
    )]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval_value(query: &str) -> (RoundedFloat, Value, Candidate) {
        let norm = normalize_query(query);
        let pq = split_as_clause(&norm).expect("float query");
        let target = match pq.target {
            TargetFormat::Supported(f) => f,
            _ => panic!("expected supported format"),
        };
        let input = parse_decimal_input(pq.lhs).expect("valid decimal input");
        let rounded = round_input_to_format(&input, target);
        let json_val = rounded.to_json_value(true);
        let cand = build_candidate(query, &pq, &input, target);
        (rounded, json_val, cand)
    }

    #[test]
    fn golden_0_1_as_bf16() {
        let res = preview("0.1 as bf16").expect("matched").expect("ok");
        assert_eq!(res.len(), 1);
        let cand = &res[0];
        assert_eq!(cand.formats.len(), 5);
        assert!(cand.diagnostics.is_empty());
        assert_eq!(
            cand.provenance
                .as_ref()
                .and_then(|p| p.get("format"))
                .and_then(Value::as_str),
            Some("bf16")
        );

        let (rounded, val, _) = eval_value("0.1 as bf16");
        assert_eq!(rounded.bits, 0x3DCD);
        assert_eq!(val["tag"], "float_bits");
        assert_eq!(val["format"], "bf16");
        assert_eq!(val["bitsHex"], "0x3DCD");
        assert_eq!(val["bitsBinary"], "0011110111001101");
        assert_eq!(val["signBit"], "0");
        assert_eq!(val["exponentBits"], "01111011");
        assert_eq!(val["fractionBits"], "1001101");
        assert_eq!(val["unbiasedExponent"], -4);
        assert_eq!(val["significand"], "1.6015625");
        assert_eq!(val["storedDecimal"], "0.10009765625");
        assert_eq!(val["absoluteError"], "+0.00009765625");
        assert_eq!(val["relativeError"], "+0.0009765625");
        assert_eq!(val["relativeErrorPercent"], "+0.09765625");
        assert_eq!(val["classification"], "normal");
        assert_eq!(val["roundingMode"], "round_to_nearest_ties_to_even");
        assert_eq!(val["exact"], false);
    }

    #[test]
    fn golden_0_1_as_fp16_and_fp32() {
        let (fp16_res, fp16_val, _) = eval_value("0.1 as fp16");
        assert_eq!(fp16_res.bits, 0x2E66);
        assert_eq!(fp16_val["bitsHex"], "0x2E66");
        assert_eq!(fp16_val["storedDecimal"], "0.0999755859375");
        assert_eq!(fp16_val["absoluteError"], "-0.0000244140625");
        assert_eq!(fp16_val["classification"], "normal");

        let (fp32_res, fp32_val, _) = eval_value("0.1 as fp32");
        assert_eq!(fp32_res.bits, 0x3DCCCCCD);
        assert_eq!(fp32_val["bitsHex"], "0x3DCCCCCD");
        assert_eq!(fp32_val["storedDecimal"], "0.100000001490116119384765625");
        assert_eq!(fp32_val["absoluteError"], "+0.000000001490116119384765625");
        assert_eq!(fp32_val["classification"], "normal");
    }

    #[test]
    fn avoids_double_rounding_via_f64_and_f32() {
        // Midpoint between BF16 0x3F81 (1 + 1/128, odd) and 0x3F82 (1 + 2/128, even) is 1.01171875.
        // Subtracting 1e-17 makes it < midpoint, while f64/f32 would round UP onto 1.01171875
        // and then double-round tie-to-even to 0x3F82. Exact rational rounding yields 0x3F81.
        let bf16_input = "1.01171874999999999";
        let as_f64: f64 = bf16_input.parse().unwrap();
        assert_eq!(bf16::from_f64(as_f64).to_bits(), 0x3F82);
        let (bf16_exact, _, _) = eval_value(&format!("{bf16_input} as bf16"));
        assert_eq!(bf16_exact.bits, 0x3F81);

        // Above midpoint: 1.01171875 + 1e-17 rounds to 0x3F82, even for even-to-odd boundary.
        // Midpoint between 0x3F80 (even) and 0x3F81 (odd) is 1.00390625.
        // 1.00390625000000001 > midpoint -> f64 double-rounds to 0x3F80, exact rounds to 0x3F81.
        let bf16_above = "1.00390625000000001";
        let as_f64_above: f64 = bf16_above.parse().unwrap();
        assert_eq!(bf16::from_f64(as_f64_above).to_bits(), 0x3F80);
        let (bf16_exact_above, _, _) = eval_value(&format!("{bf16_above} as bf16"));
        assert_eq!(bf16_exact_above.bits, 0x3F81);

        // FP16 midpoint between 0x3C01 (odd) and 0x3C02 (even) is 1 + 3/2048 = 1.00146484375.
        let fp16_input = "1.00146484374999999";
        let as_f64_16: f64 = fp16_input.parse().unwrap();
        assert_eq!(f16::from_f64(as_f64_16).to_bits(), 0x3C02);
        let (fp16_exact, _, _) = eval_value(&format!("{fp16_input} as fp16"));
        assert_eq!(fp16_exact.bits, 0x3C01);

        // FP32 midpoint between 0x3F800001 (odd) and 0x3F800002 (even) is 1 + 3 * 2^-24.
        let fp32_input = "1.000000178813934316171875";
        let as_f64_32: f64 = fp32_input.parse().unwrap();
        assert_eq!((as_f64_32 as f32).to_bits(), 0x3F80_0002);
        let (fp32_exact, _, _) = eval_value(&format!("{fp32_input} as fp32"));
        assert_eq!(fp32_exact.bits, 0x3F80_0001);
    }

    #[test]
    fn rtne_exact_ties() {
        // 1.5 in BF16 is exact, 1.00390625 is midpoint between 0x3F80 (even) and 0x3F81 (odd) -> 0x3F80.
        let (tie_even, _, _) = eval_value("1.00390625 as bf16");
        assert_eq!(tie_even.bits, 0x3F80);

        // 1.01171875 is midpoint between 0x3F81 (odd) and 0x3F82 (even) -> 0x3F82.
        let (tie_odd, _, _) = eval_value("1.01171875 as bf16");
        assert_eq!(tie_odd.bits, 0x3F82);
    }

    #[test]
    fn signed_zero_and_omitted_relative_error() {
        for q in ["-0 as fp16", "-0.0 as fp16", "−0.00 as f16"] {
            let (res, val, _) = eval_value(q);
            assert_eq!(res.bits, 0x8000);
            assert_eq!(val["signBit"], "1");
            assert_eq!(val["storedDecimal"], "-0");
            assert_eq!(val["classification"], "negative_zero");
            assert_eq!(val["absoluteError"], "0");
            assert!(val.get("relativeError").is_none());
            assert!(val.get("relativeErrorPercent").is_none());
            assert_eq!(val["exact"], true);
        }

        let (pos_zero, val, _) = eval_value("0 as bf16");
        assert_eq!(pos_zero.bits, 0x0000);
        assert_eq!(val["signBit"], "0");
        assert_eq!(val["classification"], "zero");
        assert!(val.get("relativeError").is_none());
    }

    #[test]
    fn subnormal_boundaries_and_carry_to_normal() {
        // FP16 min subnormal = 2^-24 = 0.000000059604644775390625
        let (min_sub, val, _) = eval_value("0.000000059604644775390625 as fp16");
        assert_eq!(min_sub.bits, 0x0001);
        assert_eq!(val["classification"], "subnormal");
        assert_eq!(val["unbiasedExponent"], -14);
        assert_eq!(val["exact"], true);

        // Half of FP16 min subnormal = 2^-25 rounds to even (0x0000).
        let (half_min_sub, val_zero, _) = eval_value("0.0000000298023223876953125 as fp16");
        assert_eq!(half_min_sub.bits, 0x0000);
        assert_eq!(val_zero["classification"], "zero");
        assert_eq!(val_zero["relativeError"], "-1");
        assert_eq!(val_zero["relativeErrorPercent"], "-100");

        // Just above half min subnormal -> rounds up to min subnormal 0x0001.
        let (above_half, _, _) = eval_value("0.00000003 as fp16");
        assert_eq!(above_half.bits, 0x0001);

        // FP16 min normal = 2^-14 = 0.00006103515625
        let (min_norm, val_norm, _) = eval_value("0.00006103515625 as fp16");
        assert_eq!(min_norm.bits, 0x0400);
        assert_eq!(val_norm["classification"], "normal");
        assert_eq!(val_norm["unbiasedExponent"], -14);

        // Midpoint between FP16 max subnormal (1023 * 2^-24) and min normal (1024 * 2^-24)
        // = 1023.5 * 2^-24 = 2047 * 2^-25 = 0.0000610053539276123046875 -> rounds to even (min normal 0x0400)!
        let (sub_to_norm, val_s2n, _) = eval_value("0.0000610053539276123046875 as fp16");
        assert_eq!(sub_to_norm.bits, 0x0400);
        assert_eq!(val_s2n["classification"], "normal");
    }

    #[test]
    fn infinity_nan_and_overflow_behavior() {
        let (inf_p, val_inf, _) = eval_value("inf as fp32");
        assert_eq!(inf_p.bits, 0x7F80_0000);
        assert_eq!(val_inf["classification"], "infinity");
        assert!(val_inf.get("relativeError").is_none());
        assert!(val_inf.get("absoluteError").is_none());

        let (inf_n, val_ninf, _) = eval_value("-infinity as fp16");
        assert_eq!(inf_n.bits, 0xFC00);
        assert_eq!(val_ninf["classification"], "negative_infinity");

        let (nan_p, val_nan, _) = eval_value("NaN as bf16");
        assert_eq!(nan_p.bits, 0x7FC0);
        assert_eq!(val_nan["classification"], "nan");
        assert_eq!(val_nan["signBit"], "0");
        assert!(val_nan.get("relativeError").is_none());

        let (nan_n, val_nnan, _) = eval_value("-nan as fp16");
        assert_eq!(nan_n.bits, 0xFE00);
        assert_eq!(val_nnan["classification"], "nan");
        assert_eq!(val_nnan["signBit"], "1");

        // FP16 max finite is 65504; overflow threshold is 65520.
        let (fp16_max, _, cand_max) = eval_value("65519.9 as fp16");
        assert_eq!(fp16_max.bits, 0x7BFF);
        assert!(cand_max.diagnostics.is_empty());

        let (fp16_ovf, val_ovf, cand_ovf) = eval_value("65520 as fp16");
        assert_eq!(fp16_ovf.bits, 0x7C00);
        assert_eq!(val_ovf["classification"], "infinity");
        assert_eq!(cand_ovf.diagnostics.len(), 1);

        // Huge exponent does not hang or allocate huge BigInt.
        let (huge_ovf, val_huge, _) = eval_value("1e100000 as fp32");
        assert_eq!(huge_ovf.bits, 0x7F80_0000);
        assert_eq!(val_huge["classification"], "infinity");

        // Tiny exponent underflows cleanly to negative zero.
        let (tiny_unf, val_tiny, _) = eval_value("-1e-100000 as fp32");
        assert_eq!(tiny_unf.bits, 0x8000_0000);
        assert_eq!(val_tiny["classification"], "negative_zero");
        assert_eq!(val_tiny["relativeError"], "-1");
        assert_eq!(val_tiny["relativeErrorPercent"], "-100");
    }

    #[test]
    fn roundtrip_representative_corpus_with_half_and_f32() {
        let samples = [
            "0",
            "-0",
            "1",
            "-1",
            "0.5",
            "0.1",
            "-123.456",
            "1e-5",
            "65504",
            "3.14159265",
            "0.00006103515625",
            "1e-40",
            "inf",
            "-inf",
            "nan",
            "-nan",
        ];
        for s in samples {
            for fmt in ["fp32", "fp16", "bf16"] {
                let q = format!("{s} as {fmt}");
                let (res, _, _) = eval_value(&q);
                match fmt {
                    "fp32" => assert_eq!(f32::from_bits(res.bits).to_bits(), res.bits),
                    "fp16" => {
                        assert_eq!(f16::from_bits(res.bits as u16).to_bits(), res.bits as u16)
                    }
                    "bf16" => {
                        assert_eq!(bf16::from_bits(res.bits as u16).to_bits(), res.bits as u16)
                    }
                    _ => unreachable!(),
                }
            }
        }
    }

    #[test]
    fn deferred_fp8_and_typed_errors() {
        for q in [
            "0.1 as fp8",
            "0.1 as fp8 e4m3fn",
            "0.1 as fp8_e5m2",
            "0.1 as f64",
        ] {
            let err = preview(q).expect("matched").unwrap_err();
            assert_eq!(
                err.parameters["supportedFormats"],
                json!(["fp32", "fp16", "bf16"])
            );
            assert!(err.span.is_some());
        }

        let incomplete = preview("as bf16").expect("matched").unwrap_err();
        assert_eq!(incomplete.parameters["targetFormat"], "bf16");
        assert!(incomplete.span.is_some());

        let incomplete_exp = preview("1e- as fp16").expect("matched").unwrap_err();
        assert_eq!(incomplete_exp.parameters["targetFormat"], "fp16");

        let invalid = preview("abc as bf16").expect("matched").unwrap_err();
        assert_eq!(invalid.parameters["input"], "abc");

        let invalid_underscore = preview("1__0 as fp32").expect("matched").unwrap_err();
        assert_eq!(invalid_underscore.parameters["targetFormat"], "fp32");

        let huge_exp = preview("1e99999999999999999999 as bf16")
            .expect("matched")
            .unwrap_err();
        assert_eq!(huge_exp.parameters["targetFormat"], "bf16");
    }

    #[test]
    fn non_float_queries_return_none() {
        assert!(preview("").is_none());
        assert!(preview("   ").is_none());
        assert!(preview("1 + 2").is_none());
        assert!(preview("wallpaper-bf16.png").is_none());
        assert!(preview("0xff+1 as u8").is_none());
        assert!(preview("0.1 as").is_none());
        assert!(preview("100 Mbps to MB/s").is_none());
    }

    #[test]
    fn normalizes_unicode_minus_and_underscores() {
        let (res_u, _, _) = eval_value("−1_000.5 as fp32");
        let (res_a, _, _) = eval_value("-1000.5 as fp32");
        assert_eq!(res_u.bits, res_a.bits);
        assert_eq!(res_u.bits, (-1000.5f32).to_bits());
    }

    #[test]
    fn fp32_and_bf16_exact_overflow_thresholds() {
        // FP32 overflow threshold = (2^25 - 1) * 2^103
        let (below_32, val_b32, cand_b32) =
            eval_value("340282356779733661637539395458142568447 as fp32");
        assert_eq!(below_32.bits, 0x7F7F_FFFF);
        assert_eq!(val_b32["classification"], "normal");
        assert!(cand_b32.diagnostics.is_empty());

        let (at_32, val_a32, cand_a32) =
            eval_value("340282356779733661637539395458142568448 as fp32");
        assert_eq!(at_32.bits, 0x7F80_0000);
        assert_eq!(val_a32["classification"], "infinity");
        assert_eq!(cand_a32.diagnostics.len(), 1);

        // BF16 overflow threshold = 511 * 2^119
        let (below_bf16, _, cand_bbf16) =
            eval_value("339617752923046005526922703901628039167 as bf16");
        assert_eq!(below_bf16.bits, 0x7F7F);
        assert!(cand_bbf16.diagnostics.is_empty());

        let (at_bf16, val_abf16, cand_abf16) =
            eval_value("-339617752923046005526922703901628039168 as bf16");
        assert_eq!(at_bf16.bits, 0xFF80);
        assert_eq!(val_abf16["classification"], "negative_infinity");
        assert_eq!(cand_abf16.diagnostics.len(), 1);
    }

    #[test]
    fn repeating_relative_error_and_limits_and_utf16_spans() {
        let (res_03, val_03, _) = eval_value("0.3 as bf16");
        assert_eq!(res_03.bits, 0x3E9A);
        assert_eq!(val_03["storedDecimal"], "0.30078125");
        assert_eq!(val_03["absoluteError"], "+0.00078125");
        assert!(
            val_03["relativeError"]
                .as_str()
                .unwrap()
                .starts_with("+0.002604166666666")
        );

        let oversized = format!("{}1 as bf16", "0".repeat(4100));
        let err_len = preview(&oversized).expect("matched").unwrap_err();
        assert_eq!(err_len.parameters["limitBytes"], 4096);

        let err_ctrl = preview("0.1\0 as bf16").expect("matched").unwrap_err();
        assert_eq!(err_ctrl.parameters["reason"], "control_character");

        // Surrogate-pair emoji (2 UTF-16 units) + " as " (4 UTF-16 units) -> "fp8" at 6..9.
        let err_span = preview("😀 as fp8").expect("matched").unwrap_err();
        assert_eq!(
            err_span.span,
            Some(json!({ "startUtf16": 6, "endUtf16": 9 }))
        );
    }
}
