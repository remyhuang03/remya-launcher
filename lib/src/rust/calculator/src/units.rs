use std::collections::{BTreeMap, HashSet};

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{One, Signed, Zero};
use serde_json::{Value, json};

use crate::protocol::{Candidate, Diagnostic, Format};

const MAX_QUERY_BYTES: usize = 4096;
const MAX_TOKENS: usize = 128;
const MAX_PAREN_DEPTH: usize = 32;
const MAX_UNIT_EXPONENT: i32 = 12;
const MAX_VALUE_EXPONENT: i32 = 1024;
const MAX_SCI_EXPONENT: i32 = 1000;
const MAX_INTEGER_DIGITS: usize = 200;
const MAX_BIGINT_BITS: u64 = 8192;
const MAX_CANDIDATES: usize = 16;
const MAX_SUGGESTIONS: usize = 8;

/// Evaluate a unit conversion or unit arithmetic preview.
///
/// `query` is the normalized shadow string; `original` is the raw user input
/// used to compute exact UTF-16 spans for correction patches and diagnostics.
pub(crate) fn preview(query: &str, original: &str) -> Option<Result<Vec<Candidate>, Diagnostic>> {
    if query.trim().is_empty() {
        return None;
    }
    if query.len() > MAX_QUERY_BYTES || original.len() > MAX_QUERY_BYTES {
        return looks_unit_related_quick(query)
            .then(|| Err(limit_diag("resource_limit", "query_too_long", None)));
    }
    if query
        .chars()
        .any(|c| c == '\0' || (c.is_control() && !c.is_whitespace()))
    {
        return None;
    }
    if is_non_unit_domain_or_prose(query.trim()) {
        return None;
    }

    let orig = if original.is_empty() { query } else { original };
    let tokens = match tokenize_with_spans(query, orig) {
        Ok(t) => t,
        Err(d) => return looks_unit_related_quick(query).then_some(Err(d)),
    };
    if tokens.is_empty() || has_non_calc_markers(&tokens) {
        return None;
    }
    if tokens.len() > MAX_TOKENS {
        return looks_unit_related_quick(query)
            .then(|| Err(limit_diag("resource_limit", "too_many_tokens", None)));
    }

    if let Some(op_idx) = find_conversion_split(&tokens) {
        evaluate_conversion(&tokens, op_idx, orig)
    } else {
        evaluate_unit_expression(&tokens, orig)
    }
}

// ---------------------------------------------------------------------------
// Dimension model (SI base exponents + data + angle)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Dim([i8; 9]); // [L, M, T, I, Theta, N, J, D, A]

impl Dim {
    const ZERO: Self = Self([0; 9]);
    const LEN: Self = Self([1, 0, 0, 0, 0, 0, 0, 0, 0]);
    const MASS: Self = Self([0, 1, 0, 0, 0, 0, 0, 0, 0]);
    const TIME: Self = Self([0, 0, 1, 0, 0, 0, 0, 0, 0]);
    const CURR: Self = Self([0, 0, 0, 1, 0, 0, 0, 0, 0]);
    const TEMP: Self = Self([0, 0, 0, 0, 1, 0, 0, 0, 0]);
    const DATA: Self = Self([0, 0, 0, 0, 0, 0, 0, 1, 0]);
    const ANG: Self = Self([0, 0, 0, 0, 0, 0, 0, 0, 1]);
    const AREA: Self = Self([2, 0, 0, 0, 0, 0, 0, 0, 0]);
    const VOL: Self = Self([3, 0, 0, 0, 0, 0, 0, 0, 0]);
    const SPEED: Self = Self([1, 0, -1, 0, 0, 0, 0, 0, 0]);
    const ACCEL: Self = Self([1, 0, -2, 0, 0, 0, 0, 0, 0]);
    const FORCE: Self = Self([1, 1, -2, 0, 0, 0, 0, 0, 0]);
    const PRESS: Self = Self([-1, 1, -2, 0, 0, 0, 0, 0, 0]);
    const ENERGY: Self = Self([2, 1, -2, 0, 0, 0, 0, 0, 0]);
    const POWER: Self = Self([2, 1, -3, 0, 0, 0, 0, 0, 0]);
    const FREQ: Self = Self([0, 0, -1, 0, 0, 0, 0, 0, 0]);
    const RATE: Self = Self([0, 0, -1, 0, 0, 0, 0, 1, 0]);
    const VOLT: Self = Self([2, 1, -3, -1, 0, 0, 0, 0, 0]);
    const RES: Self = Self([2, 1, -3, -2, 0, 0, 0, 0, 0]);
    const CHARGE: Self = Self([0, 0, 1, 1, 0, 0, 0, 0, 0]);

    fn combine(self, rhs: Self, sign: i8) -> Option<Self> {
        let mut out = [0i8; 9];
        for i in 0..9 {
            let v = if sign > 0 {
                self.0[i].checked_add(rhs.0[i])?
            } else {
                self.0[i].checked_sub(rhs.0[i])?
            };
            out[i] = v;
        }
        Some(Self(out))
    }

    fn pow(self, exp: i8) -> Option<Self> {
        let mut out = [0i8; 9];
        for i in 0..9 {
            out[i] = self.0[i].checked_mul(exp)?;
        }
        Some(Self(out))
    }

    fn id(self) -> String {
        const KNOWN: &[(Dim, &str)] = &[
            (Dim::ZERO, "dimensionless"),
            (Dim::LEN, "length"),
            (Dim::MASS, "mass"),
            (Dim::TIME, "time"),
            (Dim::CURR, "electric_current"),
            (Dim::TEMP, "temperature"),
            (Dim::DATA, "data"),
            (Dim::ANG, "angle"),
            (Dim::AREA, "area"),
            (Dim::VOL, "volume"),
            (Dim::SPEED, "speed"),
            (Dim::ACCEL, "acceleration"),
            (Dim::FORCE, "force"),
            (Dim::PRESS, "pressure"),
            (Dim::ENERGY, "energy"),
            (Dim::POWER, "power"),
            (Dim::FREQ, "frequency"),
            (Dim::RATE, "data_rate"),
            (Dim::VOLT, "voltage"),
            (Dim::RES, "resistance"),
            (Dim::CHARGE, "electric_charge"),
        ];
        if let Some((_, name)) = KNOWN.iter().find(|(d, _)| *d == self) {
            return (*name).to_string();
        }
        const AXES: [&str; 9] = ["L", "M", "T", "I", "Theta", "N", "J", "D", "A"];
        AXES.iter()
            .zip(self.0)
            .filter(|(_, e)| *e != 0)
            .map(|(n, e)| {
                if e == 1 {
                    (*n).to_string()
                } else {
                    format!("{n}^{e}")
                }
            })
            .collect::<Vec<_>>()
            .join("*")
    }

