use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use fend_core::{Context, Interrupt, evaluate_preview_with_interrupt, evaluate_with_interrupt};
use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::Zero;
use serde_json::json;

use crate::protocol::{Candidate, Diagnostic, Format};

pub(crate) struct Budget<'a> {
    pub(crate) revision: u64,
    pub(crate) latest: &'a AtomicU64,
    deadline: Instant,
}

impl<'a> Budget<'a> {
    pub(crate) fn new(revision: u64, latest: &'a AtomicU64) -> Self {
        Self {
            revision,
            latest,
            deadline: Instant::now() + Duration::from_millis(100),
        }
    }

    pub(crate) fn check(&self) -> Result<(), Diagnostic> {
        if self.latest.load(Ordering::Acquire) > self.revision {
            return Err(Diagnostic::error("interrupted"));
        }
        if Instant::now() >= self.deadline {
            return Err(Diagnostic::error("resource_limit"));
        }
        Ok(())
    }
}

impl Interrupt for Budget<'_> {
    fn should_interrupt(&self) -> bool {
        self.latest.load(Ordering::Relaxed) > self.revision || Instant::now() >= self.deadline
    }
}

pub(crate) fn normalize(raw: &str, max: usize) -> Result<String, Diagnostic> {
    if raw.len() > max {
        return Err(Diagnostic::error("resource_limit"));
    }
    if raw
        .chars()
        .any(|c| c.is_control() || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'))
    {
        return Err(Diagnostic::error("invalid_syntax"));
    }

    // Every replacement preserves its UTF-16 length, even when its UTF-8 length changes.
    let normalized: String = raw
        .chars()
        .map(|c| match c {
            '−' => '-',
            '×' => '*',
            '÷' => '/',
            ' ' | '\u{00a0}' | '\u{202f}' => ' ',
            '０'..='９' => char::from(b'0' + (c as u32 - '０' as u32) as u8),
            _ => c,
        })
        .collect();

    let mut depth = 0usize;
    let mut tokens = 0usize;
    let mut in_word = false;
    for c in normalized.chars() {
        let word = c.is_alphanumeric() || c == '_';
        if (word && !in_word) || (!word && !c.is_whitespace()) {
            tokens += 1;
        }
        in_word = word;
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ => {}
        }
        if depth > 32 || tokens > 128 {
            return Err(Diagnostic::error("resource_limit"));
        }
    }
    Ok(normalized)
}

pub(crate) fn admits_all(query: &str) -> bool {
    let query = query.trim();
    if query.starts_with(['/', '~', '>', '$', '.'])
        || query.contains("://")
        || query.contains('\\')
        || query.contains('@')
    {
        return false;
    }
    if let Some((left, right)) = query.split_once('/')
        && left.chars().any(char::is_alphabetic)
        && right.chars().any(char::is_alphabetic)
        && !query.contains(' ')
    {
        return false;
    }
    (query.starts_with(|c: char| c.is_ascii_digit() || matches!(c, '(' | '-' | '+'))
        || query.starts_with("sqrt")
        || query.starts_with("pi ")
        || query.starts_with('π'))
        && (query.contains(['+', '-', '*', '/', '^', '%', '(', '&', '|', '<', '>'])
            || query
                .split_whitespace()
                .any(|token| matches!(token, "to" | "in" | "as")))
        && query.chars().any(|c| c.is_ascii_digit())
}

fn fend_input(input: &str) -> (String, String) {
    if let Some((left, right)) = input.split_once(" sqrt ")
        && let Some(last) = left.chars().next_back()
        && (last.is_alphanumeric() || last == ')')
        && !left.trim_start().starts_with("sqrt")
    {
        return (
            format!("{left} * sqrt {right}"),
            format!("{left} × √{right}").replace("pi", "π"),
        );
    }
    (input.into(), input.into())
}

pub(crate) fn preview(
    query: &str,
    context: &Context,
    budget: &Budget<'_>,
) -> Result<Candidate, Diagnostic> {
    let query = query.trim();
    if query.ends_with(['+', '-', '*', '/', '^', '=', '('])
        || query
            .split_whitespace()
            .next_back()
            .is_some_and(|tail| matches!(tail, "sqrt" | "to" | "in" | "as"))
    {
        return Err(Diagnostic::warning("incomplete_input"));
    }
    if query.contains(['@', '\\', '"', '\'']) {
        return Err(Diagnostic::error("invalid_syntax"));
    }
    for part in query.split('^').skip(1) {
        let exponent: String = part
            .trim_start_matches([' ', '+', '-', '('])
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if !exponent.is_empty() && exponent.parse::<u32>().unwrap_or(u32::MAX) > 10_000 {
            return Err(Diagnostic::error("resource_limit"));
        }
    }

    let (input, interpretation) = fend_input(query);
    let result = evaluate_preview_with_interrupt(&input, context, budget);
    let fallback;
    let result = if result.output_is_empty() {
        fallback =
            evaluate_with_interrupt(&input, &mut context.clone(), budget).map_err(|error| {
                if let Err(interrupt) = budget.check() {
                    return interrupt;
                }
                let code = if error.starts_with("unknown identifier") {
                    "unknown_identifier"
                } else if error.contains("exponent too large") {
                    "resource_limit"
                } else {
                    "invalid_syntax"
                };
                Diagnostic::error(code)
            })?;
        &fallback
    } else {
        &result
    };
    budget.check()?;
    let display = result.get_main_result();
    if result.output_is_empty() || display.contains(char::is_control) {
        return Err(Diagnostic::error("invalid_syntax"));
    }
    if display.len() > 4096 {
        return Err(Diagnostic::error("resource_limit"));
    }
    let value = exact(&input).map_or_else(
        || json!({"tag":"display_only","text":display,"engine":"fend-core","limitations":["formatted_engine_output"]}),
        |number| exact_value(number, display),
    );
    let mut candidate = Candidate::new("math", "math", &input, &interpretation, value);
    candidate
        .formats
        .push(Format::new("default", display, display));
    candidate.provenance = Some(
        json!({"kind":"expression_engine","engine":"fend-core","version":fend_core::get_version()}),
    );
    Ok(candidate)
}

