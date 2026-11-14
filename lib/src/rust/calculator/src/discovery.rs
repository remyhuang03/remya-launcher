use serde_json::{Value, json};
use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CatalogKind {
    Constant,
    Function,
    Unit,
}

impl CatalogKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Constant => "constant",
            Self::Function => "function",
            Self::Unit => "unit",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ScoreTier {
    ExactSymbol = 0,
    ExactName = 1,
    ExactAlias = 2,
    Prefix = 3,
    WordStart = 4,
    Fuzzy = 5,
    Description = 6,
}

impl ScoreTier {
    const fn as_str(self) -> &'static str {
        match self {
            Self::ExactSymbol => "exact",
            Self::ExactName => "exact",
            Self::ExactAlias => "exact_alias",
            Self::Prefix => "prefix",
            Self::WordStart => "word_start",
            Self::Fuzzy => "fuzzy",
            Self::Description => "description",
        }
    }
}

struct CatalogEntry {
    stable_id: &'static str,
    kind: CatalogKind,
    canonical_symbol: &'static str,
    english_name: &'static str,
    aliases: &'static [&'static str],
    description_key: &'static str,
    description: &'static str,
    dimension_id: Option<&'static str>,
    exact_definition: Option<&'static str>,
    is_exact: bool,
    example_queries: &'static [&'static str],
    fend_compatible: bool,
    introduced_in_catalog_version: u32,
}