    fn to_json(self) -> Value {
        let [l, m, t, i, th, n, j, d, a] = self.0;
        json!({
            "id": self.id(),
            "exponents": {
                "length": l, "mass": m, "time": t, "current": i,
                "temperature": th, "amount": n, "luminousIntensity": j,
                "data": d, "angle": a,
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Unit registry, compound metadata, and ambiguity table
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnitSystem {
    Si,
    Iec,
    Imperial,
    Other,
}

#[derive(Debug, Clone, Copy)]
struct UnitDef {
    id: &'static str,
    symbol: &'static str,
    description: &'static str,
    dimension: Dim,
    scale_num: i128,
    scale_den: i128,
    offset_num: i128,
    offset_den: i128,
    is_affine: bool,
    pi_power: i8,
    system: UnitSystem,
    counterpart: Option<&'static str>,
    aliases: &'static str,
}

impl UnitDef {
    fn scale_rational(&self) -> BigRational {
        BigRational::new(BigInt::from(self.scale_num), BigInt::from(self.scale_den))
    }
    fn offset_rational(&self) -> BigRational {
        BigRational::new(BigInt::from(self.offset_num), BigInt::from(self.offset_den))
    }
}

macro_rules! u {
    ($id:expr, $sym:expr, $desc:expr, $dim:expr, $num:expr, $den:expr, $sys:expr, $ctr:expr, $al:expr) => {
        UnitDef {
            id: $id,
            symbol: $sym,
            description: $desc,
            dimension: $dim,
            scale_num: $num,
            scale_den: $den,
            offset_num: 0,
            offset_den: 1,
            is_affine: false,
            pi_power: 0,
            system: $sys,
            counterpart: $ctr,
            aliases: $al,
        }
    };
}

use UnitSystem::{Iec, Imperial as Imp, Other as Oth, Si};

static UNITS: &[UnitDef] = &[
    // Data (base: 1 bit)
    u!(
        "data_bit",
        "b",
        "bits",
        Dim::DATA,
        1,
        1,
        Si,
        None,
        "b bit bits"
    ),
    u!(
        "data_byte",
        "B",
        "bytes",
        Dim::DATA,
        8,
        1,
        Si,
        None,
        "B byte bytes"
    ),
    u!(
        "data_kilobit_si",
        "kb",
        "kilobits",
        Dim::DATA,
        1_000,
        1,
        Si,
        Some("Kib"),
        "kb kbit kilobit kilobits"
    ),
    u!(
        "data_megabit_si",
        "Mb",
        "megabits",
        Dim::DATA,
        1_000_000,
        1,
        Si,
        Some("Mib"),
        "Mb Mbit megabit megabits"
    ),
    u!(
        "data_gigabit_si",
        "Gb",
        "gigabits",
        Dim::DATA,
        1_000_000_000,
        1,
        Si,
        Some("Gib"),
        "Gb Gbit gigabit gigabits"
    ),
    u!(
        "data_terabit_si",
        "Tb",
        "terabits",
        Dim::DATA,
        1_000_000_000_000,
        1,
        Si,
        Some("Tib"),
        "Tb Tbit terabit terabits"
    ),
    u!(
        "data_petabit_si",
        "Pb",
        "petabits",
        Dim::DATA,
        1_000_000_000_000_000,
        1,
        Si,
        Some("Pib"),
        "Pb Pbit petabit petabits"
    ),
    u!(
        "data_kibibit_iec",
        "Kib",
        "kibibits",
        Dim::DATA,
        1_024,
        1,
        Iec,
        Some("kb"),
        "Kib Kibit kibibit kibibits"
    ),
    u!(
        "data_mebibit_iec",
        "Mib",
        "mebibits",
        Dim::DATA,
        1_048_576,
        1,
        Iec,
        Some("Mb"),
        "Mib Mibit mebibit mebibits"
    ),
    u!(
        "data_gibibit_iec",
        "Gib",
        "gibibits",
        Dim::DATA,
        1_073_741_824,
        1,
        Iec,
        Some("Gb"),
        "Gib Gibit gibibit gibibits"
    ),
    u!(
        "data_tebibit_iec",
        "Tib",
        "tebibits",
        Dim::DATA,
        1_099_511_627_776,
        1,
        Iec,
        Some("Tb"),
        "Tib Tibit tebibit tebibits"
    ),
    u!(
        "data_pebibit_iec",
        "Pib",
        "pebibits",
        Dim::DATA,
        1_125_899_906_842_624,
        1,
        Iec,
        Some("Pb"),
        "Pib Pibit pebibit pebibits"
    ),
    u!(
        "data_kilobyte_si",
        "kB",
        "kilobytes",
        Dim::DATA,
        8_000,
        1,
        Si,
        Some("KiB"),
        "kB kilobyte kilobytes"
    ),
    u!(
        "data_megabyte_si",
        "MB",
        "megabytes",
        Dim::DATA,
        8_000_000,
        1,
        Si,
        Some("MiB"),
        "MB megabyte megabytes"
    ),
    u!(
        "data_gigabyte_si",
        "GB",
        "gigabytes",
        Dim::DATA,
        8_000_000_000,
        1,
        Si,
        Some("GiB"),
        "GB gigabyte gigabytes"
    ),
    u!(
        "data_terabyte_si",
        "TB",
        "terabytes",
        Dim::DATA,
        8_000_000_000_000,
        1,
        Si,
        Some("TiB"),
        "TB terabyte terabytes"
    ),
    u!(
        "data_petabyte_si",
        "PB",
        "petabytes",
        Dim::DATA,
        8_000_000_000_000_000,
        1,
        Si,
        Some("PiB"),
        "PB petabyte petabytes"
    ),
    u!(
        "data_kibibyte_iec",
        "KiB",
        "kibibytes",
        Dim::DATA,
        8_192,
        1,
        Iec,
        Some("kB"),
        "KiB kibibyte kibibytes"
    ),
    u!(
        "data_mebibyte_iec",
        "MiB",
        "mebibytes",
        Dim::DATA,
        8_388_608,
        1,
        Iec,
        Some("MB"),
        "MiB mebibyte mebibytes"
    ),
    u!(
        "data_gibibyte_iec",
        "GiB",
        "gibibytes",
        Dim::DATA,
        8_589_934_592,
        1,
        Iec,
        Some("GB"),
        "GiB gibibyte gibibytes"
    ),
    u!(
        "data_tebibyte_iec",
        "TiB",
        "tebibytes",
        Dim::DATA,
        8_796_093_022_208,
        1,
        Iec,
        Some("TB"),
        "TiB tebibyte tebibytes"
    ),
    u!(
        "data_pebibyte_iec",
        "PiB",
        "pebibytes",
        Dim::DATA,
        9_007_199_254_740_992,
        1,
        Iec,
        Some("PB"),
        "PiB pebibyte pebibytes"
    ),
    // Data rate aliases
    u!(
        "bit_per_second_bit_si",
        "bps",
        "bits per second",
        Dim::RATE,
        1,
        1,
        Si,
        None,
        "bps"
    ),
    u!(
        "bit_per_second_kilobit_si",
        "kbps",
        "kilobits per second",
        Dim::RATE,
        1_000,
        1,
        Si,
        Some("KiB/s"),
        "kbps"
    ),
    u!(
        "bit_per_second_megabit_si",
        "Mbps",
        "megabits per second",
        Dim::RATE,
        1_000_000,
        1,
        Si,
        Some("MiB/s"),
        "Mbps"
    ),
    u!(
        "bit_per_second_gigabit_si",
        "Gbps",
        "gigabits per second",
        Dim::RATE,
        1_000_000_000,
        1,
        Si,
        Some("GiB/s"),
        "Gbps"
    ),
    u!(
        "bit_per_second_terabit_si",
        "Tbps",
        "terabits per second",
        Dim::RATE,
        1_000_000_000_000,
        1,
        Si,
        Some("TiB/s"),
        "Tbps"
    ),
    u!(
        "bit_per_second_kibibit_iec",
        "Kibps",
        "kibibits per second",
        Dim::RATE,
        1_024,
        1,
        Iec,
        Some("kbps"),
        "Kibps"
    ),
    u!(
        "bit_per_second_mebibit_iec",
        "Mibps",
        "mebibits per second",
        Dim::RATE,
        1_048_576,
        1,
        Iec,
        Some("Mbps"),
        "Mibps"
    ),
    u!(
        "bit_per_second_gibibit_iec",
        "Gibps",
        "gibibits per second",
        Dim::RATE,
        1_073_741_824,
        1,
        Iec,
        Some("Gbps"),
        "Gibps"
    ),
    u!(
        "byte_per_second_byte_si",
        "Bps",
        "bytes per second",
        Dim::RATE,
        8,
        1,
        Si,
        None,
        "Bps"
    ),
    u!(
        "byte_per_second_kilobyte_si",
        "kBps",
        "kilobytes per second",
        Dim::RATE,
        8_000,
        1,
        Si,
        Some("KiB/s"),
        "kBps"
    ),
    u!(
        "byte_per_second_megabyte_si",
        "MBps",
        "megabytes per second",
        Dim::RATE,
        8_000_000,
        1,
        Si,
        Some("MiB/s"),
        "MBps"
    ),
    u!(
        "byte_per_second_gigabyte_si",
        "GBps",
        "gigabytes per second",
        Dim::RATE,
        8_000_000_000,
        1,
        Si,
        Some("GiB/s"),
        "GBps"
    ),
    // Time (base: s)
    u!(
        "time_nanosecond",
        "ns",
        "nanoseconds",
        Dim::TIME,
        1,
        1_000_000_000,
        Si,
        None,
        "ns nanosecond nanoseconds"
    ),
    u!(
        "time_microsecond",
        "µs",
        "microseconds",
        Dim::TIME,
        1,
        1_000_000,
        Si,
        None,
        "us µs microsecond microseconds"
    ),
    u!(
        "time_millisecond",
        "ms",
        "milliseconds",
        Dim::TIME,
        1,
        1_000,
        Si,
        None,
        "ms millisecond milliseconds"
    ),
    u!(
        "time_second",
        "s",
        "seconds",
        Dim::TIME,
        1,
        1,
        Si,
        None,
        "s sec secs second seconds"
    ),
    u!(
        "time_minute",
        "min",
        "minutes",
        Dim::TIME,
        60,
        1,
        Oth,
        None,
        "min mins minute minutes"
    ),
    u!(
        "time_hour",
        "h",
        "hours",
        Dim::TIME,
        3_600,
        1,
        Oth,
        None,
        "h hr hrs hour hours"
    ),
    u!(
        "time_day",
        "d",
        "days",
        Dim::TIME,
        86_400,
        1,
        Oth,
        None,
        "d day days"
    ),
    u!(
        "time_week",
        "wk",
        "weeks",
        Dim::TIME,
        604_800,
        1,
        Oth,
        None,
        "wk wks week weeks"
    ),
    // Mass (base: kg)
    u!(
        "mass_microgram",
        "µg",
        "micrograms",
        Dim::MASS,
        1,
        1_000_000_000,
        Si,
        None,
        "ug µg microgram micrograms"
    ),
    u!(
        "mass_milligram",
        "mg",
        "milligrams",
        Dim::MASS,
        1,
        1_000_000,
        Si,
        None,
        "mg milligram milligrams"
    ),
    u!(
        "mass_gram",
        "g",
        "grams",
        Dim::MASS,
        1,
        1_000,
        Si,
        None,
        "g gram grams"
    ),
    u!(
        "mass_kilogram",
        "kg",
        "kilograms",
        Dim::MASS,
        1,
        1,
        Si,
        None,
        "kg kilogram kilograms"
    ),
    u!(
        "mass_tonne",
        "t",
        "tonnes",
        Dim::MASS,
        1_000,
        1,
        Si,
        None,
        "t tonne tonnes"
    ),
    u!(
        "mass_ounce",
        "oz",
        "ounces",
        Dim::MASS,
        45_359_237,
        1_600_000_000,
        Imp,
        None,
        "oz ounce ounces"
    ),
    u!(
        "mass_pound",
        "lb",
        "pounds",
        Dim::MASS,
        453_592_370,
        1_000_000_000,
        Imp,
        None,
        "lb lbs pound pounds"
    ),
    u!(
        "mass_stone",
        "st",
        "stones",
        Dim::MASS,
        6_350_293_180,
        1_000_000_000,
        Imp,
        None,
        "st stone stones"
    ),
    u!(
        "mass_short_ton",
        "ton",
        "short tons",
        Dim::MASS,
        907_184_740_000,
        1_000_000_000,
        Imp,
        None,
        "ton tons"
    ),
    // Length (base: m)
    u!(
        "length_nanometre",
        "nm",
        "nanometres",
        Dim::LEN,
        1,
        1_000_000_000,
        Si,
        None,
        "nm nanometre nanometres nanometer nanometers"
    ),
    u!(
        "length_micrometre",
        "µm",
        "micrometres",
        Dim::LEN,
        1,
        1_000_000,
        Si,
        None,
        "um µm micrometre micrometres micrometer micrometers"
    ),
    u!(
        "length_millimetre",
        "mm",
        "millimetres",
        Dim::LEN,
        1,
        1_000,
        Si,
        None,
        "mm millimetre millimetres millimeter millimeters"
    ),
    u!(
        "length_centimetre",
        "cm",
        "centimetres",
        Dim::LEN,
        1,
        100,
        Si,
        None,
        "cm centimetre centimetres centimeter centimeters"
    ),
    u!(
        "length_metre",
        "m",
        "metres",
        Dim::LEN,
        1,
        1,
        Si,
        None,
        "m metre metres meter meters"
    ),
    u!(
        "length_kilometre",
        "km",
        "kilometres",
        Dim::LEN,
        1_000,
        1,
        Si,
        None,
        "km kilometre kilometres kilometer kilometers"
    ),
    u!(
        "length_inch",
        "in",
        "inches",
        Dim::LEN,
        254,
        10_000,
        Imp,
        None,
        "in inch inches"
    ),
    u!(
        "length_foot",
        "ft",
        "feet",
        Dim::LEN,
        3_048,
        10_000,
        Imp,
        None,
        "ft foot feet"
    ),
    u!(
        "length_yard",
        "yd",
        "yards",
        Dim::LEN,
        9_144,
        10_000,
        Imp,
        None,
        "yd yard yards"
    ),
    u!(
        "length_mile",
        "mi",
        "miles",
        Dim::LEN,
        1_609_344,
        1_000,
        Imp,
        None,
        "mi mile miles"
    ),
    u!(
        "length_nautical_mile",
        "nmi",
        "nautical miles",
        Dim::LEN,
        1_852,
        1,
        Oth,
        None,
        "nmi"
    ),
    u!(
        "length_astronomical_unit",
        "au",
        "astronomical units",
        Dim::LEN,
        149_597_870_700,
        1,
        Oth,
        None,
        "au AU"
    ),
    u!(
        "length_light_year",
        "ly",
        "light-years",
        Dim::LEN,
        9_460_730_472_580_800,
        1,
        Oth,
        None,
        "ly"
    ),
    // Area (base: m^2)
    u!(
        "area_square_metre",
        "m²",
        "square metres",
        Dim::AREA,
        1,
        1,
        Si,
        None,
        "m2 sqm"
    ),
    u!(
        "area_square_centimetre",
        "cm²",
        "square centimetres",
        Dim::AREA,
        1,
        10_000,
        Si,
        None,
        "cm2"
    ),
    u!(
        "area_square_millimetre",
        "mm²",
        "square millimetres",
        Dim::AREA,
        1,
        1_000_000,
        Si,
        None,
        "mm2"
    ),
    u!(
        "area_square_kilometre",
        "km²",
        "square kilometres",
        Dim::AREA,
        1_000_000,
        1,
        Si,
        None,
        "km2"
    ),
    u!(
        "area_hectare",
        "ha",
        "hectares",
        Dim::AREA,
        10_000,
        1,
        Si,
        None,
        "ha hectare hectares"
    ),
    u!(
        "area_square_foot",
        "ft²",
        "square feet",
        Dim::AREA,
        9_290_304,
        100_000_000,
        Imp,
        None,
        "ft2 sqft"
    ),
    u!(
        "area_square_inch",
        "in²",
        "square inches",
        Dim::AREA,
        64_516,
        100_000_000,
        Imp,
        None,
        "in2 sqin"
    ),
    u!(
        "area_square_mile",
        "mi²",
        "square miles",
        Dim::AREA,
        2_589_988_110_336,
        1_000_000,
        Imp,
        None,
        "mi2 sqmi"
    ),
    u!(
        "area_acre",
        "acre",
        "acres",
        Dim::AREA,
        404_685_642_240,
        100_000_000,
        Imp,
        None,
        "acre acres"
    ),
    // Volume (base: m^3)
    u!(
        "volume_cubic_metre",
        "m³",
        "cubic metres",
        Dim::VOL,
        1,
        1,
        Si,
        None,
        "m3"
    ),
    u!(
        "volume_cubic_centimetre",
        "cm³",
        "cubic centimetres",
        Dim::VOL,
        1,
        1_000_000,
        Si,
        None,
        "cm3 cc"
    ),
    u!(
        "volume_cubic_millimetre",
        "mm³",
        "cubic millimetres",
        Dim::VOL,
        1,
        1_000_000_000,
        Si,
        None,
        "mm3"
    ),
    u!(
        "volume_litre",
        "L",
        "litres",
        Dim::VOL,
        1,
        1_000,
        Si,
        None,
        "L l litre litres liter liters"
    ),
    u!(
        "volume_millilitre",
        "mL",
        "millilitres",
        Dim::VOL,
        1,
        1_000_000,
        Si,
        None,
        "mL ml millilitre millilitres milliliter milliliters"
    ),
    u!(
        "volume_gallon_us",
        "gal",
        "US gallons",
        Dim::VOL,
        3_785_411_784,
        1_000_000_000_000,
        Imp,
        None,
        "gal gallon gallons"
    ),
    u!(
        "volume_quart_us",
        "qt",
        "US quarts",
        Dim::VOL,
        3_785_411_784,
        4_000_000_000_000,
        Imp,
        None,
        "qt quart quarts"
    ),
    u!(
        "volume_pint_us",
        "pt",
        "US pints",
        Dim::VOL,
        3_785_411_784,
        8_000_000_000_000,
        Imp,
        None,
        "pt pint pints"
    ),
    u!(
        "volume_fluid_ounce_us",
        "floz",
        "US fluid ounces",
        Dim::VOL,
        3_785_411_784,
        128_000_000_000_000,
        Imp,
        None,
        "floz"
    ),
    u!(
        "volume_cubic_foot",
        "ft³",
        "cubic feet",
        Dim::VOL,
        28_316_846_592,
        1_000_000_000_000,
        Imp,
        None,
        "ft3"
    ),
    u!(
        "volume_cubic_inch",
        "in³",
        "cubic inches",
        Dim::VOL,
        16_387_064,
        1_000_000_000_000,
        Imp,
        None,
        "in3"
    ),
    // Speed, Force, Pressure, Energy, Power, Frequency
    u!(
        "speed_km_per_hour",
        "km/h",
        "kilometres per hour",
        Dim::SPEED,
        1_000,
        3_600,
        Si,
        None,
        "kmh kph"
    ),
    u!(
        "speed_mile_per_hour",
        "mph",
        "miles per hour",
        Dim::SPEED,
        1_609_344,
        3_600_000,
        Imp,
        None,
        "mph"
    ),
    u!(
        "speed_knot",
        "kt",
        "knots",
        Dim::SPEED,
        1_852,
        3_600,
        Oth,
        None,
        "kt knot knots"
    ),
    u!(
        "force_newton",
        "N",
        "newtons",
        Dim::FORCE,
        1,
        1,
        Si,
        None,
        "N newton newtons"
    ),
    u!(
        "force_kilonewton",
        "kN",
        "kilonewtons",
        Dim::FORCE,
        1_000,
        1,
        Si,
        None,
        "kN kilonewton kilonewtons"
    ),
    u!(
        "force_pound_force",
        "lbf",
        "pounds-force",
        Dim::FORCE,
        4_448_221_615_260_500,
        1_000_000_000_000_000,
        Imp,
        None,
        "lbf"
    ),
    u!(
        "pressure_pascal",
        "Pa",
        "pascals",
        Dim::PRESS,
        1,
        1,
        Si,
        None,
        "Pa pascal pascals"
    ),
    u!(
        "pressure_kilopascal",
        "kPa",
        "kilopascals",
        Dim::PRESS,
        1_000,
        1,
        Si,
        None,
        "kPa kilopascal kilopascals"
    ),
    u!(
        "pressure_megapascal",
        "MPa",
        "megapascals",
        Dim::PRESS,
        1_000_000,
        1,
        Si,
        None,
        "MPa megapascal megapascals"
    ),
    u!(
        "pressure_bar",
        "bar",
        "bars",
        Dim::PRESS,
        100_000,
        1,
        Oth,
        None,
        "bar bars"
    ),
    u!(
        "pressure_millibar",
        "mbar",
        "millibars",
        Dim::PRESS,
        100,
        1,
        Oth,
        None,
        "mbar millibar millibars"
    ),
    u!(
        "pressure_atmosphere",
        "atm",
        "atmospheres",
        Dim::PRESS,
        101_325,
        1,
        Oth,
        None,
        "atm atmosphere atmospheres"
    ),
    u!(
        "pressure_psi",
        "psi",
        "pounds per square inch",
        Dim::PRESS,
        6_894_757_293_168_361,
        1_000_000_000_000,
        Imp,
        None,
        "psi"
    ),
    u!(
        "energy_joule",
        "J",
        "joules",
        Dim::ENERGY,
        1,
        1,
        Si,
        None,
        "J joule joules"
    ),
    u!(
        "energy_kilojoule",
        "kJ",
        "kilojoules",
        Dim::ENERGY,
        1_000,
        1,
        Si,
        None,
        "kJ kilojoule kilojoules"
    ),
    u!(
        "energy_megajoule",
        "MJ",
        "megajoules",
        Dim::ENERGY,
        1_000_000,
        1,
        Si,
        None,
        "MJ megajoule megajoules"
    ),
    u!(
        "energy_watt_hour",
        "Wh",
        "watt-hours",
        Dim::ENERGY,
        3_600,
        1,
        Si,
        None,
        "Wh"
    ),
    u!(
        "energy_kilowatt_hour",
        "kWh",
        "kilowatt-hours",
        Dim::ENERGY,
        3_600_000,
        1,
        Si,
        None,
        "kWh"
    ),
    u!(
        "energy_megawatt_hour",
        "MWh",
        "megawatt-hours",
        Dim::ENERGY,
        3_600_000_000,
        1,
        Si,
        None,
        "MWh"
    ),
    u!(
        "energy_calorie",
        "cal",
        "calories",
        Dim::ENERGY,
        4_184,
        1_000,
        Oth,
        None,
        "cal calorie calories"
    ),
    u!(
        "energy_kilocalorie",
        "kcal",
        "kilocalories",
        Dim::ENERGY,
        4_184,
        1,
        Oth,
        None,
        "kcal kilocalorie kilocalories"
    ),
    u!(
        "power_milliwatt",
        "mW",
        "milliwatts",
        Dim::POWER,
        1,
        1_000,
        Si,
        None,
        "mW milliwatt milliwatts"
    ),
    u!(
        "power_watt",
        "W",
        "watts",
        Dim::POWER,
        1,
        1,
        Si,
        None,
        "W watt watts"
    ),
    u!(
        "power_kilowatt",
        "kW",
        "kilowatts",
        Dim::POWER,
        1_000,
        1,
        Si,
        None,
        "kW kilowatt kilowatts"
    ),
    u!(
        "power_megawatt",
        "MW",
        "megawatts",
        Dim::POWER,
        1_000_000,
        1,
        Si,
        None,
        "MW megawatt megawatts"
    ),
    u!(
        "power_gigawatt",
        "GW",
        "gigawatts",
        Dim::POWER,
        1_000_000_000,
        1,
        Si,
        None,
        "GW gigawatt gigawatts"
    ),
    u!(
        "power_horsepower",
        "hp",
        "horsepower",
        Dim::POWER,
        745_699_871_582_270_220,
        1_000_000_000_000_000,
        Imp,
        None,
        "hp horsepower"
    ),
    u!(
        "frequency_hertz",
        "Hz",
        "hertz",
        Dim::FREQ,
        1,
        1,
        Si,
        None,
        "Hz hertz"
    ),
    u!(
        "frequency_kilohertz",
        "kHz",
        "kilohertz",
        Dim::FREQ,
        1_000,
        1,
        Si,
        None,
        "kHz kilohertz"
    ),
    u!(
        "frequency_megahertz",
        "MHz",
        "megahertz",
        Dim::FREQ,
        1_000_000,
        1,
        Si,
        None,
        "MHz megahertz"
    ),
    u!(
        "frequency_gigahertz",
        "GHz",
        "gigahertz",
        Dim::FREQ,
        1_000_000_000,
        1,
        Si,
        None,
        "GHz gigahertz"
    ),
    // Temperature (affine via Kelvin)
    u!(
        "temperature_kelvin",
        "K",
        "kelvin",
        Dim::TEMP,
        1,
        1,
        Si,
        None,
        "K kelvin"
    ),
    UnitDef {
        id: "temperature_celsius",
        symbol: "°C",
        description: "degrees Celsius",
        dimension: Dim::TEMP,
        scale_num: 1,
        scale_den: 1,
        offset_num: 5_463,
        offset_den: 20,
        is_affine: true,
        pi_power: 0,
        system: Si,
        counterpart: None,
        aliases: "°C degC celsius Celsius",
    },
    UnitDef {
        id: "temperature_fahrenheit",
        symbol: "°F",
        description: "degrees Fahrenheit",
        dimension: Dim::TEMP,
        scale_num: 5,
        scale_den: 9,
        offset_num: 45_967,
        offset_den: 180,
        is_affine: true,
        pi_power: 0,
        system: Imp,
        counterpart: None,
        aliases: "°F degF fahrenheit Fahrenheit",
    },
    // Angle (base: turn = 1, rad carries pi_power = -1)
    u!(
        "angle_turn",
        "turn",
        "turns",
        Dim::ANG,
        1,
        1,
        Oth,
        None,
        "turn turns rev"
    ),
    u!(
        "angle_degree",
        "deg",
        "degrees",
        Dim::ANG,
        1,
        360,
        Oth,
        None,
        "deg ° degree degrees"
    ),
    u!(
        "angle_gradian",
        "grad",
        "gradians",
        Dim::ANG,
        1,
        400,
        Oth,
        None,
        "grad gradian gradians"
    ),
    UnitDef {
        id: "angle_radian",
        symbol: "rad",
        description: "radians",
        dimension: Dim::ANG,
        scale_num: 1,
        scale_den: 2,
        offset_num: 0,
        offset_den: 1,
        is_affine: false,
        pi_power: -1,
        system: Si,
        counterpart: None,
        aliases: "rad radian radians",
    },
    // Electrical
    u!(
        "current_ampere",
        "A",
        "amperes",
        Dim::CURR,
        1,
        1,
        Si,
        None,
        "A amp amps ampere amperes"
    ),
    u!(
        "current_milliampere",
        "mA",
        "milliamperes",
        Dim::CURR,
        1,
        1_000,
        Si,
        None,
        "mA milliampere milliamperes"
    ),
    u!(
        "voltage_volt",
        "V",
        "volts",
        Dim::VOLT,
        1,
        1,
        Si,
        None,
        "V volt volts"
    ),
    u!(
        "voltage_millivolt",
        "mV",
        "millivolts",
        Dim::VOLT,
        1,
        1_000,
        Si,
        None,
        "mV millivolt millivolts"
    ),
    u!(
        "voltage_kilovolt",
        "kV",
        "kilovolts",
        Dim::VOLT,
        1_000,
        1,
        Si,
        None,
        "kV kilovolt kilovolts"
    ),
    u!(
        "resistance_ohm",
        "Ω",
        "ohms",
        Dim::RES,
        1,
        1,
        Si,
        None,
        "Ω ohm ohms"
    ),
    u!(
        "charge_coulomb",
        "C",
        "coulombs",
        Dim::CHARGE,
        1,
        1,
        Si,
        None,
        "C coulomb coulombs"
    ),
    u!(
        "charge_milliamp_hour",
        "mAh",
        "milliamp-hours",
        Dim::CHARGE,
        3_600,
        1_000,
        Oth,
        None,
        "mAh"
    ),
    u!(
        "charge_amp_hour",
        "Ah",
        "amp-hours",
        Dim::CHARGE,
        3_600,
        1,
        Oth,
        None,
        "Ah"
    ),
];

fn lookup_exact(tok: &str) -> Option<&'static UnitDef> {
    UNITS
        .iter()
        .find(|u| u.aliases.split_ascii_whitespace().any(|a| a == tok))
}

fn lookup_symbol(sym: &str) -> Option<&'static UnitDef> {
    UNITS.iter().find(|u| u.symbol == sym)
}

fn named_compound_meta(
    sym: &str,
) -> Option<(&'static str, &'static str, UnitSystem, Option<&'static str>)> {
    const TABLE: &[(&str, &str, &str, UnitSystem, Option<&str>)] = &[
        (
            "B/s",
            "byte_per_second_byte_si",
            "bytes per second",
            Si,
            None,
        ),
        (
            "kB/s",
            "byte_per_second_kilobyte_si",
            "kilobytes per second",
            Si,
            Some("KiB/s"),
        ),
        (
            "MB/s",
            "byte_per_second_megabyte_si",
            "megabytes per second",
            Si,
            Some("MiB/s"),
        ),
        (
            "GB/s",
            "byte_per_second_gigabyte_si",
            "gigabytes per second",
            Si,
            Some("GiB/s"),
        ),
        (
            "TB/s",
            "byte_per_second_terabyte_si",
            "terabytes per second",
            Si,
            Some("TiB/s"),
        ),
        (
            "KiB/s",
            "byte_per_second_kibibyte_iec",
            "kibibytes per second",
            Iec,
            Some("kB/s"),
        ),
        (
            "MiB/s",
            "byte_per_second_mebibyte_iec",
            "mebibytes per second",
            Iec,
            Some("MB/s"),
        ),
        (
            "GiB/s",
            "byte_per_second_gibibyte_iec",
            "gibibytes per second",
            Iec,
            Some("GB/s"),
        ),
        (
            "TiB/s",
            "byte_per_second_tebibyte_iec",
            "tebibytes per second",
            Iec,
            Some("TB/s"),
        ),
        ("b/s", "bit_per_second_bit_si", "bits per second", Si, None),
        (
            "kb/s",
            "bit_per_second_kilobit_si",
            "kilobits per second",
            Si,
            Some("KiB/s"),
        ),
        (
            "Mb/s",
            "bit_per_second_megabit_si",
            "megabits per second",
            Si,
            Some("MiB/s"),
        ),
        (
            "Gb/s",
            "bit_per_second_gigabit_si",
            "gigabits per second",
            Si,
            Some("GiB/s"),
        ),
        (
            "m/s",
            "speed_metre_per_second",
            "metres per second",
            Si,
            None,
        ),
        ("km/h", "speed_km_per_hour", "kilometres per hour", Si, None),
        ("mi/h", "speed_mile_per_hour", "miles per hour", Imp, None),
        (
            "m/s^2",
            "acceleration_metre_per_second_squared",
            "metres per second squared",
            Si,
            None,
        ),
    ];
    TABLE
        .iter()
        .find(|r| r.0 == sym)
        .map(|r| (r.1, r.2, r.3, r.4))
}

fn ambiguous_data_choices(raw: &str) -> Option<&'static [(&'static str, &'static str)]> {
    match raw {
        "mb" => Some(&[
            ("MB", "Assumed 'mb' means megabytes (MB, SI 10^6 bytes)"),
            ("Mb", "Interpreted 'mb' as megabits (Mb, SI 10^6 bits)"),
            ("MiB", "Interpreted 'mb' as mebibytes (MiB, IEC 2^20 bytes)"),
        ]),
        "gb" => Some(&[
            ("GB", "Assumed 'gb' means gigabytes (GB, SI 10^9 bytes)"),
            ("Gb", "Interpreted 'gb' as gigabits (Gb, SI 10^9 bits)"),
            ("GiB", "Interpreted 'gb' as gibibytes (GiB, IEC 2^30 bytes)"),
        ]),
        "tb" => Some(&[
            ("TB", "Assumed 'tb' means terabytes (TB, SI 10^12 bytes)"),
            ("Tb", "Interpreted 'tb' as terabits (Tb, SI 10^12 bits)"),
            ("TiB", "Interpreted 'tb' as tebibytes (TiB, IEC 2^40 bytes)"),
        ]),
        "pb" => Some(&[
            ("PB", "Assumed 'pb' means petabytes (PB, SI 10^15 bytes)"),
            ("Pb", "Interpreted 'pb' as petabits (Pb, SI 10^15 bits)"),
            ("PiB", "Interpreted 'pb' as pebibytes (PiB, IEC 2^50 bytes)"),
        ]),
        "KB" => Some(&[
            ("kB", "Assumed 'KB' means kilobytes (kB, SI 10^3 bytes)"),
            ("KiB", "Interpreted 'KB' as kibibytes (KiB, IEC 2^10 bytes)"),
        ]),
        "kib" => Some(&[
            ("KiB", "Assumed 'kib' means kibibytes (KiB, IEC 2^10 bytes)"),
            ("Kib", "Interpreted 'kib' as kibibits (Kib, IEC 2^10 bits)"),
        ]),
        "mib" => Some(&[
            ("MiB", "Assumed 'mib' means mebibytes (MiB, IEC 2^20 bytes)"),
            ("Mib", "Interpreted 'mib' as mebibits (Mib, IEC 2^20 bits)"),
        ]),
        "gib" => Some(&[
            ("GiB", "Assumed 'gib' means gibibytes (GiB, IEC 2^30 bytes)"),
            ("Gib", "Interpreted 'gib' as gibibits (Gib, IEC 2^30 bits)"),
        ]),
        "mbps" => Some(&[
            ("Mbps", "Assumed 'mbps' means megabits per second (Mbps)"),
            ("MBps", "Interpreted 'mbps' as megabytes per second (MB/s)"),
        ]),
        "gbps" => Some(&[
            ("Gbps", "Assumed 'gbps' means gigabits per second (Gbps)"),
            ("GBps", "Interpreted 'gbps' as gigabytes per second (GB/s)"),
        ]),
        _ => None,
    }
}

const KB_PAIRED: &[(&str, &str)] = &[
    ("kB", "Assumed 'kb' means kilobytes (kB, SI 10^3 bytes)"),
    (
        "kb",
        "Interpreted 'kb' literally as kilobits (kb, SI 10^3 bits)",
    ),
    ("KiB", "Interpreted 'kb' as kibibytes (KiB, IEC 2^10 bytes)"),
];

// ---------------------------------------------------------------------------
// Lexer & UTF-16 span tracking
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum TokKind {
    Number(String),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    LParen,
    RParen,
    Arrow,
}

#[derive(Debug, Clone)]
struct Token {
    kind: TokKind,
    text: String,
    start_utf16: usize,
    end_utf16: usize,
}

fn normalize_char(c: char) -> char {
    match c {
        '−' | '–' | '—' => '-',
        '×' | '·' => '*',
        '÷' => '/',
        'μ' => 'µ',
        '\u{00A0}' | '\u{2007}' | '\u{202F}' | '\u{3000}' => ' ',
        '\u{FF01}'..='\u{FF5E}' => char::from_u32((c as u32) - 0xFEE0).unwrap_or(c),
        _ => c,
    }
}

fn superscript_to_ascii(c: char) -> Option<char> {
    match c {
        '⁰' => Some('0'),
        '¹' => Some('1'),
        '²' => Some('2'),
        '³' => Some('3'),
        '⁴' => Some('4'),
        '⁵' => Some('5'),
        '⁶' => Some('6'),
        '⁷' => Some('7'),
        '⁸' => Some('8'),
        '⁹' => Some('9'),
        '⁻' => Some('-'),
        _ => None,
    }
}

fn is_skippable_emoji(c: char) -> bool {
    matches!(c as u32, 0x2600..=0x27BF | 0x1F000..=0x1FAFF | 0xFE00..=0xFE0F | 0x200D)
}

fn raw_tokenize(input: &str) -> Result<Vec<Token>, Diagnostic> {
    let mut chars = Vec::new();
    let mut u16 = 0usize;
    for raw_c in input.chars() {
        chars.push((u16, raw_c, normalize_char(raw_c)));
        u16 += raw_c.len_utf16();
    }

    let mut tokens = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        let (su16, raw_c, c) = chars[i];
        if c.is_whitespace() || is_skippable_emoji(c) {
            i += 1;
            continue;
        }
        if tokens.len() >= MAX_TOKENS {
            return Err(limit_diag("resource_limit", "too_many_tokens", None));
        }
        if c == '→' || (c == '-' && chars.get(i + 1).is_some_and(|n| n.2 == '>')) {
            let step = if c == '→' { 1 } else { 2 };
            let eu16 = chars[i + step - 1].0 + chars[i + step - 1].1.len_utf16();
            tokens.push(Token {
                kind: TokKind::Arrow,
                text: if step == 1 { "→" } else { "->" }.into(),
                start_utf16: su16,
                end_utf16: eu16,
            });
            i += step;
            continue;
        }
        if let Some(sc) = superscript_to_ascii(c) {
            let mut exp = String::from(sc);
            let mut j = i + 1;
            while j < chars.len() && superscript_to_ascii(chars[j].2).is_some_and(|d| d != '-') {
                exp.push(superscript_to_ascii(chars[j].2).unwrap());
                j += 1;
            }
            let eu16 = chars[j - 1].0 + chars[j - 1].1.len_utf16();
            let c_end = su16 + raw_c.len_utf16();
            tokens.push(Token {
                kind: TokKind::Caret,
                text: "^".into(),
                start_utf16: su16,
                end_utf16: c_end,
            });
            if let Some(rest) = exp.strip_prefix('-') {
                tokens.push(Token {
                    kind: TokKind::Minus,
                    text: "-".into(),
                    start_utf16: su16,
                    end_utf16: c_end,
                });
                if !rest.is_empty() {
                    tokens.push(Token {
                        kind: TokKind::Number(rest.into()),
                        text: rest.into(),
                        start_utf16: su16,
                        end_utf16: eu16,
                    });
                }
            } else {
                tokens.push(Token {
                    kind: TokKind::Number(exp.clone()),
                    text: exp,
                    start_utf16: su16,
                    end_utf16: eu16,
                });
            }
            i = j;
            continue;
        }
        if let Some(kind) = match c {
            '+' => Some(TokKind::Plus),
            '-' => Some(TokKind::Minus),
            '*' => Some(TokKind::Star),
            '/' => Some(TokKind::Slash),
            '^' => Some(TokKind::Caret),
            '(' => Some(TokKind::LParen),
            ')' => Some(TokKind::RParen),
            _ => None,
        } {
            tokens.push(Token {
                kind,
                text: c.into(),
                start_utf16: su16,
                end_utf16: su16 + raw_c.len_utf16(),
            });
            i += 1;
            continue;
        }
        if c.is_ascii_digit()
            || (c == '.' && chars.get(i + 1).is_some_and(|n| n.2.is_ascii_digit()))
        {
            let mut j = i;
            let mut num = String::new();
            let mut dot = false;
            while j < chars.len() {
                let nc = chars[j].2;
                if nc.is_ascii_digit() {
                    num.push(nc);
                    j += 1;
                } else if nc == '_'
                    && !num.is_empty()
                    && chars.get(j + 1).is_some_and(|n| n.2.is_ascii_digit())
                {
                    j += 1;
                } else if nc == '.' && !dot {
                    dot = true;
                    num.push('.');
                    j += 1;
                } else {
                    break;
                }
            }
            if chars.get(j).is_some_and(|n| matches!(n.2, 'e' | 'E')) {
                let mut k = j + 1;
                let sign = chars
                    .get(k)
                    .and_then(|n| matches!(n.2, '+' | '-').then_some(n.2));
                if sign.is_some() {
                    k += 1;
                }
                let exp_start = k;
                let mut exp_digits = String::new();
                while k < chars.len() && chars[k].2.is_ascii_digit() {
                    exp_digits.push(chars[k].2);
                    k += 1;
                }
                if k > exp_start {
                    num.push('e');
                    if let Some(s) = sign {
                        num.push(s);
                    }
                    num.push_str(&exp_digits);
                    j = k;
                }
            }
            let eu16 = chars[j - 1].0 + chars[j - 1].1.len_utf16();
            tokens.push(Token {
                kind: TokKind::Number(num.clone()),
                text: num,
                start_utf16: su16,
                end_utf16: eu16,
            });
            i = j;
            continue;
        }
        if c.is_alphabetic() || matches!(c, '°' | 'µ' | 'Ω' | '℃' | '℉') {
            if matches!(c, '℃' | '℉') {
                let sym = if c == '℃' { "°C" } else { "°F" };
                tokens.push(Token {
                    kind: TokKind::Ident(sym.into()),
                    text: sym.into(),
                    start_utf16: su16,
                    end_utf16: su16 + raw_c.len_utf16(),
                });
                i += 1;
                continue;
            }
            let mut j = i;
            let mut id = String::new();
            while j < chars.len()
                && (chars[j].2.is_alphanumeric() || matches!(chars[j].2, '_' | '°' | 'µ' | 'Ω'))
            {
                id.push(chars[j].2);
                j += 1;
            }
            let eu16 = chars[j - 1].0 + chars[j - 1].1.len_utf16();
            tokens.push(Token {
                kind: TokKind::Ident(id.clone()),
                text: id,
                start_utf16: su16,
                end_utf16: eu16,
            });
            i = j;
            continue;
        }
        return Err(Diagnostic::error("invalid_syntax"));
    }
    Ok(tokens)
}

fn tokenize_with_spans(query: &str, original: &str) -> Result<Vec<Token>, Diagnostic> {
    let mut q_toks = raw_tokenize(query)?;
    if query == original {
        return Ok(q_toks);
    }
    if let Ok(o_toks) = raw_tokenize(original)
        && o_toks.len() == q_toks.len()
        && o_toks.iter().zip(&q_toks).all(|(o, q)| o.kind == q.kind)
    {
        for (q, o) in q_toks.iter_mut().zip(o_toks) {
            q.start_utf16 = o.start_utf16;
            q.end_utf16 = o.end_utf16;
        }
        return Ok(q_toks);
    }
    for q in &mut q_toks {
        if let Some(pos) = original.find(&q.text) {
            let su16: usize = original[..pos].chars().map(char::len_utf16).sum();
            let lu16: usize = q.text.chars().map(char::len_utf16).sum();
            q.start_utf16 = su16;
            q.end_utf16 = su16 + lu16;
        }
    }
    Ok(q_toks)
}

// ---------------------------------------------------------------------------
// Preflight route exclusions & conversion split
// ---------------------------------------------------------------------------

fn looks_unit_related_quick(s: &str) -> bool {
    let low = s.to_ascii_lowercase();
    low.contains(" to ")
        || low.contains(" in ")
        || s.contains("->")
        || s.contains(['→', '/', '*', '×'])
}

fn is_non_unit_domain_or_prose(s: &str) -> bool {
    if s.starts_with(['/', '$', '>'])
        || s.starts_with("./")
        || s.starts_with("../")
        || s.starts_with("~/")
        || s.contains("://")
    {
        return true;
    }
    if s.contains('.')
        && s.split_whitespace().count() == 1
        && s.rsplit('.').next().is_some_and(|e| {
            (2..=5).contains(&e.len()) && e.chars().all(|c| c.is_ascii_alphabetic())
        })
    {
        return true;
    }
    if s.contains(['¥', '€', '£', '₹', '₩', '$', '&', '|', '~', '=', ';']) {
        return true;
    }
    let low = s.to_ascii_lowercase();
    low.contains("0x")
        || low.contains("0b")
        || low.contains("0o")
        || low.contains(" as ")
        || low.contains("<<")
        || low.contains(">>")
        || low.contains(" xor ")
}

fn has_non_calc_markers(tokens: &[Token]) -> bool {
    const NON_UNIT: &[&str] = &[
        "usd", "eur", "cny", "jpy", "gbp", "krw", "twd", "aud", "cad", "chf", "hkd", "sgd", "pst",
        "pdt", "est", "edt", "cst", "cdt", "ist", "utc", "gmt", "beijing", "shanghai", "tokyo",
        "london", "fp32", "fp16", "f16", "bf16", "fp8", "e4m3fn", "e5m2", "sqrt", "sin", "cos",
        "tan", "log", "ln", "abs", "pi", "π",
    ];
    tokens.iter().any(|t| matches!(&t.kind, TokKind::Ident(id) if NON_UNIT.contains(&id.to_ascii_lowercase().as_str())))
}

fn find_conversion_split(tokens: &[Token]) -> Option<usize> {
    if let Some(idx) = tokens.iter().position(|t| {
        matches!(t.kind, TokKind::Arrow)
            || matches!(&t.kind, TokKind::Ident(s) if s.eq_ignore_ascii_case("to"))
    }) {
        return Some(idx);
    }
    let ins: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            matches!(&t.kind, TokKind::Ident(s) if s.eq_ignore_ascii_case("in")).then_some(i)
        })
        .collect();
    if ins.len() <= 1 {
        return ins.first().copied();
    }
    ins.iter()
        .copied()
        .find(|&idx| {
            idx > 0
                && idx + 1 < tokens.len()
                && tokens[..idx]
                    .iter()
                    .any(|t| matches!(t.kind, TokKind::Ident(_)))
                && parse_tokens(&tokens[idx + 1..], None, 0, true).is_ok()
        })
        .or(Some(ins[1]))
}