fn exact_value(number: BigRational, display: &str) -> serde_json::Value {
    if number.is_integer() {
        return json!({"tag":"integer","decimal":number.to_integer().to_string(),"signed":true,"radix":10,"display":display});
    }
    let mut denominator = number.denom().clone();
    let base = BigInt::from(10);
    let mut scale = BigInt::from(1);
    for digits in 0..=64 {
        if denominator == BigInt::from(1) {
            let scaled = (number.numer() * &scale / number.denom()).to_string();
            let (sign, magnitude) = scaled
                .strip_prefix('-')
                .map_or(("", scaled.as_str()), |s| ("-", s));
            let padded = format!("{magnitude:0>width$}", width = digits + 1);
            let (whole, fraction) = padded.split_at(padded.len() - digits);
            return json!({"tag":"decimal","decimal":format!("{sign}{whole}.{fraction}"),"exact":true,"precisionDigits":digits,"display":display});
        }
        let before = denominator.clone();
        for prime in [2, 5] {
            if (&denominator % prime as u32).is_zero() {
                denominator /= prime;
            }
        }
        if denominator == before {
            break;
        }
        scale *= &base;
    }
    json!({"tag":"rational","numerator":number.numer().to_string(),"denominator":number.denom().to_string(),"display":display})
}

// Independently type bounded decimal arithmetic. Fend remains authoritative for other syntax.
fn exact(input: &str) -> Option<BigRational> {
    fn expression(input: &mut &str, minimum: u8) -> Option<BigRational> {
        *input = input.trim_start();
        let mut left = if let Some(rest) = input.strip_prefix('(') {
            *input = rest;
            let value = expression(input, 0)?;
            *input = input.trim_start().strip_prefix(')')?;
            value
        } else if let Some(rest) = input.strip_prefix('-') {
            *input = rest;
            -expression(input, 3)?
        } else if let Some(rest) = input.strip_prefix('+') {
            *input = rest;
            expression(input, 3)?
        } else {
            let length = input
                .bytes()
                .take_while(|b| b.is_ascii_digit() || *b == b'.')
                .count();
            let (number, rest) = input.split_at(length);
            *input = rest;
            if number.len() > 128 {
                return None;
            }
            let (whole, frac) = number.split_once('.').unwrap_or((number, ""));
            let numerator = format!("{whole}{frac}").parse::<BigInt>().ok()?;
            BigRational::new(numerator, BigInt::from(10).pow(frac.len() as u32))
        };
        loop {
            *input = input.trim_start();
            let (precedence, operation) = match input.as_bytes().first() {
                Some(b'+' | b'-') => (1, input.as_bytes()[0]),
                Some(b'*' | b'/') => (2, input.as_bytes()[0]),
                Some(b'(') => (2, b'('),
                _ => break,
            };
            if precedence < minimum {
                break;
            }
            if operation != b'(' {
                *input = &input[1..];
            }
            let right = expression(input, precedence + 1)?;
            left = match operation {
                b'+' => left + right,
                b'-' => left - right,
                b'/' if right.is_zero() => return None,
                b'/' => left / right,
                _ => left * right,
            };
        }
        Some(left)
    }
    let mut tail = input;
    let result = expression(&mut tail, 0)?;
    tail.is_empty().then_some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expression_core_and_isolated_previews() {
        let revision = AtomicU64::new(1);
        let budget = Budget::new(1, &revision);
        let context = Context::new();
        for (input, tag, text) in [
            ("1+1", "integer", "2"),
            ("3pi sqrt 2", "display_only", "approx. 13.3286488145"),
            ("(42-7)*1.08", "decimal", "37.8"),
            ("2(3+4)", "integer", "14"),
            ("20% of 80", "display_only", "16"),
            ("x=2", "display_only", "2"),
        ] {
            let result = preview(input, &context, &budget).unwrap();
            assert_eq!(result.value["tag"], tag, "{input}");
            assert_eq!(result.formats[0].copy_text, text, "{input}");
        }
        assert!(preview("x", &context, &budget).is_err());
        assert!(preview("roll d6", &context, &budget).is_err());
        assert_eq!(
            preview("3pi sqrt", &context, &budget).unwrap_err().code,
            "incomplete_input"
        );
        revision.store(2, Ordering::Release);
        assert_eq!(
            preview("1+1", &context, &budget).unwrap_err().code,
            "interrupted"
        );
    }
}