const CATALOG: &[CatalogEntry] = &[
    // --- Constants ---
    CatalogEntry {
        stable_id: "constant.speed_of_light",
        kind: CatalogKind::Constant,
        canonical_symbol: "c",
        english_name: "speed of light",
        aliases: &["speed of light", "speed_of_light", "light speed", "c_0"],
        description_key: "calc.catalog.constant.speed_of_light",
        description: "Speed of light in vacuum, defined exactly as 299792458 m/s in SI",
        dimension_id: Some("speed"),
        exact_definition: Some("299792458 m/s"),
        is_exact: true,
        example_queries: &["c", "c in km/s", "c * 1 s"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "constant.pi",
        kind: CatalogKind::Constant,
        canonical_symbol: "pi",
        english_name: "pi",
        aliases: &["π", "archimedes constant"],
        description_key: "calc.catalog.constant.pi",
        description: "Ratio of a circle's circumference to its diameter",
        dimension_id: None,
        exact_definition: None,
        is_exact: false,
        example_queries: &["pi", "3pi sqrt 2", "sin(pi / 2)"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "constant.e",
        kind: CatalogKind::Constant,
        canonical_symbol: "e",
        english_name: "Euler's number",
        aliases: &["euler", "eulers number", "napier constant"],
        description_key: "calc.catalog.constant.e",
        description: "Base of the natural logarithm",
        dimension_id: None,
        exact_definition: None,
        is_exact: false,
        example_queries: &["e", "ln(e)", "e^2"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    // --- Functions ---
    CatalogEntry {
        stable_id: "function.sqrt",
        kind: CatalogKind::Function,
        canonical_symbol: "sqrt",
        english_name: "square root",
        aliases: &["√", "square root", "sq"],
        description_key: "calc.catalog.function.sqrt",
        description: "Principal square root of a number",
        dimension_id: None,
        exact_definition: None,
        is_exact: false,
        example_queries: &["sqrt 2", "sqrt(144)", "3pi sqrt 2"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "function.cbrt",
        kind: CatalogKind::Function,
        canonical_symbol: "cbrt",
        english_name: "cube root",
        aliases: &["∛", "cube root"],
        description_key: "calc.catalog.function.cbrt",
        description: "Principal cube root of a number",
        dimension_id: None,
        exact_definition: None,
        is_exact: false,
        example_queries: &["cbrt 27", "cbrt(8)"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "function.abs",
        kind: CatalogKind::Function,
        canonical_symbol: "abs",
        english_name: "absolute value",
        aliases: &["absolute value", "magnitude"],
        description_key: "calc.catalog.function.abs",
        description: "Non-negative magnitude of a number",
        dimension_id: None,
        exact_definition: None,
        is_exact: false,
        example_queries: &["abs(-42)", "abs 3.5"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "function.sin",
        kind: CatalogKind::Function,
        canonical_symbol: "sin",
        english_name: "sine",
        aliases: &["sine"],
        description_key: "calc.catalog.function.sin",
        description: "Trigonometric sine of an angle in radians",
        dimension_id: None,
        exact_definition: None,
        is_exact: false,
        example_queries: &["sin(pi / 6)", "sin(30 deg)"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "function.cos",
        kind: CatalogKind::Function,
        canonical_symbol: "cos",
        english_name: "cosine",
        aliases: &["cosine"],
        description_key: "calc.catalog.function.cos",
        description: "Trigonometric cosine of an angle in radians",
        dimension_id: None,
        exact_definition: None,
        is_exact: false,
        example_queries: &["cos(0)", "cos(60 deg)"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "function.tan",
        kind: CatalogKind::Function,
        canonical_symbol: "tan",
        english_name: "tangent",
        aliases: &["tangent"],
        description_key: "calc.catalog.function.tan",
        description: "Trigonometric tangent of an angle in radians",
        dimension_id: None,
        exact_definition: None,
        is_exact: false,
        example_queries: &["tan(pi / 4)"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "function.asin",
        kind: CatalogKind::Function,
        canonical_symbol: "asin",
        english_name: "arcsine",
        aliases: &["arcsin", "inverse sine"],
        description_key: "calc.catalog.function.asin",
        description: "Inverse trigonometric sine",
        dimension_id: None,
        exact_definition: None,
        is_exact: false,
        example_queries: &["asin(0.5)"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "function.acos",
        kind: CatalogKind::Function,
        canonical_symbol: "acos",
        english_name: "arccosine",
        aliases: &["arccos", "inverse cosine"],
        description_key: "calc.catalog.function.acos",
        description: "Inverse trigonometric cosine",
        dimension_id: None,
        exact_definition: None,
        is_exact: false,
        example_queries: &["acos(0.5)"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "function.atan",
        kind: CatalogKind::Function,
        canonical_symbol: "atan",
        english_name: "arctangent",
        aliases: &["arctan", "inverse tangent"],
        description_key: "calc.catalog.function.atan",
        description: "Inverse trigonometric tangent",
        dimension_id: None,
        exact_definition: None,
        is_exact: false,
        example_queries: &["atan(1)"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "function.ln",
        kind: CatalogKind::Function,
        canonical_symbol: "ln",
        english_name: "natural logarithm",
        aliases: &["natural log", "logarithm base e"],
        description_key: "calc.catalog.function.ln",
        description: "Natural logarithm (base e)",
        dimension_id: None,
        exact_definition: None,
        is_exact: false,
        example_queries: &["ln(e)", "ln(10)"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "function.log10",
        kind: CatalogKind::Function,
        canonical_symbol: "log10",
        english_name: "common logarithm",
        aliases: &["log base 10", "decimal logarithm"],
        description_key: "calc.catalog.function.log10",
        description: "Base-10 logarithm",
        dimension_id: None,
        exact_definition: None,
        is_exact: false,
        example_queries: &["log10(1000)"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "function.log2",
        kind: CatalogKind::Function,
        canonical_symbol: "log2",
        english_name: "binary logarithm",
        aliases: &["log base 2"],
        description_key: "calc.catalog.function.log2",
        description: "Base-2 logarithm",
        dimension_id: None,
        exact_definition: None,
        is_exact: false,
        example_queries: &["log2(1024)"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "function.exp",
        kind: CatalogKind::Function,
        canonical_symbol: "exp",
        english_name: "exponential",
        aliases: &["e to the power"],
        description_key: "calc.catalog.function.exp",
        description: "Exponential function e^x",
        dimension_id: None,
        exact_definition: None,
        is_exact: false,
        example_queries: &["exp(1)"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "function.floor",
        kind: CatalogKind::Function,
        canonical_symbol: "floor",
        english_name: "floor",
        aliases: &["round down"],
        description_key: "calc.catalog.function.floor",
        description: "Largest integer less than or equal to x",
        dimension_id: None,
        exact_definition: None,
        is_exact: false,
        example_queries: &["floor(3.7)"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "function.ceil",
        kind: CatalogKind::Function,
        canonical_symbol: "ceil",
        english_name: "ceiling",
        aliases: &["ceiling", "round up"],
        description_key: "calc.catalog.function.ceil",
        description: "Smallest integer greater than or equal to x",
        dimension_id: None,
        exact_definition: None,
        is_exact: false,
        example_queries: &["ceil(3.2)"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "function.round",
        kind: CatalogKind::Function,
        canonical_symbol: "round",
        english_name: "round",
        aliases: &["nearest integer"],
        description_key: "calc.catalog.function.round",
        description: "Round to the nearest integer",
        dimension_id: None,
        exact_definition: None,
        is_exact: false,
        example_queries: &["round(3.6)"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    // --- Mass Units ---
    CatalogEntry {
        stable_id: "unit.kilogram",
        kind: CatalogKind::Unit,
        canonical_symbol: "kg",
        english_name: "kilogram",
        aliases: &["kilograms", "kilo"],
        description_key: "calc.catalog.unit.kilogram",
        description: "SI base unit of mass",
        dimension_id: Some("mass"),
        exact_definition: Some("1000 g"),
        is_exact: true,
        example_queries: &["5 kg in lb", "1 kilogram in g"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "unit.gram",
        kind: CatalogKind::Unit,
        canonical_symbol: "g",
        english_name: "gram",
        aliases: &["grams"],
        description_key: "calc.catalog.unit.gram",
        description: "Metric unit of mass (0.001 kg)",
        dimension_id: Some("mass"),
        exact_definition: Some("0.001 kg"),
        is_exact: true,
        example_queries: &["500 g in kg"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "unit.milligram",
        kind: CatalogKind::Unit,
        canonical_symbol: "mg",
        english_name: "milligram",
        aliases: &["milligrams"],
        description_key: "calc.catalog.unit.milligram",
        description: "One thousandth of a gram",
        dimension_id: Some("mass"),
        exact_definition: Some("0.001 g"),
        is_exact: true,
        example_queries: &["250 mg in g"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "unit.pound",
        kind: CatalogKind::Unit,
        canonical_symbol: "lb",
        english_name: "pound",
        aliases: &["pounds", "lbs"],
        description_key: "calc.catalog.unit.pound",
        description: "Imperial/avoirdupois unit of mass",
        dimension_id: Some("mass"),
        exact_definition: Some("0.45359237 kg"),
        is_exact: true,
        example_queries: &["5 kg in lb"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "unit.ounce",
        kind: CatalogKind::Unit,
        canonical_symbol: "oz",
        english_name: "ounce",
        aliases: &["ounces"],
        description_key: "calc.catalog.unit.ounce",
        description: "Imperial unit of mass (1/16 pound)",
        dimension_id: Some("mass"),
        exact_definition: Some("28.349523125 g"),
        is_exact: true,
        example_queries: &["16 oz in lb"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    // --- Length Units ---
    CatalogEntry {
        stable_id: "unit.kilometre",
        kind: CatalogKind::Unit,
        canonical_symbol: "km",
        english_name: "kilometre",
        aliases: &["kilometres", "kilometer", "kilometers"],
        description_key: "calc.catalog.unit.kilometre",
        description: "Metric unit of length (1000 metres)",
        dimension_id: Some("length"),
        exact_definition: Some("1000 m"),
        is_exact: true,
        example_queries: &["5 km in miles"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "unit.metre",
        kind: CatalogKind::Unit,
        canonical_symbol: "m",
        english_name: "metre",
        aliases: &["metres", "meter", "meters"],
        description_key: "calc.catalog.unit.metre",
        description: "SI base unit of length",
        dimension_id: Some("length"),
        exact_definition: Some("distance travelled by light in vacuum in 1/299792458 s"),
        is_exact: true,
        example_queries: &["100 m in ft"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "unit.centimetre",
        kind: CatalogKind::Unit,
        canonical_symbol: "cm",
        english_name: "centimetre",
        aliases: &["centimetres", "centimeter", "centimeters"],
        description_key: "calc.catalog.unit.centimetre",
        description: "One hundredth of a metre",
        dimension_id: Some("length"),
        exact_definition: Some("0.01 m"),
        is_exact: true,
        example_queries: &["100 cm in m"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "unit.millimetre",
        kind: CatalogKind::Unit,
        canonical_symbol: "mm",
        english_name: "millimetre",
        aliases: &["millimetres", "millimeter", "millimeters"],
        description_key: "calc.catalog.unit.millimetre",
        description: "One thousandth of a metre",
        dimension_id: Some("length"),
        exact_definition: Some("0.001 m"),
        is_exact: true,
        example_queries: &["25.4 mm in inch"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "unit.mile",
        kind: CatalogKind::Unit,
        canonical_symbol: "mi",
        english_name: "mile",
        aliases: &["miles"],
        description_key: "calc.catalog.unit.mile",
        description: "International mile (1609.344 metres)",
        dimension_id: Some("length"),
        exact_definition: Some("1609.344 m"),
        is_exact: true,
        example_queries: &["5 km in miles"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "unit.foot",
        kind: CatalogKind::Unit,
        canonical_symbol: "ft",
        english_name: "foot",
        aliases: &["feet"],
        description_key: "calc.catalog.unit.foot",
        description: "Imperial/US customary unit of length (0.3048 m)",
        dimension_id: Some("length"),
        exact_definition: Some("0.3048 m"),
        is_exact: true,
        example_queries: &["6 ft in cm"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "unit.inch",
        kind: CatalogKind::Unit,
        canonical_symbol: "in",
        english_name: "inch",
        aliases: &["inches"],
        description_key: "calc.catalog.unit.inch",
        description: "Imperial/US customary unit of length (25.4 mm)",
        dimension_id: Some("length"),
        exact_definition: Some("0.0254 m"),
        is_exact: true,
        example_queries: &["12 inches in cm"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "unit.light_year",
        kind: CatalogKind::Unit,
        canonical_symbol: "ly",
        english_name: "light-year",
        aliases: &["light year", "lightyear", "light-years"],
        description_key: "calc.catalog.unit.light_year",
        description: "Distance that light travels in vacuum in one Julian year",
        dimension_id: Some("length"),
        exact_definition: Some("9460730472580800 m"),
        is_exact: true,
        example_queries: &["1 lightyear in km"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "unit.light_second",
        kind: CatalogKind::Unit,
        canonical_symbol: "ls",
        english_name: "light-second",
        aliases: &["light second", "lightsecond", "light-seconds"],
        description_key: "calc.catalog.unit.light_second",
        description: "Distance that light travels in vacuum in one second",
        dimension_id: Some("length"),
        exact_definition: Some("299792458 m"),
        is_exact: true,
        example_queries: &["1 light-second in km"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    // --- Luminous Flux ---
    CatalogEntry {
        stable_id: "unit.lumen",
        kind: CatalogKind::Unit,
        canonical_symbol: "lm",
        english_name: "lumen",
        aliases: &["lumens"],
        description_key: "calc.catalog.unit.lumen",
        description: "SI derived unit of luminous flux (total visible light emitted)",
        dimension_id: Some("luminous_flux"),
        exact_definition: Some("1 cd * sr"),
        is_exact: true,
        example_queries: &["800 lm"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    // --- Information / Data Units (case-distinct) ---
    CatalogEntry {
        stable_id: "unit.bit",
        kind: CatalogKind::Unit,
        canonical_symbol: "bit",
        english_name: "bit",
        aliases: &["bits", "b"],
        description_key: "calc.catalog.unit.bit",
        description: "Basic unit of information",
        dimension_id: Some("information"),
        exact_definition: Some("1 bit"),
        is_exact: true,
        example_queries: &["8 bits in bytes"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "unit.byte",
        kind: CatalogKind::Unit,
        canonical_symbol: "B",
        english_name: "byte",
        aliases: &["bytes"],
        description_key: "calc.catalog.unit.byte",
        description: "Unit of digital information equal to 8 bits",
        dimension_id: Some("information"),
        exact_definition: Some("8 bits"),
        is_exact: true,
        example_queries: &["1 byte in bits"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "unit.kilobit",
        kind: CatalogKind::Unit,
        canonical_symbol: "kb",
        english_name: "kilobit",
        aliases: &["kilobits"],
        description_key: "calc.catalog.unit.kilobit",
        description: "SI decimal kilobit (1000 bits)",
        dimension_id: Some("information"),
        exact_definition: Some("1000 bits"),
        is_exact: true,
        example_queries: &["1 kb in bits"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "unit.kilobyte",
        kind: CatalogKind::Unit,
        canonical_symbol: "kB",
        english_name: "kilobyte",
        aliases: &["kilobytes"],
        description_key: "calc.catalog.unit.kilobyte",
        description: "SI decimal kilobyte (1000 bytes)",
        dimension_id: Some("information"),
        exact_definition: Some("1000 bytes"),
        is_exact: true,
        example_queries: &["1 kB in bytes"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "unit.kibibyte",
        kind: CatalogKind::Unit,
        canonical_symbol: "KiB",
        english_name: "kibibyte",
        aliases: &["kibibytes"],
        description_key: "calc.catalog.unit.kibibyte",
        description: "IEC binary kibibyte (1024 bytes)",
        dimension_id: Some("information"),
        exact_definition: Some("1024 bytes"),
        is_exact: true,
        example_queries: &["1 KiB in bytes"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "unit.megabit",
        kind: CatalogKind::Unit,
        canonical_symbol: "Mb",
        english_name: "megabit",
        aliases: &["megabits"],
        description_key: "calc.catalog.unit.megabit",
        description: "SI decimal megabit (1000000 bits)",
        dimension_id: Some("information"),
        exact_definition: Some("1000000 bits"),
        is_exact: true,
        example_queries: &["2 Mb in kb"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "unit.megabyte",
        kind: CatalogKind::Unit,
        canonical_symbol: "MB",
        english_name: "megabyte",
        aliases: &["megabytes"],
        description_key: "calc.catalog.unit.megabyte",
        description: "SI decimal megabyte (1000000 bytes)",
        dimension_id: Some("information"),
        exact_definition: Some("1000000 bytes"),
        is_exact: true,
        example_queries: &["2 MB in kB"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "unit.mebibyte",
        kind: CatalogKind::Unit,
        canonical_symbol: "MiB",
        english_name: "mebibyte",
        aliases: &["mebibytes"],
        description_key: "calc.catalog.unit.mebibyte",
        description: "IEC binary mebibyte (1048576 bytes)",
        dimension_id: Some("information"),
        exact_definition: Some("1048576 bytes"),
        is_exact: true,
        example_queries: &["1 MiB in KiB"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "unit.gigabyte",
        kind: CatalogKind::Unit,
        canonical_symbol: "GB",
        english_name: "gigabyte",
        aliases: &["gigabytes"],
        description_key: "calc.catalog.unit.gigabyte",
        description: "SI decimal gigabyte (1000000000 bytes)",
        dimension_id: Some("information"),
        exact_definition: Some("1000000000 bytes"),
        is_exact: true,
        example_queries: &["4.7 GB / 100 Mbps"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "unit.gibibyte",
        kind: CatalogKind::Unit,
        canonical_symbol: "GiB",
        english_name: "gibibyte",
        aliases: &["gibibytes"],
        description_key: "calc.catalog.unit.gibibyte",
        description: "IEC binary gibibyte (1073741824 bytes)",
        dimension_id: Some("information"),
        exact_definition: Some("1073741824 bytes"),
        is_exact: true,
        example_queries: &["1 GiB in MiB"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    // --- Data Rate Units ---
    CatalogEntry {
        stable_id: "unit.megabit_per_second",
        kind: CatalogKind::Unit,
        canonical_symbol: "Mbps",
        english_name: "megabit per second",
        aliases: &["megabits per second"],
        description_key: "calc.catalog.unit.megabit_per_second",
        description: "Data transfer rate of 1000000 bits per second",
        dimension_id: Some("data_rate"),
        exact_definition: Some("1000000 bits/s"),
        is_exact: true,
        example_queries: &["100 Mbps to MB/s"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "unit.megabyte_per_second",
        kind: CatalogKind::Unit,
        canonical_symbol: "MB/s",
        english_name: "megabyte per second",
        aliases: &["megabytes per second"],
        description_key: "calc.catalog.unit.megabyte_per_second",
        description: "Data transfer rate of 1000000 bytes per second",
        dimension_id: Some("data_rate"),
        exact_definition: Some("1000000 bytes/s"),
        is_exact: true,
        example_queries: &["100 Mbps to MB/s"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "unit.mebibyte_per_second",
        kind: CatalogKind::Unit,
        canonical_symbol: "MiB/s",
        english_name: "mebibyte per second",
        aliases: &["mebibytes per second"],
        description_key: "calc.catalog.unit.mebibyte_per_second",
        description: "Data transfer rate of 1048576 bytes per second",
        dimension_id: Some("data_rate"),
        exact_definition: Some("1048576 bytes/s"),
        is_exact: true,
        example_queries: &["100 Mbps to MiB/s"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    // --- Energy Units ---
    CatalogEntry {
        stable_id: "unit.kilowatt_hour",
        kind: CatalogKind::Unit,
        canonical_symbol: "kWh",
        english_name: "kilowatt-hour",
        aliases: &["kilowatt hour", "kilowatt-hours"],
        description_key: "calc.catalog.unit.kilowatt_hour",
        description: "Unit of energy equal to 3.6 megajoules",
        dimension_id: Some("energy"),
        exact_definition: Some("3600000 J"),
        is_exact: true,
        example_queries: &["1 kWh in J"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "unit.kilocalorie",
        kind: CatalogKind::Unit,
        canonical_symbol: "kcal",
        english_name: "kilocalorie",
        aliases: &["kilocalories", "food calorie"],
        description_key: "calc.catalog.unit.kilocalorie",
        description: "Thermochemical kilocalorie (4184 joules)",
        dimension_id: Some("energy"),
        exact_definition: Some("4184 J"),
        is_exact: true,
        example_queries: &["500 kcal in kJ"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "unit.joule",
        kind: CatalogKind::Unit,
        canonical_symbol: "J",
        english_name: "joule",
        aliases: &["joules"],
        description_key: "calc.catalog.unit.joule",
        description: "SI derived unit of energy",
        dimension_id: Some("energy"),
        exact_definition: Some("1 kg * m^2 / s^2"),
        is_exact: true,
        example_queries: &["1000 J in kcal"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    // --- Time Units ---
    CatalogEntry {
        stable_id: "unit.second",
        kind: CatalogKind::Unit,
        canonical_symbol: "s",
        english_name: "second",
        aliases: &["seconds", "sec"],
        description_key: "calc.catalog.unit.second",
        description: "SI base unit of time",
        dimension_id: Some("time"),
        exact_definition: Some("1 s"),
        is_exact: true,
        example_queries: &["376 s in min"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "unit.minute",
        kind: CatalogKind::Unit,
        canonical_symbol: "min",
        english_name: "minute",
        aliases: &["minutes"],
        description_key: "calc.catalog.unit.minute",
        description: "Unit of time equal to 60 seconds",
        dimension_id: Some("time"),
        exact_definition: Some("60 s"),
        is_exact: true,
        example_queries: &["5 min in s"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "unit.hour",
        kind: CatalogKind::Unit,
        canonical_symbol: "h",
        english_name: "hour",
        aliases: &["hours", "hr"],
        description_key: "calc.catalog.unit.hour",
        description: "Unit of time equal to 3600 seconds",
        dimension_id: Some("time"),
        exact_definition: Some("3600 s"),
        is_exact: true,
        example_queries: &["2 h in min"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    // --- Speed & Temperature Units ---
    CatalogEntry {
        stable_id: "unit.metre_per_second",
        kind: CatalogKind::Unit,
        canonical_symbol: "m/s",
        english_name: "metre per second",
        aliases: &["meter per second", "metres per second"],
        description_key: "calc.catalog.unit.metre_per_second",
        description: "SI derived unit of speed",
        dimension_id: Some("speed"),
        exact_definition: Some("1 m / s"),
        is_exact: true,
        example_queries: &["10 m/s in km/h"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "unit.kilometre_per_hour",
        kind: CatalogKind::Unit,
        canonical_symbol: "km/h",
        english_name: "kilometre per hour",
        aliases: &["kph", "kilometer per hour"],
        description_key: "calc.catalog.unit.kilometre_per_hour",
        description: "Unit of speed (1000/3600 m/s)",
        dimension_id: Some("speed"),
        exact_definition: Some("1000 m / 3600 s"),
        is_exact: true,
        example_queries: &["100 km/h in m/s"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "unit.kelvin",
        kind: CatalogKind::Unit,
        canonical_symbol: "K",
        english_name: "kelvin",
        aliases: &["kelvins"],
        description_key: "calc.catalog.unit.kelvin",
        description: "SI base unit of thermodynamic temperature",
        dimension_id: Some("temperature"),
        exact_definition: Some("1 K"),
        is_exact: true,
        example_queries: &["273.15 K in degC"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "unit.celsius",
        kind: CatalogKind::Unit,
        canonical_symbol: "°C",
        english_name: "degree Celsius",
        aliases: &["celsius", "degC"],
        description_key: "calc.catalog.unit.celsius",
        description: "Celsius temperature scale (offset 273.15 from Kelvin)",
        dimension_id: Some("temperature"),
        exact_definition: Some("T(K) - 273.15"),
        is_exact: true,
        example_queries: &["25 degC in degF"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
    CatalogEntry {
        stable_id: "unit.fahrenheit",
        kind: CatalogKind::Unit,
        canonical_symbol: "°F",
        english_name: "degree Fahrenheit",
        aliases: &["fahrenheit", "degF"],
        description_key: "calc.catalog.unit.fahrenheit",
        description: "Fahrenheit temperature scale",
        dimension_id: Some("temperature"),
        exact_definition: Some("(T(K) - 273.15) * 9/5 + 32"),
        is_exact: true,
        example_queries: &["72 degF in degC"],
        fend_compatible: true,
        introduced_in_catalog_version: 1,
    },
];

const RESERVED_KEYWORDS: &[&str] = &[
    "in", "to", "as", "and", "or", "xor", "not", "mod", "of", "per",
];

const MAX_COMPLETIONS: usize = 12;

#[derive(Debug, Clone)]
struct MatchSpan {
    field: &'static str,
    start_utf16: usize,
    end_utf16: usize,
}

#[derive(Debug, Clone)]
struct RankedMatch {
    entry_idx: usize,
    tier: ScoreTier,
    exact_case: bool,
    edit_distance: usize,
    match_reason: &'static str,
    spans: Vec<MatchSpan>,
}

pub(crate) fn is_reserved(name: &str) -> bool {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lower = trimmed.to_lowercase();
    if RESERVED_KEYWORDS.contains(&lower.as_str()) {
        return true;
    }
    for entry in CATALOG {
        if entry.canonical_symbol == trimmed {
            return true;
        }
        if entry.kind == CatalogKind::Function
            && entry.canonical_symbol.eq_ignore_ascii_case(trimmed)
        {
            return true;
        }
        if entry.english_name.eq_ignore_ascii_case(trimmed) {
            return true;
        }
    }
    false
}

pub(crate) fn search(query: &str, limit: usize) -> Vec<Value> {
    let trimmed = query.trim();
    if trimmed.is_empty() || limit == 0 {
        return Vec::new();
    }

    let mut ranked = rank_catalog(trimmed);
    ranked.truncate(limit);

    ranked
        .into_iter()
        .map(|m| {
            let entry = &CATALOG[m.entry_idx];
            let label = format_label(entry);
            let ranges: Vec<Value> = m
                .spans
                .iter()
                .map(|s| {
                    json!({
                        "field": s.field,
                        "startUtf16": s.start_utf16,
                        "endUtf16": s.end_utf16,
                    })
                })
                .collect();

            let mut obj = json!({
                "id": entry.stable_id,
                "kind": entry.kind.as_str(),
                "canonicalSymbol": entry.canonical_symbol,
                "name": entry.english_name,
                "label": label,
                "aliases": entry.aliases,
                "descriptionKey": entry.description_key,
                "description": entry.description,
                "exact": entry.is_exact,
                "exampleQueries": entry.example_queries,
                "fendCompatible": entry.fend_compatible,
                "documentationOnly": !entry.fend_compatible,
                "matchReason": m.match_reason,
                "scoreTier": m.tier.as_str(),
                "matchRanges": ranges,
                "catalogVersion": entry.introduced_in_catalog_version,
            });
            if let Some(dim) = entry.dimension_id {
                obj["dimensionId"] = json!(dim);
            }
            if let Some(def) = entry.exact_definition {
                obj["exactDefinition"] = json!(def);
            }
            obj
        })
        .collect()
}

pub(crate) fn complete(query: &str, cursor_utf16: usize) -> Vec<Value> {
    let Some(cursor_byte) = utf16_cursor_to_byte(query, cursor_utf16) else {
        return Vec::new();
    };
    let Some((start_byte, end_byte, token)) = extract_token_at(query, cursor_byte) else {
        return Vec::new();
    };
    if token.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return Vec::new();
    }

    let start_utf16 = query[..start_byte].encode_utf16().count();
    let end_utf16 = query[..end_byte].encode_utf16().count();

    let mut ranked = rank_catalog(token);
    ranked.truncate(MAX_COMPLETIONS);

    ranked
        .into_iter()
        .map(|m| {
            let entry = &CATALOG[m.entry_idx];
            let insert_text = match entry.kind {
                CatalogKind::Unit => entry.english_name,
                CatalogKind::Function | CatalogKind::Constant => entry.canonical_symbol,
            };
            let mut obj = json!({
                "id": entry.canonical_symbol,
                "stableId": entry.stable_id,
                "label": format_label(entry),
                "kind": entry.kind.as_str(),
                "canonicalSymbol": entry.canonical_symbol,
                "scoreTier": m.tier.as_str(),
                "matchReason": m.match_reason,
                "edit": {
                    "startUtf16": start_utf16,
                    "endUtf16": end_utf16,
                    "text": insert_text,
                },
                "previewCandidateId": format!("c-{}", entry.canonical_symbol),
                "fendCompatible": entry.fend_compatible,
            });
            if let Some(dim) = entry.dimension_id {
                obj["dimensionId"] = json!(dim);
            }
            if let Some(def) = entry.exact_definition {
                obj["exactDefinition"] = json!(def);
            }
            obj
        })
        .collect()
}

fn format_label(entry: &CatalogEntry) -> String {
    if entry
        .canonical_symbol
        .eq_ignore_ascii_case(entry.english_name)
    {
        entry.canonical_symbol.to_string()
    } else {
        format!("{} · {}", entry.english_name, entry.canonical_symbol)
    }
}

fn utf16_cursor_to_byte(s: &str, cursor_utf16: usize) -> Option<usize> {
    if cursor_utf16 == 0 {
        return Some(0);
    }
    let mut utf16_acc = 0usize;
    for (byte_idx, ch) in s.char_indices() {
        if utf16_acc == cursor_utf16 {
            return Some(byte_idx);
        }
        utf16_acc += ch.len_utf16();
        if utf16_acc > cursor_utf16 {
            // cursor_utf16 points inside a surrogate pair
            return None;
        }
    }
    if utf16_acc == cursor_utf16 {
        Some(s.len())
    } else {
        None
    }
}

fn is_token_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '°' || c == '√' || c == '∛' || c == '-'
}

fn extract_token_at(s: &str, cursor_byte: usize) -> Option<(usize, usize, &str)> {
    let mut start = cursor_byte;
    for (idx, ch) in s[..cursor_byte].char_indices().rev() {
        if is_token_char(ch) {
            if ch == '-' {
                // Only treat '-' as part of a hyphenated word if preceded by an alphabetic char.
                let prev_alpha = s[..idx]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_alphabetic);
                if !prev_alpha {
                    break;
                }
            }
            start = idx;
        } else {
            break;
        }
    }

    let mut end = cursor_byte;
    for (rel_idx, ch) in s[cursor_byte..].char_indices() {
        if is_token_char(ch) {
            if ch == '-' {
                let prev_alpha = s[..cursor_byte + rel_idx]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_alphabetic);
                if !prev_alpha {
                    break;
                }
            }
            end = cursor_byte + rel_idx + ch.len_utf8();
        } else {
            break;
        }
    }

    if start == end {
        None
    } else {
        Some((start, end, &s[start..end]))
    }
}

fn fold_for_search(s: &str) -> String {
    s.trim().to_lowercase()
}

fn rank_catalog(raw_query: &str) -> Vec<RankedMatch> {
    let query_trimmed = raw_query.trim();
    let query_folded = fold_for_search(query_trimmed);
    let mut matches = Vec::new();

    for (idx, entry) in CATALOG.iter().enumerate() {
        if let Some(m) = match_entry(idx, entry, query_trimmed, &query_folded) {
            matches.push(m);
        }
    }

    matches.sort_by(compare_matches);
    matches
}

fn compare_matches(a: &RankedMatch, b: &RankedMatch) -> Ordering {
    let ea = &CATALOG[a.entry_idx];
    let eb = &CATALOG[b.entry_idx];

    a.tier
        .cmp(&b.tier)
        .then_with(|| b.exact_case.cmp(&a.exact_case))
        .then_with(|| a.edit_distance.cmp(&b.edit_distance))
        .then_with(|| ea.canonical_symbol.len().cmp(&eb.canonical_symbol.len()))
        .then_with(|| ea.english_name.cmp(eb.english_name))
        .then_with(|| ea.stable_id.cmp(eb.stable_id))
}

fn match_entry(
    entry_idx: usize,
    entry: &CatalogEntry,
    raw_query: &str,
    folded_query: &str,
) -> Option<RankedMatch> {
    let sym_folded = fold_for_search(entry.canonical_symbol);
    let name_folded = fold_for_search(entry.english_name);

    // Tier 1: Exact canonical symbol match (exact case or folded)
    if entry.canonical_symbol == raw_query {
        return Some(RankedMatch {
            entry_idx,
            tier: ScoreTier::ExactSymbol,
            exact_case: true,
            edit_distance: 0,
            match_reason: "exact_symbol",
            spans: vec![MatchSpan {
                field: "canonicalSymbol",
                start_utf16: 0,
                end_utf16: entry.canonical_symbol.encode_utf16().count(),
            }],
        });
    }
    if sym_folded == folded_query {
        return Some(RankedMatch {
            entry_idx,
            tier: ScoreTier::ExactSymbol,
            exact_case: false,
            edit_distance: 0,
            match_reason: "exact_symbol",
            spans: vec![MatchSpan {
                field: "canonicalSymbol",
                start_utf16: 0,
                end_utf16: entry.canonical_symbol.encode_utf16().count(),
            }],
        });
    }

    // Tier 1b: Exact canonical English name match
    if name_folded == folded_query {
        return Some(RankedMatch {
            entry_idx,
            tier: ScoreTier::ExactName,
            exact_case: entry.english_name == raw_query,
            edit_distance: 0,
            match_reason: "exact_name",
            spans: vec![MatchSpan {
                field: "name",
                start_utf16: 0,
                end_utf16: entry.english_name.encode_utf16().count(),
            }],
        });
    }

    // Tier 2: Exact alias match
    for &alias in entry.aliases {
        let alias_folded = fold_for_search(alias);
        if alias_folded == folded_query {
            return Some(RankedMatch {
                entry_idx,
                tier: ScoreTier::ExactAlias,
                exact_case: alias == raw_query,
                edit_distance: 0,
                match_reason: "exact_alias",
                spans: vec![MatchSpan {
                    field: "aliases",
                    start_utf16: 0,
                    end_utf16: alias.encode_utf16().count(),
                }],
            });
        }
    }

    // Tier 3: Starts-with token prefix on symbol or English name
    if entry.canonical_symbol.starts_with(raw_query) {
        return Some(RankedMatch {
            entry_idx,
            tier: ScoreTier::Prefix,
            exact_case: true,
            edit_distance: 0,
            match_reason: "prefix",
            spans: vec![MatchSpan {
                field: "canonicalSymbol",
                start_utf16: 0,
                end_utf16: raw_query.encode_utf16().count(),
            }],
        });
    }
    if sym_folded.starts_with(folded_query) {
        return Some(RankedMatch {
            entry_idx,
            tier: ScoreTier::Prefix,
            exact_case: false,
            edit_distance: 0,
            match_reason: "prefix",
            spans: vec![MatchSpan {
                field: "canonicalSymbol",
                start_utf16: 0,
                end_utf16: raw_query.encode_utf16().count(),
            }],
        });
    }
    if name_folded.starts_with(folded_query) {
        return Some(RankedMatch {
            entry_idx,
            tier: ScoreTier::Prefix,
            exact_case: entry.english_name.starts_with(raw_query),
            edit_distance: 0,
            match_reason: "prefix",
            spans: vec![MatchSpan {
                field: "name",
                start_utf16: 0,
                end_utf16: raw_query.encode_utf16().count(),
            }],
        });
    }

    // Tier 4: Starts-with any word in title or alias
    if let Some(span) = find_word_start_span("name", entry.english_name, &name_folded, folded_query)
    {
        return Some(RankedMatch {
            entry_idx,
            tier: ScoreTier::WordStart,
            exact_case: false,
            edit_distance: 0,
            match_reason: "word_start",
            spans: vec![span],
        });
    }
    for &alias in entry.aliases {
        let alias_folded = fold_for_search(alias);
        if let Some(span) = find_word_start_span("aliases", alias, &alias_folded, folded_query) {
            return Some(RankedMatch {
                entry_idx,
                tier: ScoreTier::WordStart,
                exact_case: false,
                edit_distance: 0,
                match_reason: "word_start",
                spans: vec![span],
            });
        }
    }

    // Multi-word query word-start check (e.g. "speed of light" matching "light-year")
    let query_words: Vec<&str> = folded_query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 3 && !RESERVED_KEYWORDS.contains(w))
        .collect();
    if query_words.len() >= 2 {
        for qw in &query_words {
            if let Some(span) = find_word_start_span("name", entry.english_name, &name_folded, qw) {
                return Some(RankedMatch {
                    entry_idx,
                    tier: ScoreTier::WordStart,
                    exact_case: false,
                    edit_distance: 0,
                    match_reason: "word_start",
                    spans: vec![span],
                });
            }
        }
    }

    // Tier 5: Ordered subsequence or bounded fuzzy edit distance
    let q_chars = folded_query.chars().count();
    if q_chars >= 2 {
        if let Some(dist) = bounded_fuzzy_distance(&name_folded, folded_query) {
            return Some(RankedMatch {
                entry_idx,
                tier: ScoreTier::Fuzzy,
                exact_case: false,
                edit_distance: dist,
                match_reason: "fuzzy",
                spans: Vec::new(),
            });
        }
        if let Some(dist) = bounded_fuzzy_distance(&sym_folded, folded_query) {
            return Some(RankedMatch {
                entry_idx,
                tier: ScoreTier::Fuzzy,
                exact_case: false,
                edit_distance: dist,
                match_reason: "fuzzy",
                spans: Vec::new(),
            });
        }
        if is_subsequence(folded_query, &name_folded) {
            return Some(RankedMatch {
                entry_idx,
                tier: ScoreTier::Fuzzy,
                exact_case: false,
                edit_distance: name_folded.chars().count().saturating_sub(q_chars),
                match_reason: "subsequence",
                spans: Vec::new(),
            });
        }
    }

    // Tier 6: Description-only match
    let desc_folded = fold_for_search(entry.description);
    if let Some(byte_pos) = desc_folded.find(folded_query) {
        let start_utf16 = desc_folded[..byte_pos].encode_utf16().count();
        let end_utf16 = start_utf16 + folded_query.encode_utf16().count();
        return Some(RankedMatch {
            entry_idx,
            tier: ScoreTier::Description,
            exact_case: false,
            edit_distance: 0,
            match_reason: "description",
            spans: vec![MatchSpan {
                field: "description",
                start_utf16,
                end_utf16,
            }],
        });
    }
    for qw in &query_words {
        if let Some(byte_pos) = desc_folded.find(qw) {
            let start_utf16 = desc_folded[..byte_pos].encode_utf16().count();
            let end_utf16 = start_utf16 + qw.encode_utf16().count();
            return Some(RankedMatch {
                entry_idx,
                tier: ScoreTier::Description,
                exact_case: false,
                edit_distance: 0,
                match_reason: "description",
                spans: vec![MatchSpan {
                    field: "description",
                    start_utf16,
                    end_utf16,
                }],
            });
        }
    }

    None
}

fn find_word_start_span(
    field: &'static str,
    original: &str,
    folded: &str,
    needle: &str,
) -> Option<MatchSpan> {
    if needle.is_empty() {
        return None;
    }
    let mut in_word = false;
    for (byte_idx, ch) in folded.char_indices() {
        let is_sep = !ch.is_alphanumeric();
        if is_sep {
            in_word = false;
            continue;
        }
        if !in_word {
            in_word = true;
            if folded[byte_idx..].starts_with(needle) {
                let prefix_chars = folded[..byte_idx].chars().count();
                let start_utf16: usize = original
                    .chars()
                    .take(prefix_chars)
                    .map(char::len_utf16)
                    .sum();
                let end_utf16 = start_utf16 + needle.encode_utf16().count();
                return Some(MatchSpan {
                    field,
                    start_utf16,
                    end_utf16,
                });
            }
        }
    }
    None
}

fn bounded_fuzzy_distance(candidate: &str, query: &str) -> Option<usize> {
    let q_len = query.chars().count();
    let c_len = candidate.chars().count();
    let max_dist = match q_len {
        0..=2 => return None,
        3..=5 => 1,
        _ => 2,
    };
    if c_len.abs_diff(q_len) > max_dist {
        return None;
    }
    let dist = damerau_levenshtein(candidate, query);
    if dist <= max_dist { Some(dist) } else { None }
}

fn damerau_levenshtein(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let n = a_chars.len();
    let m = b_chars.len();

    let mut d = vec![vec![0usize; m + 1]; n + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }

    for i in 1..=n {
        for j in 1..=m {
            let cost = usize::from(a_chars[i - 1] != b_chars[j - 1]);
            let mut val = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
            if i > 1
                && j > 1
                && a_chars[i - 1] == b_chars[j - 2]
                && a_chars[i - 2] == b_chars[j - 1]
            {
                val = val.min(d[i - 2][j - 2] + 1);
            }
            d[i][j] = val;
        }
    }
    d[n][m]
}

fn is_subsequence(needle: &str, haystack: &str) -> bool {
    let mut h_chars = haystack.chars();
    for nc in needle.chars() {
        if !h_chars.any(|hc| hc == nc) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_of_light_search_returns_exact_c_primary_and_related() {
        let results = search("speed of light", 10);
        assert!(!results.is_empty());

        let primary = &results[0];
        assert_eq!(primary["id"], "constant.speed_of_light");
        assert_eq!(primary["canonicalSymbol"], "c");
        assert_eq!(primary["exact"], true);
        assert_eq!(primary["exactDefinition"], "299792458 m/s");
        assert_eq!(primary["fendCompatible"], true);

        let ids: Vec<&str> = results.iter().filter_map(|r| r["id"].as_str()).collect();
        assert!(ids.contains(&"unit.light_year"));
        assert!(ids.contains(&"unit.light_second"));
        assert!(ids.contains(&"unit.lumen"));
    }

    #[test]
    fn figma_h_autocomplete_1_kilo() {
        let query = "1 kilo";
        let cursor = query.encode_utf16().count();
        let completions = complete(query, cursor);
        assert!(!completions.is_empty());

        let ids: Vec<&str> = completions
            .iter()
            .filter_map(|c| c["stableId"].as_str())
            .collect();
        assert!(ids.contains(&"unit.kilogram"));
        assert!(ids.contains(&"unit.kilometre"));
        assert!(ids.contains(&"unit.kilobyte"));
        assert!(ids.contains(&"unit.kilowatt_hour"));
        assert!(ids.contains(&"unit.kilocalorie"));

        let first = &completions[0];
        assert_eq!(first["edit"]["startUtf16"], 2);
        assert_eq!(first["edit"]["endUtf16"], 6);
    }

    #[test]
    fn rank_tiers_prefix_before_word_start_before_subsequence() {
        let results = search("year", 10);
        // "light-year" has word-start "year"
        let ly = results
            .iter()
            .find(|r| r["id"] == "unit.light_year")
            .expect("light-year should match");
        assert_eq!(ly["scoreTier"], "word_start");

        let prefix_results = search("kilo", 12);
        assert_eq!(prefix_results[0]["scoreTier"], "exact_alias");
        assert!(prefix_results.iter().any(|r| r["scoreTier"] == "prefix"));

        let fuzzy_results = search("kilogrm", 5);
        assert_eq!(fuzzy_results[0]["id"], "unit.kilogram");
        assert_eq!(fuzzy_results[0]["scoreTier"], "fuzzy");
    }

    #[test]
    fn mb_and_mb_never_coalesce_and_preserve_case() {
        let upper = search("MB", 10);
        assert_eq!(upper[0]["id"], "unit.megabyte");
        assert_eq!(upper[0]["canonicalSymbol"], "MB");

        let mixed = search("Mb", 10);
        assert_eq!(mixed[0]["id"], "unit.megabit");
        assert_eq!(mixed[0]["canonicalSymbol"], "Mb");
    }

    #[test]
    fn emoji_and_cjk_before_edit_range_utf16() {
        // "🚀" = 2 UTF-16 code units, " " = 1, "你好" = 2, " " = 1 -> "kilo" starts at 6, ends at 10
        let query = "🚀 你好 kilo";
        let cursor = query.encode_utf16().count();
        let completions = complete(query, cursor);
        assert!(!completions.is_empty());
        assert_eq!(completions[0]["edit"]["startUtf16"], 6);
        assert_eq!(completions[0]["edit"]["endUtf16"], 10);
    }

    #[test]
    fn mid_token_and_mid_query_cursor() {
        let query = "1 ki + 2";
        // cursor right after "ki" (UTF-16 index 4)
        let completions = complete(query, 4);
        assert!(!completions.is_empty());
        assert_eq!(completions[0]["edit"]["startUtf16"], 2);
        assert_eq!(completions[0]["edit"]["endUtf16"], 4);

        let query2 = "1 kilo + 2";
        // cursor in the middle of "kilo" (UTF-16 index 4: "1 ki|lo + 2")
        let completions2 = complete(query2, 4);
        assert!(!completions2.is_empty());
        assert_eq!(completions2[0]["edit"]["startUtf16"], 2);
        assert_eq!(completions2[0]["edit"]["endUtf16"], 6);
    }

    #[test]
    fn invalid_utf16_boundary_yields_no_edit() {
        let query = "🚀 kilo";
        // index 1 splits the surrogate pair for 🚀 (0..2)
        assert!(complete(query, 1).is_empty());
        // out of bounds
        assert!(complete(query, 99).is_empty());
    }

    #[test]
    fn empty_query_returns_no_results() {
        assert!(search("", 10).is_empty());
        assert!(search("   ", 10).is_empty());
        assert!(complete("", 0).is_empty());
        assert!(complete("1 ", 2).is_empty());
    }

    #[test]
    fn reserved_names_checked_accurately() {
        assert!(is_reserved("c"));
        assert!(is_reserved("pi"));
        assert!(is_reserved("sqrt"));
        assert!(is_reserved("kg"));
        assert!(is_reserved("MB"));
        assert!(is_reserved("Mb"));
        assert!(is_reserved("in"));
        assert!(is_reserved("to"));
        assert!(!is_reserved("rate"));
        assert!(!is_reserved("fb"));
        assert!(!is_reserved("my_var"));
        assert!(!is_reserved(""));
    }
}