fn has_any_known_unit(tokens: &[Token]) -> bool {
    tokens.iter().any(|t| matches!(&t.kind, TokKind::Ident(n) if lookup_exact(n).is_some() || ambiguous_data_choices(n).is_some()))
}

// ---------------------------------------------------------------------------
// Exact BigRational numbers & decimal formatting
// ---------------------------------------------------------------------------

fn parse_exact_number(raw: &str) -> Result<BigRational, Diagnostic> {
    if raw.len() > MAX_INTEGER_DIGITS + 64 {
        return Err(limit_diag(
            "resource_limit",
            "number_literal_too_long",
            None,
        ));
    }
    let (mantissa, exp_str) = raw
        .split_once(['e', 'E'])
        .map_or((raw, None), |(m, e)| (m, Some(e)));
    let exp: i32 = match exp_str {
        Some(es) => {
            let p: i32 = es
                .parse()
                .map_err(|_| limit_diag("numeric_overflow", "exponent_out_of_range", None))?;
            if p.unsigned_abs() > MAX_SCI_EXPONENT as u32 {
                return Err(limit_diag(
                    "numeric_overflow",
                    "exponent_out_of_range",
                    None,
                ));
            }
            p
        }
        None => 0,
    };
    let (int_p, frac_p) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if int_p.len() + frac_p.len() > MAX_INTEGER_DIGITS {
        return Err(limit_diag("resource_limit", "too_many_digits", None));
    }
    let digits = format!("{int_p}{frac_p}");
    let numer = BigInt::parse_bytes(digits.as_bytes(), 10)
        .ok_or_else(|| Diagnostic::error("invalid_syntax"))?;
    let net_exp = exp - (frac_p.len() as i32);
    if net_exp.unsigned_abs() > MAX_SCI_EXPONENT as u32 + MAX_INTEGER_DIGITS as u32 {
        return Err(limit_diag(
            "numeric_overflow",
            "exponent_out_of_range",
            None,
        ));
    }
    let ten = BigInt::from(10u8);
    Ok(if net_exp >= 0 {
        BigRational::from_integer(numer * ten.pow(net_exp as u32))
    } else {
        BigRational::new(numer, ten.pow((-net_exp) as u32))
    })
}

