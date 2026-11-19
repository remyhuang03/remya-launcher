use molecular_formulas::ChargedMolecularFormula;
use molecular_formulas::prelude::ChemicalFormula;

use super::*;

macro_rules! metric {
    ($id:literal, $symbol:literal, $name:literal, $dim:ident, $power:literal, $aliases:literal) => {
        u!(
            $id,
            $symbol,
            $name,
            Dim::$dim,
            1,
            10i128.pow($power),
            Si,
            None,
            $aliases
        )
    };
}

pub(super) static UNITS: &[UnitDef] = &[
    metric!("amount_mole", "mol", "moles", AMOUNT, 0, "mol mole moles"),
    metric!(
        "amount_millimole",
        "mmol",
        "millimoles",
        AMOUNT,
        3,
        "mmol millimole millimoles"
    ),
    metric!(
        "amount_micromole",
        "µmol",
        "micromoles",
        AMOUNT,
        6,
        "µmol umol micromole micromoles"
    ),
    metric!(
        "amount_nanomole",
        "nmol",
        "nanomoles",
        AMOUNT,
        9,
        "nmol nanomole nanomoles"
    ),
    metric!(
        "amount_picomole",
        "pmol",
        "picomoles",
        AMOUNT,
        12,
        "pmol picomole picomoles"
    ),
    metric!(
        "mass_nanogram",
        "ng",
        "nanograms",
        MASS,
        12,
        "ng nanogram nanograms"
    ),
    metric!(
        "mass_picogram",
        "pg",
        "picograms",
        MASS,
        15,
        "pg picogram picograms"
    ),
    metric!(
        "volume_decilitre",
        "dL",
        "decilitres",
        VOL,
        4,
        "dL dl decilitre decilitres deciliter deciliters"
    ),
    metric!(
        "volume_microlitre",
        "µL",
        "microlitres",
        VOL,
        9,
        "µL µl uL ul microlitre microlitres microliter microliters"
    ),
    metric!(
        "volume_nanolitre",
        "nL",
        "nanolitres",
        VOL,
        12,
        "nL nl nanolitre nanolitres nanoliter nanoliters"
    ),
    metric!(
        "volume_picolitre",
        "pL",
        "picolitres",
        VOL,
        15,
        "pL pl picolitre picolitres picoliter picoliters"
    ),
];

const GRAMS_PER_MOLE: UnitDef = metric!(
    "mass_gram_per_mole",
    "g/mol",
    "grams per mole",
    MOLAR_MASS,
    3,
    "g/mol"
);

pub(super) fn command(query: &str) -> Option<&str> {
    let rest = query.trim_start().strip_prefix("molar mass")?;
    (rest.is_empty() || rest.starts_with(char::is_whitespace)).then(|| rest.trim())
}

pub(super) fn split_using(query: &str) -> (&str, Option<&str>) {
    for (start, marker) in query.match_indices("using") {
        let (before, after) = (&query[..start], &query[start + marker.len()..]);
        if before.ends_with(char::is_whitespace)
            && (after.is_empty() || after.starts_with(char::is_whitespace))
            && looks_unit_related_quick(before)
        {
            return (before.trim_end(), Some(after.trim()));
        }
    }
    (query, None)
}

fn diagnostic(code: &str, message: &str) -> Diagnostic {
    let mut diagnostic = Diagnostic::error(code);
    diagnostic.parameters = json!({ "message": message });
    diagnostic
}

pub(super) fn missing_mass(end: usize) -> Diagnostic {
    let mut diagnostic = diagnostic(
        "molar_mass_required",
        "This conversion needs a molar mass. Add using <formula> or using <number> g/mol.",
    );
    diagnostic.suggestions = vec![json!({
        "id": "using",
        "label": "Add a chemical formula or molar mass",
        "replacement": { "startUtf16": end, "endUtf16": end, "text": " using " }
    })];
    diagnostic
}

fn formula_mass(formula: &str) -> Result<BigRational, Diagnostic> {
    let invalid = || {
        diagnostic(
            "invalid_chemical_formula",
            "Enter a chemical formula such as NaCl, Ca(OH)2, or CuSO4·5H2O. Element symbols are case-sensitive.",
        )
    };
    if formula.len() > 256
        || !formula.starts_with(|c: char| c.is_ascii_uppercase() || c == '(')
        || !formula
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '(' | ')' | '·' | '₀'..='₉'))
    {
        return Err(invalid());
    }
    let component_mass = |component: &str| {
        let parsed: ChemicalFormula = component.parse().map_err(|_| invalid())?;
        let mass = parsed.molar_mass();
        if !mass.is_finite() || mass <= 0.0 {
            return Err(invalid());
        }
        Ok(mass)
    };
    // A hydrate coefficient applies to the component after the dot.
    let mut components = formula.split('·');
    let mut mass = component_mass(components.next().unwrap_or_default())?;
    for component in components {
        let compound = component.trim_start_matches(|c: char| c.is_ascii_digit());
        let coefficient = &component[..component.len() - compound.len()];
        let count = if coefficient.is_empty() {
            1
        } else {
            coefficient
                .parse::<u32>()
                .ok()
                .filter(|&count| count > 0 && !coefficient.starts_with('0'))
                .ok_or_else(invalid)?
        };
        mass += f64::from(count) * component_mass(compound)?;
    }
    if !mass.is_finite() || mass <= 0.0 {
        return Err(invalid());
    }
    parse_exact_number(&mass.to_string())
}

