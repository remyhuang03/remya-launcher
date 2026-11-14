use num_bigint::BigInt;
use num_traits::{One, Signed, ToPrimitive, Zero};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::protocol::{Candidate, Diagnostic, Format};

const MAX_QUERY_BYTES: usize = 4096;
const MAX_TOKENS: usize = 128;
const MAX_DEPTH: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum OverflowPolicy {
    Error,
    Wrap,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProgrammerSettings {
    pub(crate) bit_width: u8,
    pub(crate) signed: bool,
    pub(crate) overflow: OverflowPolicy,
}

impl Default for ProgrammerSettings {
    fn default() -> Self {
        Self {
            bit_width: 32,
            signed: false,
            overflow: OverflowPolicy::Error,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Span {
    start_utf16: usize,
    end_utf16: usize,
}

impl Span {
    fn to_json(self) -> Value {
        json!({
            "startUtf16": self.start_utf16,
            "endUtf16": self.end_utf16,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OpKind {
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Tilde,
    Amp,
    Pipe,
    Caret,
    Xor,
    Shl,
    Shr,
    LParen,
    RParen,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TokenKind {
    IntLiteral { value: BigInt, radix: u8 },
    InvalidLiteral { code: &'static str, radix: u8 },
    Op(OpKind),
    Ident(String),
    UnknownChar(char),
}

#[derive(Clone, Debug)]
struct Token {
    kind: TokenKind,
    span: Span,
}

#[derive(Clone, Copy)]
struct NormChar {
    ch: char,
    utf16_pos: usize,
    utf16_len: usize,
}

pub(crate) fn preview(
    query: &str,
    settings: ProgrammerSettings,
) -> Option<Result<Vec<Candidate>, Diagnostic>> {
    if is_non_programmer_fast_reject(query) {
        return None;
    }

    if query.len() > MAX_QUERY_BYTES {
        if has_programmer_hint(query) {
            return Some(Err(limit_diagnostic("query_bytes", MAX_QUERY_BYTES)));
        }
        return None;
    }

    if query.chars().any(|c| c.is_control()) {
        if has_programmer_hint(query) {
            return Some(Err(Diagnostic::error("invalid_syntax")));
        }
        return None;
    }

    let (tokens, primary_radix, normalized_query) = match lex(query) {
        Ok(res) => res,
        Err(diag) => return Some(Err(diag)),
    };

    if !is_programmer_query(&tokens) {
        return None;
    }

    if !matches!(settings.bit_width, 8 | 16 | 32 | 64 | 128) {
        let mut diag = Diagnostic::error("unsupported_format");
        diag.parameters = json!({ "bitWidth": settings.bit_width });
        return Some(Err(diag));
    }

    if tokens.len() > MAX_TOKENS {
        return Some(Err(limit_diagnostic("token_count", MAX_TOKENS)));
    }

    let mut parser = Parser::new(&tokens, settings, query.encode_utf16().count());
    let value = match parser.parse_full() {
        Ok(v) => v,
        Err(diag) => return Some(Err(diag)),
    };

    Some(Ok(vec![build_candidate(
        &normalized_query,
        &value,
        primary_radix,
        settings,
    )]))
}

fn is_non_programmer_fast_reject(query: &str) -> bool {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return true;
    }
    if trimmed.starts_with('>') || trimmed.starts_with('$') {
        return true;
    }
    if trimmed.contains("://") {
        return true;
    }
    if trimmed.starts_with('/')
        || trimmed.starts_with("./")
        || trimmed.starts_with("../")
        || trimmed.starts_with("~/")
        || trimmed.contains('\\')
    {
        return true;
    }
    if has_float_format_suffix(trimmed) {
        return true;
    }
    false
}

fn has_float_format_suffix(trimmed: &str) -> bool {
    const FLOAT_FORMATS: &[&str] = &["bf16", "fp16", "f16", "fp32", "fp8", "e4m3fn", "e5m2"];
    let lower = trimmed.to_ascii_lowercase();
    for fmt in FLOAT_FORMATS {
        let pat = format!(" as {fmt}");
        if lower.ends_with(&pat) || lower.contains(&format!("{pat} ")) {
            return true;
        }
    }
    false
}

fn has_programmer_hint(query: &str) -> bool {
    query.contains("0x")
        || query.contains("0X")
        || query.contains("0b")
        || query.contains("0B")
        || query.contains("0o")
        || query.contains("0O")
        || query.contains("<<")
        || query.contains(">>")
        || query.contains('~')
        || query.contains('&')
        || query.contains('|')
}

fn is_programmer_query(tokens: &[Token]) -> bool {
    let mut has_prefixed_radix = false;
    let mut has_strong_bitwise_op = false;
    let mut has_weak_bitwise_op = false;
    let mut has_int_literal = false;
    let mut has_ident = false;

    for tok in tokens {
        match &tok.kind {
            TokenKind::IntLiteral { radix, .. } => {
                has_int_literal = true;
                if *radix != 10 {
                    has_prefixed_radix = true;
                }
            }
            TokenKind::InvalidLiteral { radix, .. } => {
                if *radix != 10 {
                    has_prefixed_radix = true;
                }
            }
            TokenKind::Op(OpKind::Shl | OpKind::Shr | OpKind::Tilde) => {
                has_strong_bitwise_op = true;
            }
            TokenKind::Op(OpKind::Amp | OpKind::Pipe | OpKind::Xor) => {
                has_weak_bitwise_op = true;
            }
            TokenKind::Ident(_) => {
                has_ident = true;
            }
            _ => {}
        }
    }

    if has_prefixed_radix || has_strong_bitwise_op {
        return true;
    }

    // Weak bitwise operators (&, |, xor) route to programmer only when there are
    // integer literals and no prose identifiers (avoids "cats & dogs" or "AT&T").
    has_weak_bitwise_op && has_int_literal && !has_ident
}

fn lex(query: &str) -> Result<(Vec<Token>, u8, String), Diagnostic> {
    let mut chars = Vec::with_capacity(query.len());
    let mut utf16_pos = 0usize;
    let mut normalized = String::with_capacity(query.len());

    for raw_ch in query.chars() {
        let ch = match raw_ch {
            '\u{2212}' => '-',
            '\u{00D7}' => '*',
            '\u{00F7}' => '/',
            '\u{00A0}' => ' ',
            other => other,
        };
        let utf16_len = raw_ch.len_utf16();
        chars.push(NormChar {
            ch,
            utf16_pos,
            utf16_len,
        });
        utf16_pos += utf16_len;
        normalized.push(ch);
    }

    let mut tokens = Vec::new();
    let mut primary_radix: Option<u8> = None;
    let mut i = 0usize;

    while i < chars.len() {
        let nc = chars[i];
        if nc.ch.is_whitespace() {
            i += 1;
            continue;
        }

        if tokens.len() > MAX_TOKENS {
            return Err(limit_diagnostic("token_count", MAX_TOKENS));
        }

        // Two-character operators: <<, >>
        if i + 1 < chars.len() {
            let next = chars[i + 1];
            if nc.ch == '<' && next.ch == '<' {
                tokens.push(Token {
                    kind: TokenKind::Op(OpKind::Shl),
                    span: Span {
                        start_utf16: nc.utf16_pos,
                        end_utf16: next.utf16_pos + next.utf16_len,
                    },
                });
                i += 2;
                continue;
            }
            if nc.ch == '>' && next.ch == '>' {
                tokens.push(Token {
                    kind: TokenKind::Op(OpKind::Shr),
                    span: Span {
                        start_utf16: nc.utf16_pos,
                        end_utf16: next.utf16_pos + next.utf16_len,
                    },
                });
                i += 2;
                continue;
            }
        }

        // Single-character operators
        let single_op = match nc.ch {
            '+' => Some(OpKind::Plus),
            '-' => Some(OpKind::Minus),
            '*' => Some(OpKind::Star),
            '/' => Some(OpKind::Slash),
            '%' => Some(OpKind::Percent),
            '~' => Some(OpKind::Tilde),
            '&' => Some(OpKind::Amp),
            '|' => Some(OpKind::Pipe),
            '^' => Some(OpKind::Caret),
            '(' => Some(OpKind::LParen),
            ')' => Some(OpKind::RParen),
            _ => None,
        };
        if let Some(op) = single_op {
            tokens.push(Token {
                kind: TokenKind::Op(op),
                span: Span {
                    start_utf16: nc.utf16_pos,
                    end_utf16: nc.utf16_pos + nc.utf16_len,
                },
            });
            i += 1;
            continue;
        }

        // Integer literals (0x, 0b, 0o, or decimal)
        if nc.ch.is_ascii_digit() {
            let start = nc.utf16_pos;
            if nc.ch == '0' && i + 1 < chars.len() {
                let p = chars[i + 1].ch;
                let radix = match p {
                    'x' | 'X' => Some(16u8),
                    'b' | 'B' => Some(2u8),
                    'o' | 'O' => Some(8u8),
                    _ => None,
                };
                if let Some(r) = radix {
                    if primary_radix.is_none() {
                        primary_radix = Some(r);
                    }
                    let (tok, next_i) = lex_prefixed_int(&chars, i, r, start);
                    tokens.push(tok);
                    i = next_i;
                    continue;
                }
            }

            let (tok, next_i) = lex_decimal_int(&chars, i, start);
            tokens.push(tok);
            i = next_i;
            continue;
        }

        // Identifiers / keywords (xor)
        if nc.ch.is_ascii_alphabetic() || nc.ch == '_' {
            let start = nc.utf16_pos;
            let mut end = nc.utf16_pos + nc.utf16_len;
            let mut ident = String::new();
            while i < chars.len() && (chars[i].ch.is_ascii_alphanumeric() || chars[i].ch == '_') {
                ident.push(chars[i].ch);
                end = chars[i].utf16_pos + chars[i].utf16_len;
                i += 1;
            }
            let kind = if ident.eq_ignore_ascii_case("xor") {
                TokenKind::Op(OpKind::Xor)
            } else {
                TokenKind::Ident(ident)
            };
            tokens.push(Token {
                kind,
                span: Span {
                    start_utf16: start,
                    end_utf16: end,
                },
            });
            continue;
        }

        tokens.push(Token {
            kind: TokenKind::UnknownChar(nc.ch),
            span: Span {
                start_utf16: nc.utf16_pos,
                end_utf16: nc.utf16_pos + nc.utf16_len,
            },
        });
        i += 1;
    }

    Ok((
        tokens,
        primary_radix.unwrap_or(10),
        normalized.trim().to_string(),
    ))
}

fn lex_prefixed_int(
    chars: &[NormChar],
    start_idx: usize,
    radix: u8,
    start_utf16: usize,
) -> (Token, usize) {
    let mut i = start_idx + 2;
    let mut end_utf16 = chars[start_idx + 1].utf16_pos + chars[start_idx + 1].utf16_len;
    let mut digits = String::new();
    let mut invalid = false;
    let mut prev_underscore = false;

    if i >= chars.len() {
        return (
            Token {
                kind: TokenKind::InvalidLiteral {
                    code: "incomplete_input",
                    radix,
                },
                span: Span {
                    start_utf16,
                    end_utf16,
                },
            },
            i,
        );
    }

    if chars[i].ch == '_' {
        invalid = true;
    }

    while i < chars.len() {
        let c = chars[i].ch;
        if c == '_' {
            if prev_underscore {
                invalid = true;
            }
            prev_underscore = true;
            end_utf16 = chars[i].utf16_pos + chars[i].utf16_len;
            i += 1;
        } else if c.is_ascii_alphanumeric() {
            if c.to_digit(radix as u32).is_none() {
                invalid = true;
            } else {
                digits.push(c);
            }
            prev_underscore = false;
            end_utf16 = chars[i].utf16_pos + chars[i].utf16_len;
            i += 1;
        } else {
            break;
        }
    }

    if digits.is_empty() {
        let code = if invalid {
            "invalid_syntax"
        } else {
            "incomplete_input"
        };
        return (
            Token {
                kind: TokenKind::InvalidLiteral { code, radix },
                span: Span {
                    start_utf16,
                    end_utf16,
                },
            },
            i,
        );
    }

    if prev_underscore || invalid {
        return (
            Token {
                kind: TokenKind::InvalidLiteral {
                    code: "invalid_syntax",
                    radix,
                },
                span: Span {
                    start_utf16,
                    end_utf16,
                },
            },
            i,
        );
    }

    let value = BigInt::parse_bytes(digits.as_bytes(), radix as u32).unwrap_or_else(BigInt::zero);
    (
        Token {
            kind: TokenKind::IntLiteral { value, radix },
            span: Span {
                start_utf16,
                end_utf16,
            },
        },
        i,
    )
}

fn lex_decimal_int(chars: &[NormChar], start_idx: usize, start_utf16: usize) -> (Token, usize) {
    let mut i = start_idx;
    let mut end_utf16 = start_utf16;
    let mut digits = String::new();
    let mut invalid = false;
    let mut prev_underscore = false;

    while i < chars.len() {
        let c = chars[i].ch;
        if c == '_' {
            if prev_underscore {
                invalid = true;
            }
            prev_underscore = true;
            end_utf16 = chars[i].utf16_pos + chars[i].utf16_len;
            i += 1;
        } else if c.is_ascii_digit() {
            digits.push(c);
            prev_underscore = false;
            end_utf16 = chars[i].utf16_pos + chars[i].utf16_len;
            i += 1;
        } else if c.is_ascii_alphabetic() {
            invalid = true;
            prev_underscore = false;
            end_utf16 = chars[i].utf16_pos + chars[i].utf16_len;
            i += 1;
        } else {
            break;
        }
    }

    if prev_underscore || invalid || digits.is_empty() {
        return (
            Token {
                kind: TokenKind::InvalidLiteral {
                    code: "invalid_syntax",
                    radix: 10,
                },
                span: Span {
                    start_utf16,
                    end_utf16,
                },
            },
            i,
        );
    }

    let value = BigInt::parse_bytes(digits.as_bytes(), 10).unwrap_or_else(BigInt::zero);
    (
        Token {
            kind: TokenKind::IntLiteral { value, radix: 10 },
            span: Span {
                start_utf16,
                end_utf16,
            },
        },
        i,
    )
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
    settings: ProgrammerSettings,
    query_utf16_len: usize,
    modulus: BigInt,
    mask: BigInt,
    min_val: BigInt,
    max_val: BigInt,
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [Token], settings: ProgrammerSettings, query_utf16_len: usize) -> Self {
        let modulus = BigInt::one() << settings.bit_width;
        let mask = &modulus - BigInt::one();
        let (min_val, max_val) = if settings.signed {
            let half = BigInt::one() << (settings.bit_width - 1);
            (-&half, half - BigInt::one())
        } else {
            (BigInt::zero(), &modulus - BigInt::one())
        };
        Self {
            tokens,
            pos: 0,
            settings,
            query_utf16_len,
            modulus,
            mask,
            min_val,
            max_val,
        }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<&Token> {
        let tok = self.tokens.get(self.pos);
        if tok.is_some() {
            self.pos += 1;
        }
        tok
    }

    fn parse_full(&mut self) -> Result<BigInt, Diagnostic> {
        let val = self.parse_expr(0, 0)?;
        if let Some(extra) = self.peek() {
            let mut diag = Diagnostic::error("invalid_syntax");
            diag.span = Some(extra.span.to_json());
            return Err(diag);
        }
        Ok(val)
    }

    fn parse_expr(&mut self, min_bp: u8, depth: usize) -> Result<BigInt, Diagnostic> {
        if depth > MAX_DEPTH {
            return Err(limit_diagnostic("nesting_depth", MAX_DEPTH));
        }

        let mut lhs = self.parse_prefix(depth + 1)?;

        while let Some(tok) = self.peek() {
            let TokenKind::Op(op) = tok.kind else {
                break;
            };
            let Some((l_bp, r_bp)) = infix_binding_power(op) else {
                break;
            };
            if l_bp < min_bp {
                break;
            }
            let op_span = tok.span;
            self.next();
            let rhs = self.parse_expr(r_bp, depth + 1)?;
            lhs = self.eval_binary(op, lhs, rhs, op_span)?;
        }

        Ok(lhs)
    }

    fn parse_prefix(&mut self, depth: usize) -> Result<BigInt, Diagnostic> {
        if depth > MAX_DEPTH {
            return Err(limit_diagnostic("nesting_depth", MAX_DEPTH));
        }

        let Some(tok) = self.next().cloned() else {
            let mut diag = Diagnostic::error("incomplete_input");
            diag.span = Some(
                Span {
                    start_utf16: self.query_utf16_len,
                    end_utf16: self.query_utf16_len,
                }
                .to_json(),
            );
            return Err(diag);
        };

        match tok.kind {
            TokenKind::IntLiteral { value, radix } => {
                self.normalize_literal(value, radix, tok.span)
            }
            TokenKind::InvalidLiteral { code, .. } => {
                let mut diag = Diagnostic::error(code);
                diag.span = Some(tok.span.to_json());
                Err(diag)
            }
            TokenKind::Op(OpKind::Plus) => self.parse_expr(70, depth + 1),
            TokenKind::Op(OpKind::Minus) => {
                // Allow negative decimal literals such as -128 in i8 without premature overflow.
                if let Some(next_tok) = self.peek().cloned()
                    && let TokenKind::IntLiteral { value, radix: 10 } = next_tok.kind
                {
                    self.next();
                    let combined_span = Span {
                        start_utf16: tok.span.start_utf16,
                        end_utf16: next_tok.span.end_utf16,
                    };
                    return self.fit_value(-value, combined_span);
                }
                let inner = self.parse_expr(70, depth + 1)?;
                self.fit_value(-inner, tok.span)
            }
            TokenKind::Op(OpKind::Tilde) => {
                let inner = self.parse_expr(70, depth + 1)?;
                let bits = self.to_bits(&inner) ^ &self.mask;
                Ok(self.bits_to_value(bits))
            }
            TokenKind::Op(OpKind::LParen) => {
                let inner = self.parse_expr(0, depth + 1)?;
                match self.next() {
                    Some(Token {
                        kind: TokenKind::Op(OpKind::RParen),
                        ..
                    }) => Ok(inner),
                    Some(other) => {
                        let mut diag = Diagnostic::error("invalid_syntax");
                        diag.span = Some(other.span.to_json());
                        Err(diag)
                    }
                    None => {
                        let mut diag = Diagnostic::error("incomplete_input");
                        diag.span = Some(tok.span.to_json());
                        Err(diag)
                    }
                }
            }
            TokenKind::Ident(ident) => {
                let mut diag = Diagnostic::error("unknown_identifier");
                diag.parameters = json!({ "identifier": ident });
                diag.span = Some(tok.span.to_json());
                Err(diag)
            }
            TokenKind::Op(_) | TokenKind::UnknownChar(_) => {
                let mut diag = Diagnostic::error("invalid_syntax");
                diag.span = Some(tok.span.to_json());
                Err(diag)
            }
        }
    }

    fn normalize_literal(
        &self,
        value: BigInt,
        radix: u8,
        span: Span,
    ) -> Result<BigInt, Diagnostic> {
        if radix == 10 {
            return self.fit_value(value, span);
        }

        // Prefixed radix literals (0x, 0b, 0o) represent bit-patterns of up to bit_width bits.
        if value >= self.modulus {
            match self.settings.overflow {
                OverflowPolicy::Error => return Err(self.overflow_diagnostic(&value, span)),
                OverflowPolicy::Wrap => {
                    let wrapped = value & &self.mask;
                    return Ok(self.bits_to_value(wrapped));
                }
            }
        }

        Ok(self.bits_to_value(value))
    }

    fn fit_value(&self, value: BigInt, span: Span) -> Result<BigInt, Diagnostic> {
        if value < self.min_val || value > self.max_val {
            match self.settings.overflow {
                OverflowPolicy::Error => return Err(self.overflow_diagnostic(&value, span)),
                OverflowPolicy::Wrap => {
                    let bits = self.to_bits(&value);
                    return Ok(self.bits_to_value(bits));
                }
            }
        }
        Ok(value)
    }

    fn to_bits(&self, value: &BigInt) -> BigInt {
        ((value % &self.modulus) + &self.modulus) % &self.modulus
    }

    fn bits_to_value(&self, bits: BigInt) -> BigInt {
        if self.settings.signed {
            let sign_bit = BigInt::one() << (self.settings.bit_width - 1);
            if bits >= sign_bit {
                return bits - &self.modulus;
            }
        }
        bits
    }

    fn eval_binary(
        &self,
        op: OpKind,
        lhs: BigInt,
        rhs: BigInt,
        span: Span,
    ) -> Result<BigInt, Diagnostic> {
        match op {
            OpKind::Plus => self.fit_value(lhs + rhs, span),
            OpKind::Minus => self.fit_value(lhs - rhs, span),
            OpKind::Star => self.fit_value(lhs * rhs, span),
            OpKind::Slash => {
                if rhs.is_zero() {
                    return Err(domain_diagnostic("division_by_zero", span));
                }
                self.fit_value(lhs / rhs, span)
            }
            OpKind::Percent => {
                if rhs.is_zero() {
                    return Err(domain_diagnostic("division_by_zero", span));
                }
                self.fit_value(lhs % rhs, span)
            }
            OpKind::Amp => {
                let bits = self.to_bits(&lhs) & self.to_bits(&rhs);
                Ok(self.bits_to_value(bits))
            }
            OpKind::Pipe => {
                let bits = self.to_bits(&lhs) | self.to_bits(&rhs);
                Ok(self.bits_to_value(bits))
            }
            OpKind::Caret | OpKind::Xor => {
                let bits = self.to_bits(&lhs) ^ self.to_bits(&rhs);
                Ok(self.bits_to_value(bits))
            }
            OpKind::Shl => self.eval_shl(lhs, rhs, span),
            OpKind::Shr => self.eval_shr(lhs, rhs, span),
            _ => {
                let mut diag = Diagnostic::error("invalid_syntax");
                diag.span = Some(span.to_json());
                Err(diag)
            }
        }
    }

    fn eval_shl(&self, lhs: BigInt, rhs: BigInt, span: Span) -> Result<BigInt, Diagnostic> {
        if rhs.is_negative() {
            return Err(domain_diagnostic("negative_shift_count", span));
        }
        let Some(shift) = rhs.to_u32() else {
            return Err(self.shift_range_diagnostic(&rhs, span));
        };
        if shift >= self.settings.bit_width as u32 {
            return match self.settings.overflow {
                OverflowPolicy::Error => Err(self.shift_range_diagnostic(&rhs, span)),
                OverflowPolicy::Wrap => Ok(BigInt::zero()),
            };
        }
        let shifted = lhs << shift;
        self.fit_value(shifted, span)
    }

    fn eval_shr(&self, lhs: BigInt, rhs: BigInt, span: Span) -> Result<BigInt, Diagnostic> {
        if rhs.is_negative() {
            return Err(domain_diagnostic("negative_shift_count", span));
        }
        let Some(shift) = rhs.to_u32() else {
            return Err(self.shift_range_diagnostic(&rhs, span));
        };
        if shift >= self.settings.bit_width as u32 {
            return match self.settings.overflow {
                OverflowPolicy::Error => Err(self.shift_range_diagnostic(&rhs, span)),
                OverflowPolicy::Wrap => {
                    if self.settings.signed && lhs.is_negative() {
                        Ok(BigInt::from(-1))
                    } else {
                        Ok(BigInt::zero())
                    }
                }
            };
        }
        if self.settings.signed {
            Ok(lhs >> shift)
        } else {
            let bits = self.to_bits(&lhs);
            Ok(bits >> shift)
        }
    }

    fn overflow_diagnostic(&self, value: &BigInt, span: Span) -> Diagnostic {
        let mut diag = Diagnostic::error("numeric_overflow");
        diag.parameters = json!({
            "bitWidth": self.settings.bit_width,
            "signed": self.settings.signed,
            "value": value.to_string(),
            "min": self.min_val.to_string(),
            "max": self.max_val.to_string(),
        });
        diag.span = Some(span.to_json());
        diag
    }

    fn shift_range_diagnostic(&self, shift: &BigInt, span: Span) -> Diagnostic {
        let mut diag = Diagnostic::error("domain_error");
        diag.parameters = json!({
            "reason": "shift_count_exceeds_width",
            "shiftCount": shift.to_string(),
            "bitWidth": self.settings.bit_width,
        });
        diag.span = Some(span.to_json());
        diag
    }
}

fn infix_binding_power(op: OpKind) -> Option<(u8, u8)> {
    match op {
        OpKind::Pipe => Some((10, 11)),
        OpKind::Caret | OpKind::Xor => Some((20, 21)),
        OpKind::Amp => Some((30, 31)),
        OpKind::Shl | OpKind::Shr => Some((40, 41)),
        OpKind::Plus | OpKind::Minus => Some((50, 51)),
        OpKind::Star | OpKind::Slash | OpKind::Percent => Some((60, 61)),
        _ => None,
    }
}

fn domain_diagnostic(reason: &str, span: Span) -> Diagnostic {
    let mut diag = Diagnostic::error("domain_error");
    diag.parameters = json!({ "reason": reason });
    diag.span = Some(span.to_json());
    diag
}

fn limit_diagnostic(limit: &str, max: usize) -> Diagnostic {
    let mut diag = Diagnostic::error("resource_limit");
    diag.parameters = json!({
        "limit": limit,
        "max": max,
    });
    diag
}

fn build_candidate(
    normalized_query: &str,
    value: &BigInt,
    primary_radix: u8,
    settings: ProgrammerSettings,
) -> Candidate {
    let modulus = BigInt::one() << settings.bit_width;
    let bits = ((value % &modulus) + &modulus) % &modulus;

    let hex_digits = bits.to_str_radix(16).to_ascii_uppercase();
    let hex_str = if hex_digits.len() < 2 {
        format!("0x{:0>2}", hex_digits)
    } else {
        format!("0x{hex_digits}")
    };

    let dec_copy = value.to_string();
    let dec_label = format_decimal_grouped(&dec_copy);
    let oct_str = format!("0o{}", bits.to_str_radix(8));
    let bin_str = format_binary_grouped(&bits);

    let hero = match primary_radix {
        16 => &hex_str,
        8 => &oct_str,
        2 => &bin_str,
        _ => &dec_label,
    };

    let type_label = format!(
        "{}{}",
        if settings.signed { "i" } else { "u" },
        settings.bit_width
    );
    let shift_label = if settings.signed {
        "arithmetic right shift"
    } else {
        "logical right shift"
    };
    let overflow_label = match settings.overflow {
        OverflowPolicy::Error => "overflow: error",
        OverflowPolicy::Wrap => "overflow: wrap",
    };

    let interpretation = format!("{hero} ({type_label}, {overflow_label})");
    let tagged_value = json!({
        "tag": "integer",
        "decimal": dec_copy,
        "signed": settings.signed,
        "bitWidth": settings.bit_width,
        "radix": primary_radix,
    });

    let mut candidate = Candidate::new(
        "programmer-1",
        "programmer",
        normalized_query,
        &interpretation,
        tagged_value,
    );
    candidate.assumptions = vec![
        type_label,
        shift_label.to_string(),
        overflow_label.to_string(),
    ];
    candidate.formats = vec![
        Format::new("hex", &hex_str, &hex_str),
        Format::new("decimal", &dec_label, &dec_copy),
        Format::new("octal", &oct_str, &oct_str),
        Format::new("binary", &bin_str, &bin_str),
    ];
    candidate.provenance = Some(json!({
        "kind": "programmer_integer_engine",
        "bitWidth": settings.bit_width,
        "signed": settings.signed,
        "overflow": settings.overflow,
    }));
    candidate
}

fn format_decimal_grouped(dec: &str) -> String {
    let (sign, digits) = if let Some(rest) = dec.strip_prefix('-') {
        ("-", rest)
    } else {
        ("", dec)
    };
    if digits.len() <= 3 {
        return dec.to_string();
    }
    let mut out = String::with_capacity(dec.len() + digits.len() / 3);
    out.push_str(sign);
    let first_group = digits.len() % 3;
    let mut idx = 0;
    if first_group != 0 {
        out.push_str(&digits[..first_group]);
        idx = first_group;
    }
    while idx < digits.len() {
        if idx > 0 {
            out.push(',');
        }
        out.push_str(&digits[idx..idx + 3]);
        idx += 3;
    }
    out
}

fn format_binary_grouped(bits: &BigInt) -> String {
    let raw = bits.to_str_radix(2);
    let rem = raw.len() % 4;
    let pad = if rem == 0 { 0 } else { 4 - rem };
    let padded = format!("{}{}", "0".repeat(pad), raw);
    let mut out = String::with_capacity(2 + padded.len() + padded.len() / 4);
    out.push_str("0b");
    for (i, ch) in padded.chars().enumerate() {
        if i > 0 && i % 4 == 0 {
            out.push('_');
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval_ok(query: &str, settings: ProgrammerSettings) -> Candidate {
        let res = preview(query, settings).expect("expected programmer route");
        let mut candidates = res.expect("expected ok result");
        assert_eq!(candidates.len(), 1);
        candidates.remove(0)
    }

    fn eval_err(query: &str, settings: ProgrammerSettings) -> Diagnostic {
        let res = preview(query, settings).expect("expected programmer route");
        res.expect_err("expected diagnostic error")
    }

    #[test]
    fn p01_figma_deadbeef_u32() {
        let cand = eval_ok("0xDEAD_BEEF >> 4 & 0xFFFF", ProgrammerSettings::default());
        assert_eq!(cand.kind, "programmer");
        assert_eq!(cand.value["tag"], "integer");
        assert_eq!(cand.value["decimal"], "56302");
        assert_eq!(cand.value["bitWidth"], 32);
        assert_eq!(cand.value["signed"], false);
        assert_eq!(cand.value["radix"], 16);
        assert_eq!(cand.formats[0].copy_text, "0xDBEE");
    }

    #[test]
    fn p02_all_four_formats() {
        let cand = eval_ok("0xDEAD_BEEF >> 4 & 0xFFFF", ProgrammerSettings::default());
        assert_eq!(cand.formats[0].id, "hex");
        assert_eq!(cand.formats[0].copy_text, "0xDBEE");
        assert_eq!(cand.formats[1].id, "decimal");
        assert_eq!(cand.formats[1].label, "56,302");
        assert_eq!(cand.formats[1].copy_text, "56302");
        assert_eq!(cand.formats[2].id, "octal");
        assert_eq!(cand.formats[2].copy_text, "0o155756");
        assert_eq!(cand.formats[3].id, "binary");
        assert_eq!(cand.formats[3].copy_text, "0b1101_1011_1110_1110");
    }

    #[test]
    fn p03_u8_wrap_overflow() {
        let settings = ProgrammerSettings {
            bit_width: 8,
            signed: false,
            overflow: OverflowPolicy::Wrap,
        };
        let cand = eval_ok("0xff+1", settings);
        assert_eq!(cand.value["decimal"], "0");
        assert_eq!(cand.formats[0].copy_text, "0x00");
    }

    #[test]
    fn p04_u8_error_overflow() {
        let settings = ProgrammerSettings {
            bit_width: 8,
            signed: false,
            overflow: OverflowPolicy::Error,
        };
        let diag = eval_err("0xff+1", settings);
        assert_eq!(diag.code, "numeric_overflow");
    }

    #[test]
    fn p05_0xff_as_i8_twos_complement() {
        let settings = ProgrammerSettings {
            bit_width: 8,
            signed: true,
            overflow: OverflowPolicy::Error,
        };
        let cand = eval_ok("0xff", settings);
        assert_eq!(cand.value["decimal"], "-1");
        assert_eq!(cand.formats[0].copy_text, "0xFF");
        assert_eq!(cand.formats[1].copy_text, "-1");
        assert_eq!(cand.formats[3].copy_text, "0b1111_1111");
    }

    #[test]
    fn p06_negative_shift_count() {
        let i32_settings = ProgrammerSettings {
            bit_width: 32,
            signed: true,
            overflow: OverflowPolicy::Error,
        };
        let diag = eval_err("1 << -1", i32_settings);
        assert_eq!(diag.code, "domain_error");
        assert_eq!(diag.parameters["reason"], "negative_shift_count");

        let diag_shr = eval_err("0x10 >> -2", i32_settings);
        assert_eq!(diag_shr.code, "domain_error");
        assert_eq!(diag_shr.parameters["reason"], "negative_shift_count");

        // In unsigned mode, -1 is also rejected with an explicit diagnostic.
        let diag_u32 = eval_err("1 << -1", ProgrammerSettings::default());
        assert_eq!(diag_u32.code, "numeric_overflow");
    }

    #[test]
    fn arithmetic_vs_logical_right_shift_and_u8_negative() {
        let i8_settings = ProgrammerSettings {
            bit_width: 8,
            signed: true,
            overflow: OverflowPolicy::Error,
        };
        let cand = eval_ok("-1 >> 1", i8_settings);
        assert_eq!(cand.value["decimal"], "-1");

        let u8_settings = ProgrammerSettings {
            bit_width: 8,
            signed: false,
            overflow: OverflowPolicy::Error,
        };
        let diag = eval_err("-1 >> 1", u8_settings);
        assert_eq!(diag.code, "numeric_overflow");
    }

    #[test]
    fn i8_min_literal_and_u128_exact() {
        let i8_settings = ProgrammerSettings {
            bit_width: 8,
            signed: true,
            overflow: OverflowPolicy::Error,
        };
        assert!(preview("-128", i8_settings).is_none());
        let cand = eval_ok("-128 >> 0", i8_settings);
        assert_eq!(cand.value["decimal"], "-128");

        let u128_settings = ProgrammerSettings {
            bit_width: 128,
            signed: false,
            overflow: OverflowPolicy::Error,
        };
        let cand128 = eval_ok("0xFFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF", u128_settings);
        assert_eq!(
            cand128.value["decimal"],
            "340282366920938463463374607431768211455"
        );
    }

    #[test]
    fn false_positive_routing_returns_none() {
        let s = ProgrammerSettings::default();
        assert!(preview("", s).is_none());
        assert!(preview("1 + 1", s).is_none());
        assert!(preview("2 ^ 10", s).is_none());
        assert!(preview("wallpaper-1920x1080.png", s).is_none());
        assert!(preview("https://example.com/?a=1&b=2", s).is_none());
        assert!(preview("/usr/bin/foo & bar", s).is_none());
        assert!(preview("> echo 0xff", s).is_none());
        assert!(preview("$ cat 0xff", s).is_none());
        assert!(preview("cats & dogs", s).is_none());
        assert!(preview("0.1 as bf16", s).is_none());
        assert!(preview("0x3DCD as fp16", s).is_none());
        assert!(preview("100 Mbps to MB/s", s).is_none());
    }

    #[test]
    fn settings_serde_defaults_and_deny_unknown() {
        let default_s: ProgrammerSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(default_s, ProgrammerSettings::default());
        assert_eq!(default_s.bit_width, 32);
        assert!(!default_s.signed);
        assert_eq!(default_s.overflow, OverflowPolicy::Error);

        let custom: ProgrammerSettings =
            serde_json::from_str(r#"{"bitWidth":16,"signed":true,"overflow":"wrap"}"#).unwrap();
        assert_eq!(custom.bit_width, 16);
        assert!(custom.signed);
        assert_eq!(custom.overflow, OverflowPolicy::Wrap);

        assert!(serde_json::from_str::<ProgrammerSettings>(r#"{"unknown":1}"#).is_err());
    }

    #[test]
    fn operators_and_incomplete_syntax() {
        let s = ProgrammerSettings::default();
        assert_eq!(eval_ok("0xAA xor 0x55", s).value["decimal"], "255");
        assert_eq!(eval_ok("0xAA ^ 0x55", s).value["decimal"], "255");
        assert_eq!(
            eval_ok("~0", ProgrammerSettings { bit_width: 8, ..s }).value["decimal"],
            "255"
        );
        assert_eq!(eval_err("0x", s).code, "incomplete_input");
        assert_eq!(eval_err("0x10 >> ", s).code, "incomplete_input");
        assert_eq!(eval_err("0x10 / 0", s).code, "domain_error");
    }

    #[test]
    fn precedence_unicode_widths_and_limits() {
        let s = ProgrammerSettings::default();
        assert_eq!(eval_ok("1 << 2 + 1", s).value["decimal"], "8");
        assert_eq!(eval_ok("0x01 | 0x02 ^ 0x03", s).value["decimal"], "1");
        assert_eq!(eval_ok("(0x01 | 0x02) ^ 0x03", s).value["decimal"], "0");
        assert_eq!(eval_ok("0x0F & 0x03 ^ 0x01", s).value["decimal"], "2");
        assert_eq!(eval_ok("0x10 \u{2212} 1", s).value["decimal"], "15");
        assert_eq!(eval_ok("0x10 \u{00D7} 2", s).value["decimal"], "32");
        assert_eq!(eval_ok("0x10 \u{00F7} 2", s).value["decimal"], "8");

        let bad_width = ProgrammerSettings { bit_width: 24, ..s };
        assert_eq!(eval_err("0x10", bad_width).code, "unsupported_format");

        let deep = format!("{}0x1{}", "(".repeat(70), ")".repeat(70));
        assert_eq!(eval_err(&deep, s).code, "resource_limit");

        let too_many = format!("0x1{}", " + 0x1".repeat(70));
        assert_eq!(eval_err(&too_many, s).code, "resource_limit");

        let unknown = eval_err("0x10 + foo", s);
        assert_eq!(unknown.code, "unknown_identifier");
        assert_eq!(
            unknown.span,
            Some(json!({ "startUtf16": 7, "endUtf16": 10 }))
        );
    }
}