fn checked_pow_rational(base: &BigRational, exp: i32) -> Result<BigRational, Diagnostic> {
    if exp.unsigned_abs() > MAX_VALUE_EXPONENT as u32 {
        return Err(limit_diag(
            "numeric_overflow",
            "exponent_out_of_range",
            None,
        ));
    }
    if exp == 0 {
        return Ok(BigRational::one());
    }
    let bits = base.numer().bits().max(base.denom().bits());
    if bits.saturating_mul(exp.unsigned_abs() as u64) > MAX_BIGINT_BITS {
        return Err(limit_diag(
            "numeric_overflow",
            "result_exceeds_bit_limit",
            None,
        ));
    }
    if exp > 0 {
        Ok(base.pow(exp))
    } else if base.is_zero() {
        Err(limit_diag("invalid_syntax", "division_by_zero", None))
    } else {
        Ok(base.recip().pow(-exp))
    }
}

struct FormattedDecimal {
    full: String,
    display: String,
    exact: bool,
}

fn format_rational_decimal(val: &BigRational, is_exact_source: bool) -> FormattedDecimal {
    if val.is_zero() {
        return FormattedDecimal {
            full: "0".into(),
            display: "0".into(),
            exact: is_exact_source,
        };
    }
    let neg = val.is_negative();
    let abs = val.abs();
    let (numer, denom) = (abs.numer(), abs.denom());

    if let Some(exact_dec) = try_exact_terminating_decimal(numer, denom, 24) {
        let full = if neg {
            format!("-{exact_dec}")
        } else {
            exact_dec
        };
        let display = round_decimal_for_display(&full, 9);
        return FormattedDecimal {
            full,
            display,
            exact: is_exact_source,
        };
    }
    let full_abs = round_rational(numer, denom, 15);
    let disp_abs = round_rational(numer, denom, 9);
    let sign = |s: String| if neg && s != "0" { format!("-{s}") } else { s };
    FormattedDecimal {
        full: sign(full_abs),
        display: sign(disp_abs),
        exact: false,
    }
}