fn explicit_mass(specification: &str) -> Result<(BigRational, bool), Diagnostic> {
    if specification.is_empty() {
        return Err(diagnostic(
            "molar_mass_missing_value",
            "After using, enter a chemical formula or a positive molar mass in g/mol.",
        ));
    }
    if specification.starts_with(|c: char| c.is_ascii_digit() || matches!(c, '-' | '+' | '.'))
        || specification.contains('/')
    {
        let invalid = || {
            diagnostic(
                "invalid_molar_mass",
                "Enter a positive molar mass in g/mol, for example 272.38 g/mol.",
            )
        };
        let (mass, amount) = specification.split_once('/').ok_or_else(invalid)?;
        let scalar = mass
            .trim_end()
            .strip_suffix('g')
            .ok_or_else(invalid)?
            .trim();
        if amount.trim() != "mol" {
            return Err(invalid());
        }
        let tokens = raw_tokenize(scalar).map_err(|_| invalid())?;
        let number = match tokens.as_slice() {
            [
                Token {
                    kind: TokKind::Number(number),
                    ..
                },
            ]
            | [
                Token {
                    kind: TokKind::Plus,
                    ..
                },
                Token {
                    kind: TokKind::Number(number),
                    ..
                },
            ] => number,
            _ => return Err(invalid()),
        };
        let grams = parse_exact_number(number).map_err(|_| invalid())?;
        if !grams.is_positive() {
            return Err(invalid());
        }
        return Ok((grams, true));
    }
    formula_mass(specification).map(|grams| (grams, false))
}

pub(super) fn is_conversion(source: Dim, target: Dim) -> bool {
    matches!(
        (source, target),
        (Dim::MASS, Dim::AMOUNT)
            | (Dim::AMOUNT, Dim::MASS)
            | (Dim::MASS_CONCENTRATION, Dim::MOLAR_CONCENTRATION)
            | (Dim::MOLAR_CONCENTRATION, Dim::MASS_CONCENTRATION)
    )
}

fn approximate(candidate: &mut Candidate) {
    let format = &mut candidate.formats[0];
    format.label = format!("approx. {}", format.label);
    candidate.value["display"] = json!(format.label);
}

pub(super) fn molar_mass_preview(formula: &str) -> Result<Candidate, Diagnostic> {
    let grams = formula_mass(formula)?;
    let unit = ResolvedUnit::from_def(&GRAMS_PER_MOLE);
    let formatted = format_rational_decimal(&grams, false);
    let display = format!("{} g/mol", formatted.display);
    let expression = format!("molar mass {formula}");
    let mut candidate = Candidate::new(
        "chemistry:molar_mass",
        "molar_mass",
        &expression,
        &format!("Molar mass of {formula}"),
        build_quantity_value_json(&formatted.full, &grams, &unit, &display, false),
    );
    candidate.formats = vec![Format::new("si", &display, &display)];
    candidate.provenance = Some(json!({"kind":"molecular_formula","engine":"molecular-formulas"}));
    approximate(&mut candidate);
    Ok(candidate)
}

pub(super) fn convert_explicit(
    lhs: &[Token],
    rhs: &[Token],
    specification: &str,
    end: usize,
) -> Result<Vec<Candidate>, Diagnostic> {
    let parse = |tokens, unit_only| {
        parse_tokens(tokens, None, end, unit_only).map_err(|error| match error {
            EvalErr::Diagnostic(error) => error,
            EvalErr::UnknownUnit {
                token,
                start_utf16,
                end_utf16,
            } => unknown_unit_diag(&token, start_utf16, end_utf16, &[]),
        })
    };
    let source = parse(lhs, false)?;
    let target = parse(rhs, true)?.unit;
    if !is_conversion(source.unit.dimension, target.dimension) {
        return Err(diagnostic(
            "incompatible_molar_mass_conversion",
            "Use molar mass to convert mass to amount (or vice versa), including concentrations per volume.",
        ));
    }
    let (grams, exact_mass) = explicit_mass(specification)?;
    let mass = &grams * GRAMS_PER_MOLE.scale_rational();
    let mut bridged = source.clone();
    bridged.unit.dimension = target.dimension;
    if matches!(
        source.unit.dimension,
        Dim::AMOUNT | Dim::MOLAR_CONCENTRATION
    ) {
        bridged.value *= mass;
    } else {
        bridged.value /= mass;
    }
    let (converted, exact_units) = convert_quantity(&bridged, &target)?;
    let exact = exact_mass && exact_units;
    if converted.numer().bits().max(converted.denom().bits()) > MAX_BIGINT_BITS {
        return Err(limit_diag(
            "numeric_overflow",
            "result_exceeds_bit_limit",
            None,
        ));
    }
    let mut candidate = build_conversion_candidate(
        format!("chemistry:conversion:{}:{}", source.unit.id, target.id),
        &source,
        &target,
        &converted,
        exact,
        Vec::new(),
    );
    candidate.kind = "chemistry_conversion".into();
    candidate
        .normalized
        .push_str(&format!(" using {specification}"));
    candidate.formats.truncate(1);
    if exact_mass {
        candidate
            .assumptions
            .push(format!("Molar mass: {specification}"));
    } else {
        let grams = format_rational_decimal(&grams, false).display;
        candidate
            .assumptions
            .push(format!("{specification} · {grams} g/mol"));
        candidate.provenance =
            Some(json!({"kind":"molecular_formula","engine":"molecular-formulas"}));
    }
    if !exact {
        approximate(&mut candidate);
    }
    Ok(vec![candidate])
}