fn try_exact_terminating_decimal(
    numer: &BigInt,
    denom: &BigInt,
    max_frac: usize,
) -> Option<String> {
    let mut d = denom.clone();
    let (two, five) = (BigInt::from(2u8), BigInt::from(5u8));
    let (mut p2, mut p5) = (0usize, 0usize);
    while (&d % &two).is_zero() {
        d /= &two;
        p2 += 1;
        if p2 > max_frac {
            return None;
        }
    }
    while (&d % &five).is_zero() {
        d /= &five;
        p5 += 1;
        if p5 > max_frac {
            return None;
        }
    }
    if !d.is_one() {
        return None;
    }
    let k = p2.max(p5);
    let scaled = numer * two.pow((k - p2) as u32) * five.pow((k - p5) as u32);
    Some(insert_decimal_point(&scaled.to_string(), k))
}

fn round_rational(numer: &BigInt, denom: &BigInt, places: usize) -> String {
    let ten = BigInt::from(10u8);
    let frac_places = if (numer / denom).is_zero() {
        let mut rem = numer % denom;
        let mut lz = 0usize;
        while !rem.is_zero() && &rem * &ten < *denom && lz < 30 {
            rem *= &ten;
            lz += 1;
        }
        places + lz
    } else {
        places
    };
    let scaled = numer * ten.pow(frac_places as u32);
    let (q, r) = (&scaled / denom, &scaled % denom);
    let rounded = if r * BigInt::from(2u8) >= *denom {
        q + BigInt::one()
    } else {
        q
    };
    insert_decimal_point(&rounded.to_string(), frac_places)
}

fn insert_decimal_point(digits: &str, frac_places: usize) -> String {
    if frac_places == 0 {
        return digits.to_string();
    }
    let mut s = if digits.len() <= frac_places {
        format!("{}{digits}", "0".repeat(frac_places + 1 - digits.len()))
    } else {
        digits.to_string()
    };
    s.insert(s.len() - frac_places, '.');
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn round_decimal_for_display(dec: &str, max_frac: usize) -> String {
    let neg = dec.starts_with('-');
    let unsigned = dec.strip_prefix('-').unwrap_or(dec);
    let Some((int_p, frac_p)) = unsigned.split_once('.') else {
        return dec.to_string();
    };
    let eff_max = if int_p == "0" {
        max_frac.max(frac_p.chars().take_while(|&c| c == '0').count() + 6)
    } else {
        max_frac
    };
    if frac_p.len() <= eff_max {
        return dec.to_string();
    }
    let all = format!("{int_p}{frac_p}");
    let keep = all.len() - (frac_p.len() - eff_max);
    let mut val = BigInt::parse_bytes(&all.as_bytes()[..keep], 10).unwrap_or_default();
    if all.as_bytes()[keep] >= b'5' {
        val += BigInt::one();
    }
    let rounded = insert_decimal_point(&val.to_string(), eff_max);
    if neg && rounded != "0" {
        format!("-{rounded}")
    } else {
        rounded
    }
}

fn format_duration_hms(seconds: &BigRational) -> Option<String> {
    let sixty = BigRational::from_integer(BigInt::from(60u8));
    let max_s = BigRational::from_integer(BigInt::from(1_000_000_000_000i64));
    if !seconds.is_positive() || seconds < &sixty || seconds > &max_s {
        return None;
    }
    let total = (seconds.numer() / seconds.denom())
        .to_string()
        .parse::<u64>()
        .ok()?;
    let frac = seconds - BigRational::from_integer(BigInt::from(total));
    let (days, rem_d) = (total / 86_400, total % 86_400);
    let (hours, rem_h) = (rem_d / 3_600, rem_d % 3_600);
    let (mins, secs) = (rem_h / 60, rem_h % 60);
    let mut parts = Vec::new();
    if days > 0 {
        parts.push(format!("{days} d"));
    }
    if hours > 0 {
        parts.push(format!("{hours} h"));
    }
    if mins > 0 {
        parts.push(format!("{mins} min"));
    }
    if secs > 0 || !frac.is_zero() {
        let s_val = BigRational::from_integer(BigInt::from(secs)) + frac;
        parts.push(format!(
            "{} s",
            format_rational_decimal(&s_val, true).display
        ));
    }
    (!parts.is_empty()).then(|| parts.join(" "))
}

// ---------------------------------------------------------------------------
// Unified bounded typed expression & compound unit parser
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct ResolvedUnit {
    id: String,
    symbol: String,
    description: String,
    dimension: Dim,
    scale: BigRational,
    offset: BigRational,
    is_affine: bool,
    pi_power: i8,
    system: UnitSystem,
    counterpart: Option<String>,
    assumptions: Vec<String>,
}

impl ResolvedUnit {
    fn from_def(d: &UnitDef) -> Self {
        Self {
            id: d.id.into(),
            symbol: d.symbol.into(),
            description: d.description.into(),
            dimension: d.dimension,
            scale: d.scale_rational(),
            offset: d.offset_rational(),
            is_affine: d.is_affine,
            pi_power: d.pi_power,
            system: d.system,
            counterpart: d.counterpart.map(str::to_string),
            assumptions: Vec::new(),
        }
    }
    fn dimensionless() -> Self {
        Self {
            id: "dimensionless".into(),
            symbol: String::new(),
            description: "dimensionless".into(),
            dimension: Dim::ZERO,
            scale: BigRational::one(),
            offset: BigRational::zero(),
            is_affine: false,
            pi_power: 0,
            system: Oth,
            counterpart: None,
            assumptions: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
struct Quantity {
    value: BigRational,
    unit: ResolvedUnit,
    has_unit: bool,
}

#[derive(Debug, Clone)]
enum EvalErr {
    UnknownUnit {
        token: String,
        start_utf16: usize,
        end_utf16: usize,
    },
    Diagnostic(Diagnostic),
}

type OverrideMap = BTreeMap<String, (&'static UnitDef, String)>;

fn parse_tokens(
    tokens: &[Token],
    overrides: Option<&OverrideMap>,
    orig_len_utf16: usize,
    unit_only: bool,
) -> Result<Quantity, EvalErr> {
    if tokens.is_empty() {
        return Err(EvalErr::Diagnostic(Diagnostic::error("invalid_syntax")));
    }
    let mut p = Parser {
        tokens,
        pos: 0,
        depth: 0,
        overrides,
        orig_len_utf16,
        unit_only,
    };
    let q = if unit_only {
        p.parse_mul_div()?
    } else {
        p.parse_add_sub()?
    };
    if p.pos != tokens.len() {
        return Err(EvalErr::Diagnostic(Diagnostic::error("invalid_syntax")));
    }
    Ok(q)
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
    depth: usize,
    overrides: Option<&'a OverrideMap>,
    orig_len_utf16: usize,
    unit_only: bool,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn parse_add_sub(&mut self) -> Result<Quantity, EvalErr> {
        let mut lhs = self.parse_mul_div()?;
        while let Some(tok) = self.peek().cloned() {
            let is_add = match tok.kind {
                TokKind::Plus => true,
                TokKind::Minus => false,
                _ => break,
            };
            let span = json!({"startUtf16": tok.start_utf16, "endUtf16": tok.end_utf16});
            self.pos += 1;
            let rhs = self.parse_mul_div()?;
            if lhs.unit.is_affine || rhs.unit.is_affine {
                return Err(EvalErr::Diagnostic(limit_diag(
                    "invalid_syntax",
                    "offset_temperature_arithmetic",
                    Some(span),
                )));
            }
            if lhs.unit.dimension != rhs.unit.dimension {
                return Err(EvalErr::Diagnostic(dimension_mismatch_diagnostic(
                    lhs.unit.dimension,
                    rhs.unit.dimension,
                    &lhs.unit,
                    &rhs.unit,
                    span,
                    None,
                    self.orig_len_utf16,
                )));
            }
            if lhs.unit.pi_power != rhs.unit.pi_power {
                return Err(EvalErr::Diagnostic(limit_diag(
                    "unsupported_format",
                    "irrational_angle_addition",
                    Some(span),
                )));
            }
            let rhs_in_lhs = &rhs.value * (&rhs.unit.scale / &lhs.unit.scale);
            lhs.value = if is_add {
                lhs.value + rhs_in_lhs
            } else {
                lhs.value - rhs_in_lhs
            };
            lhs.unit.assumptions.extend(rhs.unit.assumptions);
            lhs.has_unit |= rhs.has_unit;
        }
        Ok(lhs)
    }

    fn parse_mul_div(&mut self) -> Result<Quantity, EvalErr> {
        let mut lhs = self.parse_unary()?;
        while let Some(tok) = self.peek().cloned() {
            match &tok.kind {
                TokKind::Star => {
                    self.pos += 1;
                    if !self.unit_only
                        && lhs.unit.dimension == Dim::MASS
                        && self
                            .peek()
                            .is_some_and(|n| matches!(&n.kind, TokKind::Ident(s) if s == "g"))
                        && !self
                            .tokens
                            .get(self.pos + 1)
                            .is_some_and(|n| n.kind == TokKind::Caret)
                    {
                        self.pos += 1;
                        lhs = apply_standard_gravity(lhs)?;
                        continue;
                    }
                    let rhs = self.parse_unary()?;
                    if !self.unit_only
                        && lhs.unit.id == "mass_gram"
                        && rhs.unit.dimension == Dim::MASS
                        && lhs.value == BigRational::new(BigInt::one(), BigInt::from(1000))
                    {
                        lhs = apply_standard_gravity(rhs)?;
                        continue;
                    }
                    lhs = mul_div_quantities(lhs, rhs, false, None)?;
                }
                TokKind::Slash => {
                    let span = json!({"startUtf16": tok.start_utf16, "endUtf16": tok.end_utf16});
                    self.pos += 1;
                    let rhs = self.parse_unary()?;
                    lhs = mul_div_quantities(lhs, rhs, true, Some(span))?;
                }
                TokKind::Ident(s) if s == "per" => {
                    let span = json!({"startUtf16": tok.start_utf16, "endUtf16": tok.end_utf16});
                    self.pos += 1;
                    let rhs = self.parse_unary()?;
                    lhs = mul_div_quantities(lhs, rhs, true, Some(span))?;
                }
                _ => break,
            }
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self) -> Result<Quantity, EvalErr> {
        if !self.unit_only
            && let Some(tok) = self.peek()
        {
            match tok.kind {
                TokKind::Plus => {
                    self.pos += 1;
                    return self.parse_unary();
                }
                TokKind::Minus => {
                    self.pos += 1;
                    let mut q = self.parse_unary()?;
                    q.value = -q.value;
                    return Ok(q);
                }
                _ => {}
            }
        }
        self.parse_juxt()
    }

    fn parse_juxt(&mut self) -> Result<Quantity, EvalErr> {
        let mut lhs = self.parse_pow()?;
        while let Some(tok) = self.peek() {
            let is_per = matches!(&tok.kind, TokKind::Ident(s) if s == "per");
            if matches!(tok.kind, TokKind::Ident(_) | TokKind::LParen) && !is_per {
                let rhs = self.parse_pow()?;
                lhs = mul_div_quantities(lhs, rhs, false, None)?;
            } else {
                break;
            }
        }
        Ok(lhs)
    }

    fn parse_pow(&mut self) -> Result<Quantity, EvalErr> {
        let base = self.parse_atom()?;
        if let Some(tok) = self.peek().cloned()
            && tok.kind == TokKind::Caret
        {
            let span = json!({"startUtf16": tok.start_utf16, "endUtf16": tok.end_utf16});
            self.pos += 1;
            let exp_val = self.parse_exp_value(&span)?;
            if base.has_unit && exp_val.unsigned_abs() > MAX_UNIT_EXPONENT as u32 {
                return Err(EvalErr::Diagnostic(limit_diag(
                    "numeric_overflow",
                    "unit_exponent_out_of_range",
                    Some(span),
                )));
            }
            let new_val =
                checked_pow_rational(&base.value, exp_val).map_err(EvalErr::Diagnostic)?;
            let new_unit = if base.has_unit {
                pow_unit(base.unit, exp_val)?
            } else {
                base.unit
            };
            return Ok(Quantity {
                value: new_val,
                unit: new_unit,
                has_unit: base.has_unit,
            });
        }
        Ok(base)
    }

    fn parse_exp_value(&mut self, span: &Value) -> Result<i32, EvalErr> {
        let mut neg = false;
        if let Some(tok) = self.peek() {
            match tok.kind {
                TokKind::Minus => {
                    neg = true;
                    self.pos += 1;
                }
                TokKind::Plus => {
                    self.pos += 1;
                }
                _ => {}
            }
        }
        let Some(tok) = self.peek().cloned() else {
            return Err(EvalErr::Diagnostic(Diagnostic::error("invalid_syntax")));
        };
        let TokKind::Number(ref s) = tok.kind else {
            return Err(EvalErr::Diagnostic(Diagnostic::error(if self.unit_only {
                "invalid_syntax"
            } else {
                "unsupported_format"
            })));
        };
        self.pos += 1;
        if s.contains(['.', 'e', 'E']) {
            return Err(EvalErr::Diagnostic(limit_diag(
                "unsupported_format",
                "fractional_exponent",
                Some(span.clone()),
            )));
        }
        let parsed: i32 = s.parse().map_err(|_| {
            EvalErr::Diagnostic(limit_diag(
                "numeric_overflow",
                "exponent_out_of_range",
                Some(span.clone()),
            ))
        })?;
        Ok(if neg { -parsed } else { parsed })
    }

    fn parse_atom(&mut self) -> Result<Quantity, EvalErr> {
        let Some(tok) = self.peek().cloned() else {
            return Err(EvalErr::Diagnostic(Diagnostic::error("invalid_syntax")));
        };
        match tok.kind {
            TokKind::Number(ref s) if !self.unit_only => {
                self.pos += 1;
                let value = parse_exact_number(s).map_err(EvalErr::Diagnostic)?;
                Ok(Quantity {
                    value,
                    unit: ResolvedUnit::dimensionless(),
                    has_unit: false,
                })
            }
            TokKind::Ident(ref name) => {
                self.pos += 1;
                if let Some(ov) = self.overrides
                    && let Some((def, note)) = ov.get(name)
                {
                    let mut unit = ResolvedUnit::from_def(def);
                    if !note.is_empty() {
                        unit.assumptions.push(note.clone());
                    }
                    return Ok(Quantity {
                        value: BigRational::one(),
                        unit,
                        has_unit: true,
                    });
                }
                if let Some(def) = lookup_exact(name) {
                    return Ok(Quantity {
                        value: BigRational::one(),
                        unit: ResolvedUnit::from_def(def),
                        has_unit: true,
                    });
                }
                Err(EvalErr::UnknownUnit {
                    token: name.clone(),
                    start_utf16: tok.start_utf16,
                    end_utf16: tok.end_utf16,
                })
            }
            TokKind::LParen => {
                if self.depth >= MAX_PAREN_DEPTH {
                    return Err(EvalErr::Diagnostic(limit_diag(
                        "resource_limit",
                        "max_paren_depth",
                        None,
                    )));
                }
                self.depth += 1;
                self.pos += 1;
                let inner = if self.unit_only {
                    self.parse_mul_div()?
                } else {
                    self.parse_add_sub()?
                };
                if self.peek().is_some_and(|t| t.kind == TokKind::RParen) {
                    self.pos += 1;
                    self.depth -= 1;
                    Ok(inner)
                } else {
                    Err(EvalErr::Diagnostic(Diagnostic::error("invalid_syntax")))
                }
            }
            _ => Err(EvalErr::Diagnostic(Diagnostic::error("invalid_syntax"))),
        }
    }
}

fn mul_div_quantities(
    a: Quantity,
    b: Quantity,
    is_div: bool,
    span: Option<Value>,
) -> Result<Quantity, EvalErr> {
    if is_div && b.value.is_zero() {
        return Err(EvalErr::Diagnostic(limit_diag(
            "invalid_syntax",
            "division_by_zero",
            span,
        )));
    }
    let value = if is_div {
        a.value / b.value
    } else {
        a.value * b.value
    };
    if a.unit.is_affine && (b.has_unit || is_div || a.has_unit && !b.has_unit) {
        return Err(EvalErr::Diagnostic(limit_diag(
            "invalid_syntax",
            "offset_temperature_arithmetic",
            span,
        )));
    }
    if b.unit.is_affine && (is_div || a.has_unit) {
        return Err(EvalErr::Diagnostic(limit_diag(
            "invalid_syntax",
            "offset_temperature_arithmetic",
            span,
        )));
    }
    let (unit, has_unit) = match (a.has_unit, b.has_unit) {
        (false, false) => (ResolvedUnit::dimensionless(), false),
        (true, false) => (a.unit, true),
        (false, true) if !is_div => (b.unit, true),
        _ => (combine_units(a.unit, b.unit, is_div)?, true),
    };
    Ok(Quantity {
        value,
        unit,
        has_unit,
    })
}

fn combine_units(a: ResolvedUnit, b: ResolvedUnit, is_div: bool) -> Result<ResolvedUnit, EvalErr> {
    let sign = if is_div { -1 } else { 1 };
    let dim = a.dimension.combine(b.dimension, sign).ok_or_else(|| {
        EvalErr::Diagnostic(limit_diag(
            "numeric_overflow",
            "dimension_exponent_overflow",
            None,
        ))
    })?;
    let pi_power = if is_div {
        a.pi_power.checked_sub(b.pi_power)
    } else {
        a.pi_power.checked_add(b.pi_power)
    }
    .ok_or_else(|| {
        EvalErr::Diagnostic(limit_diag(
            "numeric_overflow",
            "dimension_exponent_overflow",
            None,
        ))
    })?;
    let scale = if is_div {
        a.scale / b.scale
    } else {
        a.scale * b.scale
    };
    let symbol = if a.symbol.is_empty() {
        b.symbol
    } else if b.symbol.is_empty() {
        a.symbol
    } else {
        format!("{}{}{}", a.symbol, if is_div { "/" } else { "*" }, b.symbol)
    };
    let mut assumptions = a.assumptions;
    assumptions.extend(b.assumptions);
    if is_div && let Some((cid, cdesc, csys, cctr)) = named_compound_meta(&symbol) {
        return Ok(ResolvedUnit {
            id: cid.into(),
            symbol,
            description: cdesc.into(),
            dimension: dim,
            scale,
            offset: BigRational::zero(),
            is_affine: false,
            pi_power,
            system: csys,
            counterpart: cctr.map(str::to_string),
            assumptions,
        });
    }
    let sep = if is_div { "/" } else { "*" };
    Ok(ResolvedUnit {
        id: format!("compound:{}{sep}{}", a.id, b.id),
        symbol: symbol.clone(),
        description: symbol,
        dimension: dim,
        scale,
        offset: BigRational::zero(),
        is_affine: false,
        pi_power,
        system: Oth,
        counterpart: None,
        assumptions,
    })
}

fn pow_unit(a: ResolvedUnit, exp: i32) -> Result<ResolvedUnit, EvalErr> {
    if a.is_affine {
        return Err(EvalErr::Diagnostic(limit_diag(
            "invalid_syntax",
            "offset_temperature_arithmetic",
            None,
        )));
    }
    let exp_i8 = i8::try_from(exp).map_err(|_| {
        EvalErr::Diagnostic(limit_diag(
            "numeric_overflow",
            "unit_exponent_out_of_range",
            None,
        ))
    })?;
    let dim = a.dimension.pow(exp_i8).ok_or_else(|| {
        EvalErr::Diagnostic(limit_diag(
            "numeric_overflow",
            "dimension_exponent_overflow",
            None,
        ))
    })?;
    let pi_power = a.pi_power.checked_mul(exp_i8).ok_or_else(|| {
        EvalErr::Diagnostic(limit_diag(
            "numeric_overflow",
            "dimension_exponent_overflow",
            None,
        ))
    })?;
    let scale = checked_pow_rational(&a.scale, exp).map_err(EvalErr::Diagnostic)?;
    let symbol = if exp == 1 {
        a.symbol
    } else {
        format!("{}^{exp}", a.symbol)
    };
    Ok(ResolvedUnit {
        id: format!("compound:{}^{exp}", a.id),
        symbol: symbol.clone(),
        description: symbol,
        dimension: dim,
        scale,
        offset: BigRational::zero(),
        is_affine: false,
        pi_power,
        system: a.system,
        counterpart: None,
        assumptions: a.assumptions,
    })
}

fn apply_standard_gravity(mass_qty: Quantity) -> Result<Quantity, EvalErr> {
    let gravity = BigRational::new(BigInt::from(980_665i64), BigInt::from(100_000i64));
    let force_n = mass_qty.value * mass_qty.unit.scale * gravity;
    let mut unit = ResolvedUnit::from_def(lookup_symbol("N").expect("N registered"));
    unit.assumptions = mass_qty.unit.assumptions;
    unit.assumptions
        .push("Standard gravity g = 9.80665 m/s² (exact)".into());
    Ok(Quantity {
        value: force_n,
        unit,
        has_unit: true,
    })
}

fn pi_approx_rational() -> BigRational {
    let num =
        BigInt::parse_bytes(b"31415926535897932384626433832795028841971693993751", 10).unwrap();
    BigRational::new(num, BigInt::from(10u8).pow(49))
}

fn convert_quantity(src: &Quantity, dst: &ResolvedUnit) -> Result<(BigRational, bool), Diagnostic> {
    if src.unit.dimension != dst.dimension {
        return Err(Diagnostic::error("dimension_mismatch"));
    }
    if src.unit.is_affine || dst.is_affine {
        let kelvin = (&src.value * &src.unit.scale) + &src.unit.offset;
        return Ok(((kelvin - &dst.offset) / &dst.scale, true));
    }
    let ratio = &src.unit.scale / &dst.scale;
    let pi_diff = src.unit.pi_power - dst.pi_power;
    if pi_diff == 0 {
        Ok((&src.value * ratio, true))
    } else {
        let pi_factor = checked_pow_rational(&pi_approx_rational(), pi_diff as i32)?;
        Ok((&src.value * ratio * pi_factor, false))
    }
}

// ---------------------------------------------------------------------------
// Auto-correction & UTF-16 patches
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct CorrectionMatch {
    original: String,
    corrected: String,
    unit_def: &'static UnitDef,
    distance: usize,
    start_utf16: usize,
    end_utf16: usize,
}

fn damerau_levenshtein_ci(a: &str, b: &str) -> usize {
    let (ab, bb): (Vec<u8>, Vec<u8>) = (
        a.bytes().map(|c| c.to_ascii_lowercase()).collect(),
        b.bytes().map(|c| c.to_ascii_lowercase()).collect(),
    );
    let (n, m) = (ab.len(), bb.len());
    let mut d = vec![vec![0usize; m + 1]; n + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=n {
        for j in 1..=m {
            let cost = usize::from(ab[i - 1] != bb[j - 1]);
            let mut best = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && ab[i - 1] == bb[j - 2] && ab[i - 2] == bb[j - 1] {
                best = best.min(d[i - 2][j - 2] + cost);
            }
            d[i][j] = best;
        }
    }
    d[n][m]
}

fn typo_eligible(alias: &str) -> bool {
    alias.len() >= 4
        && alias.bytes().all(|b| b.is_ascii_lowercase())
        && !alias.ends_with("ps")
        && !alias.starts_with("sq")
        && alias != "floz"
}

fn find_unit_correction(
    token: &str,
    start_utf16: usize,
    end_utf16: usize,
) -> Result<Option<CorrectionMatch>, Vec<CorrectionMatch>> {
    if token.len() < 4
        || !token.is_ascii()
        || !token.chars().all(|c| c.is_ascii_alphabetic())
        || ambiguous_data_choices(token).is_some()
    {
        return Ok(None);
    }
    let max_dist = if token.len() <= 5 { 1 } else { 2 };
    let mut best: BTreeMap<&'static str, (&'static UnitDef, &'static str, usize)> = BTreeMap::new();
    for unit in UNITS {
        for alias in unit
            .aliases
            .split_ascii_whitespace()
            .filter(|a| typo_eligible(a))
        {
            let dist = damerau_levenshtein_ci(token, alias);
            if (1..=max_dist).contains(&dist) && best.get(unit.id).is_none_or(|p| dist < p.2) {
                best.insert(unit.id, (unit, alias, dist));
            }
        }
    }
    if best.is_empty() {
        return Ok(None);
    }
    let mut ranked: Vec<_> = best.into_values().collect();
    ranked.sort_by(|a, b| a.2.cmp(&b.2).then_with(|| a.0.id.cmp(b.0.id)));
    let min_d = ranked[0].2;
    let to_cm = |(def, alias, distance)| CorrectionMatch {
        original: token.into(),
        corrected: String::from(alias),
        unit_def: def,
        distance,
        start_utf16,
        end_utf16,
    };
    let tied: Vec<_> = ranked
        .iter()
        .filter(|r| r.2 == min_d)
        .copied()
        .map(to_cm)
        .collect();
    if tied.len() == 1 && !(min_d == 2 && ranked.get(1).is_some_and(|r| r.2 <= 2)) {
        Ok(tied.into_iter().next())
    } else if tied.len() > 1 {
        Err(tied.into_iter().take(4).collect())
    } else {
        Err(ranked.into_iter().take(4).map(to_cm).collect())
    }
}

fn apply_utf16_patches(original: &str, corrections: &[CorrectionMatch]) -> String {
    let mut units: Vec<u16> = original.encode_utf16().collect();
    let mut sorted = corrections.to_vec();
    sorted.sort_by_key(|edit| std::cmp::Reverse(edit.start_utf16));
    for c in sorted {
        if c.start_utf16 <= c.end_utf16 && c.end_utf16 <= units.len() {
            units.splice(c.start_utf16..c.end_utf16, c.corrected.encode_utf16());
        }
    }
    String::from_utf16_lossy(&units)
}

// ---------------------------------------------------------------------------
// Conversion & expression pipelines
// ---------------------------------------------------------------------------

fn evaluate_conversion(
    tokens: &[Token],
    op_idx: usize,
    original: &str,
) -> Option<Result<Vec<Candidate>, Diagnostic>> {
    let (lhs, rhs) = (&tokens[..op_idx], &tokens[op_idx + 1..]);
    let orig_len_u16: usize = original.chars().map(char::len_utf16).sum();
    if lhs.is_empty() {
        return None;
    }
    if rhs.is_empty() {
        if let Ok(q) = parse_tokens(lhs, None, orig_len_u16, false)
            && q.has_unit
        {
            let mut d = Diagnostic::warning("incomplete_input");
            d.parameters =
                json!({"sourceDimension": q.unit.dimension.id(), "sourceUnitId": q.unit.id});
            return Some(Err(d));
        }
        return None;
    }

    let ambig = collect_ambiguity_candidates(lhs, rhs, orig_len_u16);
    if !ambig.is_empty() {
        return Some(Ok(ambig));
    }

    let mut overrides = OverrideMap::new();
    let mut corrections = Vec::new();
    let (src_q, dst_q) = loop {
        let l_res = parse_tokens(lhs, Some(&overrides), orig_len_u16, false);
        let r_res = parse_tokens(rhs, Some(&overrides), orig_len_u16, true);
        match (l_res, r_res) {
            (Ok(sq), Ok(dq)) => break (sq, dq),
            (
                Err(EvalErr::UnknownUnit {
                    token,
                    start_utf16,
                    end_utf16,
                }),
                other,
            ) => {
                if !handle_unknown_side(
                    &token,
                    start_utf16,
                    end_utf16,
                    other.is_ok(),
                    &mut overrides,
                    &mut corrections,
                ) {
                    return handle_unknown_err(
                        &token,
                        start_utf16,
                        end_utf16,
                        other.is_ok(),
                        corrections.len(),
                    )
                    .map(Err);
                }
            }
            (
                lhs_res,
                Err(EvalErr::UnknownUnit {
                    token,
                    start_utf16,
                    end_utf16,
                }),
            ) => {
                let other_ok = matches!(&lhs_res, Ok(q) if q.has_unit);
                if !handle_unknown_side(
                    &token,
                    start_utf16,
                    end_utf16,
                    other_ok,
                    &mut overrides,
                    &mut corrections,
                ) {
                    return handle_unknown_err(
                        &token,
                        start_utf16,
                        end_utf16,
                        other_ok,
                        corrections.len(),
                    )
                    .map(Err);
                }
            }
            (Err(EvalErr::Diagnostic(d)), _) => return has_any_known_unit(rhs).then_some(Err(d)),
            (_, Err(EvalErr::Diagnostic(d))) => return has_any_known_unit(lhs).then_some(Err(d)),
        }
    };

    if !src_q.has_unit {
        return None;
    }
    if src_q.unit.dimension != dst_q.unit.dimension {
        let span = json!({
            "startUtf16": rhs.first().map_or(0, |t| t.start_utf16),
            "endUtf16": rhs.last().map_or(orig_len_u16, |t| t.end_utf16),
        });
        let phrase = format!(
            "{} {}",
            format_rational_decimal(&src_q.value, true).display,
            src_q.unit.symbol
        );
        return Some(Err(dimension_mismatch_diagnostic(
            src_q.unit.dimension,
            dst_q.unit.dimension,
            &src_q.unit,
            &dst_q.unit,
            span,
            Some(&phrase),
            orig_len_u16,
        )));
    }

    let (converted, exact_conv) = match convert_quantity(&src_q, &dst_q.unit) {
        Ok(res) => res,
        Err(d) => return Some(Err(d)),
    };
    let mut diags = Vec::new();
    let id = if corrections.is_empty() {
        format!("unit:conv:{}:{}", src_q.unit.id, dst_q.unit.id)
    } else {
        diags.push(build_correction_warning(&corrections, original));
        format!("unit:conv:corrected:{}:{}", src_q.unit.id, dst_q.unit.id)
    };
    Some(Ok(vec![build_conversion_candidate(
        id,
        &src_q,
        &dst_q.unit,
        &converted,
        exact_conv,
        diags,
    )]))
}

fn handle_unknown_side(
    token: &str,
    s_u16: usize,
    e_u16: usize,
    other_ok: bool,
    overrides: &mut OverrideMap,
    corrections: &mut Vec<CorrectionMatch>,
) -> bool {
    if corrections.len() >= 2 || (!other_ok && token.len() < 6) {
        return false;
    }
    if let Ok(Some(cm)) = find_unit_correction(token, s_u16, e_u16) {
        overrides.insert(
            token.into(),
            (
                cm.unit_def,
                format!("Corrected '{}' → '{}'", cm.original, cm.corrected),
            ),
        );
        corrections.push(cm);
        true
    } else {
        false
    }
}

fn handle_unknown_err(
    token: &str,
    s_u16: usize,
    e_u16: usize,
    other_ok: bool,
    corr_count: usize,
) -> Option<Diagnostic> {
    if corr_count >= 2 {
        return Some(unknown_unit_diag(token, s_u16, e_u16, &[]));
    }
    match find_unit_correction(token, s_u16, e_u16) {
        Err(choices) => Some(unknown_unit_diag(token, s_u16, e_u16, &choices)),
        _ if other_ok => Some(unknown_unit_diag(token, s_u16, e_u16, &[])),
        _ => None,
    }
}

fn collect_ambiguity_candidates(
    lhs: &[Token],
    rhs: &[Token],
    orig_len_u16: usize,
) -> Vec<Candidate> {
    let find_amb = |toks: &[Token]| {
        toks.iter().find_map(|t| match &t.kind {
            TokKind::Ident(n) => ambiguous_data_choices(n).map(|c| (n.clone(), c)),
            _ => None,
        })
    };
    let l_amb = find_amb(lhs);
    let mut r_amb = find_amb(rhs);
    if l_amb.is_some()
        && r_amb.is_none()
        && rhs.len() == 1
        && matches!(&rhs[0].kind, TokKind::Ident(s) if s == "kb")
    {
        r_amb = Some(("kb".into(), KB_PAIRED));
    }
    if l_amb.is_none() && r_amb.is_none() {
        return Vec::new();
    }

    let mut plans: Vec<(OverrideMap, Vec<String>, bool)> = Vec::new();
    match (l_amb, r_amb) {
        (Some((lt, lc)), Some((rt, rc))) => {
            for &(lsym, la) in lc {
                for &(rsym, ra) in rc {
                    if let (Some(ld), Some(rd)) = (lookup_symbol(lsym), lookup_symbol(rsym)) {
                        let same = ld.system == rd.system
                            && (ld.id.contains("byte") == rd.id.contains("byte"));
                        if same || (ld.system == Si && rd.system == Si) {
                            let mut ov = OverrideMap::new();
                            ov.insert(lt.clone(), (ld, la.into()));
                            if lt != rt {
                                ov.insert(rt.clone(), (rd, ra.into()));
                            }
                            plans.push((ov, vec![la.into(), ra.into()], same));
                        }
                    }
                }
            }
            plans.sort_by_key(|p| !p.2);
        }
        (Some((tok, choices)), None) | (None, Some((tok, choices))) => {
            for &(sym, assump) in choices {
                if let Some(d) = lookup_symbol(sym) {
                    let mut ov = OverrideMap::new();
                    ov.insert(tok.clone(), (d, assump.into()));
                    plans.push((ov, vec![assump.into()], true));
                }
            }
        }
        (None, None) => {}
    }

    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for (ov, assumps, _) in plans {
        let (Ok(sq), Ok(dq)) = (
            parse_tokens(lhs, Some(&ov), orig_len_u16, false),
            parse_tokens(rhs, Some(&ov), orig_len_u16, true),
        ) else {
            continue;
        };
        if !sq.has_unit
            || sq.unit.dimension != dq.unit.dimension
            || !seen.insert((sq.unit.id.clone(), dq.unit.id.clone()))
        {
            continue;
        }
        if let Ok((val, exact)) = convert_quantity(&sq, &dq.unit) {
            let id = format!("unit:ambig:{}:{}", sq.unit.id, dq.unit.id);
            let mut c = build_conversion_candidate(id, &sq, &dq.unit, &val, exact, Vec::new());
            if c.assumptions.is_empty() {
                c.assumptions = assumps;
            }
            out.push(c);
            if out.len() >= MAX_CANDIDATES {
                break;
            }
        }
    }
    out
}

fn evaluate_unit_expression(
    tokens: &[Token],
    original: &str,
) -> Option<Result<Vec<Candidate>, Diagnostic>> {
    let has_op = tokens.iter().any(|t| {
        matches!(
            t.kind,
            TokKind::Plus | TokKind::Minus | TokKind::Star | TokKind::Slash | TokKind::Caret
        )
    });
    if !has_op || !has_any_known_unit(tokens) {
        return None;
    }
    let orig_len_u16: usize = original.chars().map(char::len_utf16).sum();
    let mut q = match parse_tokens(tokens, None, orig_len_u16, false) {
        Ok(q) => q,
        Err(EvalErr::Diagnostic(d)) => return Some(Err(d)),
        Err(EvalErr::UnknownUnit {
            token,
            start_utf16,
            end_utf16,
        }) => {
            return Some(Err(unknown_unit_diag(&token, start_utf16, end_utf16, &[])));
        }
    };
    if !q.has_unit {
        return None;
    }
    if q.unit.dimension == Dim::TIME && q.unit.id.starts_with("compound:") {
        let mut s_u = ResolvedUnit::from_def(lookup_symbol("s").expect("s registered"));
        q.value *= &q.unit.scale / &s_u.scale;
        s_u.assumptions = q.unit.assumptions;
        q.unit = s_u;
    } else if q.unit.dimension == Dim::ZERO {
        q.value *= &q.unit.scale;
        let mut d_u = ResolvedUnit::dimensionless();
        d_u.assumptions = q.unit.assumptions;
        q.unit = d_u;
    }

    let fmt = format_rational_decimal(&q.value, true);
    let display = if q.unit.symbol.is_empty() {
        fmt.display.clone()
    } else {
        format!("{} {}", fmt.display, q.unit.symbol)
    };
    let val_json = build_quantity_value_json(&fmt.full, &q.value, &q.unit, &display, fmt.exact);
    let clean: String = tokens
        .iter()
        .enumerate()
        .fold(String::new(), |mut acc, (i, t)| {
            if i > 0
                && !matches!(tokens[i - 1].kind, TokKind::LParen | TokKind::Caret)
                && !matches!(t.kind, TokKind::RParen | TokKind::Caret)
            {
                acc.push(' ');
            }
            acc.push_str(&t.text);
            acc
        });
    let normalized = format!("{clean} = {display}");
    let interp = if q.unit.description.is_empty() {
        normalized.clone()
    } else {
        format!("{display} ({})", q.unit.description)
    };
    let mut cand = Candidate::new(
        format!("unit:expr:{}", q.unit.id),
        "unit_conversion",
        &normalized,
        &interp,
        val_json,
    );
    cand.assumptions = q.unit.assumptions.clone();
    if matches!(q.unit.dimension, Dim::DATA | Dim::RATE) {
        cand.assumptions.push("1 byte = 8 bits".into());
    }
    cand.formats = build_formats(&q.value, &q.unit, &display, fmt.exact);
    cand.provenance = Some(json!({"kind": "built_in_unit_catalog", "version": "1"}));
    Some(Ok(vec![cand]))
}

// ---------------------------------------------------------------------------
// Candidate, format & diagnostic builders
// ---------------------------------------------------------------------------

fn build_conversion_candidate(
    id: String,
    src: &Quantity,
    dst: &ResolvedUnit,
    converted: &BigRational,
    exact_conv: bool,
    diagnostics: Vec<Diagnostic>,
) -> Candidate {
    let s_fmt = format_rational_decimal(&src.value, true);
    let d_fmt = format_rational_decimal(converted, exact_conv);
    let display = format!("{} {}", d_fmt.display, dst.symbol);
    let normalized = format!("{} {} → {}", s_fmt.display, src.unit.symbol, dst.symbol);
    let interp = format!(
        "{} {} → {}",
        s_fmt.display, src.unit.description, dst.description
    );
    let value = build_quantity_value_json(&d_fmt.full, converted, dst, &display, d_fmt.exact);

    let mut assumptions = Vec::new();
    for a in src.unit.assumptions.iter().chain(&dst.assumptions) {
        if !assumptions.contains(a) {
            assumptions.push(a.clone());
        }
    }
    let is_data = matches!(dst.dimension, Dim::DATA | Dim::RATE);
    if is_data && dst.system == Si && !assumptions.iter().any(|a| a.contains("SI")) {
        assumptions.push("SI decimal prefixes".into());
    }
    if is_data && dst.system == Iec && !assumptions.iter().any(|a| a.contains("IEC")) {
        assumptions.push("IEC binary prefixes".into());
    }
    if is_data
        && (src.unit.id.contains("byte") != dst.id.contains("byte")
            || matches!(src.unit.id.as_str(), "data_byte" | "data_bit")
            || matches!(dst.id.as_str(), "data_byte" | "data_bit")
            || dst.dimension == Dim::RATE)
        && !assumptions.iter().any(|a| a.contains("1 byte = 8 bits"))
    {
        assumptions.push("1 byte = 8 bits".into());
    }

    let mut cand = Candidate::new(id, "unit_conversion", &normalized, &interp, value);
    cand.assumptions = assumptions;
    cand.formats = build_formats(converted, dst, &display, d_fmt.exact);
    cand.diagnostics = diagnostics;
    cand.provenance = Some(json!({"kind": "built_in_unit_catalog", "version": "1"}));
    cand
}

fn build_quantity_value_json(
    full: &str,
    rat: &BigRational,
    unit: &ResolvedUnit,
    display: &str,
    exact: bool,
) -> Value {
    json!({
        "tag": "quantity", "decimal": full,
        "rational": {"numerator": rat.numer().to_string(), "denominator": rat.denom().to_string()},
        "unitId": unit.id, "unitSymbol": unit.symbol, "dimension": unit.dimension.to_json(),
        "display": display, "exact": exact,
    })
}

fn build_formats(
    val: &BigRational,
    unit: &ResolvedUnit,
    primary: &str,
    exact: bool,
) -> Vec<Format> {
    let sys_id = |s| match s {
        Si => "si",
        Iec => "iec",
        _ => "canonical",
    };
    let mut formats = vec![Format::new(sys_id(unit.system), primary, primary)];

    if let Some(ref c_sym) = unit.counterpart
        && let Ok(toks) = raw_tokenize(c_sym)
        && let Ok(alt_q) = parse_tokens(&toks, None, 0, true)
    {
        let dummy = Quantity {
            value: val.clone(),
            unit: unit.clone(),
            has_unit: true,
        };
        if let Ok((alt_val, alt_exact)) = convert_quantity(&dummy, &alt_q.unit) {
            let af = format_rational_decimal(&alt_val, alt_exact);
            let lbl = format!("{} {}", af.display, alt_q.unit.symbol);
            let aid = match alt_q.unit.system {
                Iec => "iec",
                Si => "si",
                _ => "alternate",
            };
            formats.push(Format::new(aid, &lbl, &lbl));
        }
    }
    if matches!(unit.id.as_str(), "data_bit" | "data_byte") {
        let dec = format_rational_decimal(val, exact).display;
        let base = if unit.id == "data_bit" { "bit" } else { "byte" };
        let v = format!("{dec} {base}{}", if dec == "1" { "" } else { "s" });
        formats.push(Format::new("verbose", &v, &v));
    }
    if unit.dimension == Dim::TIME
        && let Some(hms) = format_duration_hms(&(val * &unit.scale))
    {
        formats.push(Format::new("duration", &hms, &hms));
    }
    if !exact && !unit.symbol.is_empty() {
        let r = if val.denom().is_one() {
            format!("{} {}", val.numer(), unit.symbol)
        } else {
            format!("{}/{} {}", val.numer(), val.denom(), unit.symbol)
        };
        formats.push(Format::new("rational", &r, &r));
    }
    formats
}

fn build_correction_warning(corrections: &[CorrectionMatch], original: &str) -> Diagnostic {
    let mut diag = Diagnostic::warning("unit_corrected");
    let (first, last) = (&corrections[0], &corrections[corrections.len() - 1]);
    diag.span = Some(json!({"startUtf16": first.start_utf16, "endUtf16": last.end_utf16}));
    let entries: Vec<Value> = corrections.iter().map(|c| json!({
        "original": c.original, "corrected": c.corrected, "canonicalUnitId": c.unit_def.id,
        "editDistance": c.distance, "span": {"startUtf16": c.start_utf16, "endUtf16": c.end_utf16},
    })).collect();
    let patched = apply_utf16_patches(original, corrections);
    diag.parameters = json!({
        "originalToken": first.original, "correctedToken": first.corrected,
        "canonicalUnitId": first.unit_def.id, "editDistance": first.distance,
        "correctedQuery": patched, "corrections": entries,
    });
    let orig_len: usize = original.chars().map(char::len_utf16).sum();
    let repl = if corrections.len() == 1 {
        json!({"startUtf16": first.start_utf16, "endUtf16": first.end_utf16, "text": first.corrected})
    } else {
        json!({"startUtf16": 0, "endUtf16": orig_len, "text": patched})
    };
    diag.suggestions = vec![json!({"id": "s1", "label": patched, "replacement": repl})];
    diag
}

fn dimension_mismatch_diagnostic(
    src_dim: Dim,
    dst_dim: Dim,
    src_u: &ResolvedUnit,
    dst_u: &ResolvedUnit,
    span: Value,
    src_phrase: Option<&str>,
    orig_len_u16: usize,
) -> Diagnostic {
    let mut diag = Diagnostic::error("dimension_mismatch");
    diag.parameters = json!({
        "sourceDimension": src_dim.id(), "targetDimension": dst_dim.id(),
        "sourceUnitId": src_u.id, "targetUnitId": dst_u.id,
    });
    diag.span = Some(span);
    if let Some(phrase) = src_phrase {
        let pool: &[&str] = match src_dim {
            Dim::MASS => &["lb", "g", "kg", "oz"],
            Dim::LEN => &["ft", "m", "km", "cm", "mi"],
            Dim::TIME => &["min", "h", "s", "ms"],
            Dim::DATA => &["MiB", "MB", "kB", "KiB", "b"],
            Dim::RATE => &["MB/s", "MiB/s", "Mbps", "Gbps"],
            Dim::TEMP => &["°C", "°F", "K"],
            Dim::SPEED => &["km/h", "mph", "m/s"],
            Dim::AREA => &["m^2", "ft^2", "km^2"],
            Dim::VOL => &["L", "mL", "gal"],
            Dim::FORCE => &["N", "kN", "lbf"],
            Dim::ENERGY => &["J", "kJ", "kWh", "kcal"],
            Dim::POWER => &["W", "kW", "hp"],
            Dim::PRESS => &["Pa", "kPa", "bar", "psi"],
            _ => &[],
        };
        let push_s = |diag: &mut Diagnostic, text: String| {
            let id = format!("s{}", diag.suggestions.len() + 1);
            diag.suggestions.push(json!({"id": id, "label": text, "replacement": {"startUtf16": 0, "endUtf16": orig_len_u16, "text": text}}));
        };
        for (i, &target) in pool
            .iter()
            .filter(|&&s| s != src_u.symbol)
            .take(3)
            .enumerate()
        {
            if diag.suggestions.len() >= 3.min(MAX_SUGGESTIONS) {
                break;
            }
            push_s(&mut diag, format!("{phrase} in {target}"));
            if i == 0 && src_dim == Dim::MASS && dst_dim == Dim::LEN {
                push_s(&mut diag, format!("{phrase} × g"));
            }
        }
    }
    diag
}

fn unknown_unit_diag(
    token: &str,
    start_utf16: usize,
    end_utf16: usize,
    choices: &[CorrectionMatch],
) -> Diagnostic {
    let mut diag = Diagnostic::error("unknown_identifier");
    diag.parameters = json!({"identifier": token});
    diag.span = Some(json!({"startUtf16": start_utf16, "endUtf16": end_utf16}));
    diag.suggestions = choices.iter().enumerate().take(MAX_SUGGESTIONS).map(|(i, c)| json!({
        "id": format!("s{}", i + 1), "label": c.corrected, "canonicalUnitId": c.unit_def.id,
        "editDistance": c.distance, "replacement": {"startUtf16": c.start_utf16, "endUtf16": c.end_utf16, "text": c.corrected},
    })).collect();
    diag
}

fn limit_diag(code: &str, reason: &str, span: Option<Value>) -> Diagnostic {
    let mut diag = Diagnostic::error(code);
    diag.parameters = json!({"reason": reason});
    diag.span = span;
    diag
}

// ---------------------------------------------------------------------------
// Unit tests (P2 U01–U10, C03, §05, §09, security & UTF-16 edges)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn expect_ok(query: &str, original: &str) -> Vec<Candidate> {
        preview(query, original)
            .expect("expected Some(_)")
            .expect("expected Ok(_)")
    }
    fn expect_err(query: &str, original: &str) -> Diagnostic {
        preview(query, original)
            .expect("expected Some(_)")
            .expect_err("expected Err(_)")
    }

    #[test]
    fn u01_u02_100_mbps_to_mb_per_s() {
        let cands = expect_ok("100 Mbps to MB/s", "100 Mbps to MB/s");
        assert_eq!(cands.len(), 1);
        let c = &cands[0];
        assert_eq!(c.kind, "unit_conversion");
        assert_eq!(c.value["tag"], "quantity");
        assert_eq!(c.value["decimal"], "12.5");
        assert_eq!(c.value["unitId"], "byte_per_second_megabyte_si");
        assert_eq!(c.value["display"], "12.5 MB/s");
        assert_eq!(c.value["exact"], true);
        assert_eq!(c.value["dimension"]["id"], "data_rate");
        let fmts = serde_json::to_value(&c.formats).unwrap();
        let arr = fmts.as_array().unwrap();
        assert_eq!(
            (arr.len(), &arr[0]["id"], &arr[0]["label"]),
            (2, &json!("si"), &json!("12.5 MB/s"))
        );
        assert_eq!(
            (&arr[1]["id"], &arr[1]["label"]),
            (&json!("iec"), &json!("11.920928955 MiB/s"))
        );
    }

    #[test]
    fn u03_1_gbps_to_mb_per_s() {
        let cands = expect_ok("1 Gbps to MB/s", "1 Gbps to MB/s");
        assert_eq!(cands.len(), 1);
        assert_eq!(
            (
                &cands[0].value["decimal"],
                &cands[0].value["display"],
                &cands[0].value["exact"]
            ),
            (&json!("125"), &json!("125 MB/s"), &json!(true))
        );
    }

    #[test]
    fn u04_4_7_gb_div_100_mbps() {
        let cands = expect_ok("4.7 GB / 100 Mbps", "4.7 GB / 100 Mbps");
        assert_eq!(cands.len(), 1);
        let c = &cands[0];
        assert_eq!(
            (
                &c.value["decimal"],
                &c.value["unitId"],
                &c.value["display"],
                &c.value["exact"]
            ),
            (
                &json!("376"),
                &json!("time_second"),
                &json!("376 s"),
                &json!(true)
            )
        );
        assert_eq!(c.value["dimension"]["id"], "time");
        let fmts = serde_json::to_value(&c.formats).unwrap();
        let labels: Vec<&str> = fmts
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|f| f["label"].as_str())
            .collect();
        assert!(labels.contains(&"376 s") && labels.contains(&"6 min 16 s"));
    }

    #[test]
    fn u05_5_kg_in_m_dimension_mismatch() {
        let diag = expect_err("5 kg in m", "5 kg in m");
        assert_eq!(
            serde_json::to_value(&diag).unwrap()["code"],
            "dimension_mismatch"
        );
        assert_eq!(
            (
                &diag.parameters["sourceDimension"],
                &diag.parameters["targetDimension"]
            ),
            (&json!("mass"), &json!("length"))
        );
        assert_eq!(diag.span, Some(json!({"startUtf16": 8, "endUtf16": 9})));
        let labels: Vec<&str> = diag
            .suggestions
            .iter()
            .filter_map(|s| s["label"].as_str())
            .collect();
        assert_eq!(labels, vec!["5 kg in lb", "5 kg × g", "5 kg in g"]);
    }

    #[test]
    fn u06_5_kg_in_lb() {
        let cands = expect_ok("5 kg in lb", "5 kg in lb");
        assert_eq!(cands.len(), 1);
        let c = &cands[0];
        assert_eq!(
            (&c.value["unitId"], &c.value["display"], &c.value["exact"]),
            (
                &json!("mass_pound"),
                &json!("11.023113109 lb"),
                &json!(false)
            )
        );
        assert_eq!(
            (
                &c.value["rational"]["numerator"],
                &c.value["rational"]["denominator"]
            ),
            (&json!("500000000"), &json!("45359237"))
        );
    }

    #[test]
    fn u07_2_mb_in_kb_ambiguous() {
        let cands = expect_ok("2 mb in kb", "2 mb in kb");
        assert!(cands.len() >= 3);
        assert_eq!(
            (
                &cands[0].value["decimal"],
                &cands[0].value["unitId"],
                &cands[0].value["display"]
            ),
            (
                &json!("2000"),
                &json!("data_kilobyte_si"),
                &json!("2000 kB")
            )
        );
        let uids: Vec<&str> = cands
            .iter()
            .filter_map(|c| c.value["unitId"].as_str())
            .collect();
        assert!(
            uids.contains(&"data_kilobyte_si")
                && uids.contains(&"data_kilobit_si")
                && uids.contains(&"data_kibibyte_iec")
        );
        assert_eq!(
            cands
                .iter()
                .map(|c| c.id.as_str())
                .collect::<HashSet<_>>()
                .len(),
            cands.len()
        );
    }

    #[test]
    fn u08_u09_u10_iec_si_bit_byte() {
        let u8c = expect_ok("1 MiB in KiB", "1 MiB in KiB");
        assert_eq!(
            (
                &u8c[0].value["decimal"],
                &u8c[0].value["unitId"],
                &u8c[0].value["display"],
                &u8c[0].value["exact"]
            ),
            (
                &json!("1024"),
                &json!("data_kibibyte_iec"),
                &json!("1024 KiB"),
                &json!(true)
            )
        );
        let u9c = expect_ok("1 MB in kB", "1 MB in kB");
        assert_eq!(
            (
                &u9c[0].value["decimal"],
                &u9c[0].value["unitId"],
                &u9c[0].value["display"],
                &u9c[0].value["exact"]
            ),
            (
                &json!("1000"),
                &json!("data_kilobyte_si"),
                &json!("1000 kB"),
                &json!(true)
            )
        );
        let u10c = expect_ok("1 B in b", "1 B in b");
        assert_eq!(
            (
                &u10c[0].value["decimal"],
                &u10c[0].value["unitId"],
                &u10c[0].value["display"],
                &u10c[0].value["exact"]
            ),
            (&json!("8"), &json!("data_bit"), &json!("8 b"), &json!(true))
        );
    }

    #[test]
    fn c03_typo_correction_and_raw_utf16_spans() {
        let cands = expect_ok("5 kilomters in miles", "5 kilomters in miles");
        assert_eq!(cands.len(), 1);
        let c = &cands[0];
        assert_eq!(
            (&c.value["unitId"], &c.value["display"], c.diagnostics.len()),
            (&json!("length_mile"), &json!("3.106855961 mi"), 1)
        );
        let d = &c.diagnostics[0];
        assert_eq!(serde_json::to_value(d).unwrap()["severity"], "warning");
        assert_eq!(
            (
                &d.parameters["originalToken"],
                &d.parameters["correctedToken"],
                &d.parameters["canonicalUnitId"]
            ),
            (
                &json!("kilomters"),
                &json!("kilometers"),
                &json!("length_kilometre")
            )
        );
        assert_eq!(d.span, Some(json!({"startUtf16": 2, "endUtf16": 11})));
        assert_eq!(
            d.suggestions[0]["replacement"],
            json!({"startUtf16": 2, "endUtf16": 11, "text": "kilometers"})
        );

        let cands_u16 = expect_ok("🚀 5 kilomters in miles", "🚀  ５  kilomters  in  miles");
        assert_eq!(
            cands_u16[0].diagnostics[0].suggestions[0]["replacement"],
            json!({"startUtf16": 7, "endUtf16": 16, "text": "kilometers"})
        );
    }

    #[test]
    fn non_unique_unit_correction_rejected() {
        let diag = expect_err("5 grak in kg", "5 grak in kg");
        assert_eq!(
            serde_json::to_value(&diag).unwrap()["code"],
            "unknown_identifier"
        );
        assert!(diag.suggestions.len() >= 2);
    }

    #[test]
    fn gravity_weight_and_affine_temperature() {
        let g = expect_ok("5 kg * g", "5 kg × g");
        assert_eq!(
            (
                &g[0].value["decimal"],
                &g[0].value["unitId"],
                &g[0].value["display"],
                &g[0].value["exact"]
            ),
            (
                &json!("49.03325"),
                &json!("force_newton"),
                &json!("49.03325 N"),
                &json!(true)
            )
        );
        let t = expect_ok("100 °C in °F", "100 °C in °F");
        assert_eq!(
            (&t[0].value["decimal"], &t[0].value["exact"]),
            (&json!("212"), &json!(true))
        );
        assert_eq!(
            expect_ok("-40 °C in °F", "-40 °C in °F")[0].value["decimal"],
            "-40"
        );
    }

    #[test]
    fn unbounded_exponents_and_non_unit_queries() {
        for q in [
            "10^100000000 m in km",
            "5 m^999999 in m",
            "1e1000000 m in km",
        ] {
            assert_eq!(
                serde_json::to_value(expect_err(q, q)).unwrap()["code"],
                "numeric_overflow"
            );
        }
        for q in [
            "1+1",
            "3pi sqrt 2",
            "1 kilo",
            "speed of light",
            "wallpaper-1920x1080.png",
            "3pm PST in beijing",
            "¥2999 in usd",
            "0xDEAD_BEEF >> 4 & 0xFFFF",
            "0.1 as bf16",
        ] {
            assert!(preview(q, q).is_none(), "expected None for {q}");
        }
    }
}
