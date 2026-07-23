use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity {
    Error,
    Warning,
    Note,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LabelStyle {
    Primary,
    Secondary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WarningCategory {
    Unused,
    Type,
    Performance,
    Style,
    Complexity,
    Logic,
    Memory,
    Deprecated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticCode {
    E0001,
    E0003,
    E0005,
    E0007,
    E0008,
    E0009,
    E0010,
    E0011,
    E0013,
    E0014,
    E0015,
    E0016,
    E0100,
    E0101,
    E0102,
    E0103,
    E0104,
    E0105,
    E0106,
    E0107,
    E0108,
    E0109,
    E0110,
    W0001,
    W0002,
    W0004,
    W0005,
    W0006,
    W0007,
    W0008,
    W0009,
    W0010,
    W0011,
    W0012,
    W0013,
    W0014,
    W0017,
    W0018,
    W0019,
    W0021,
    W0024,
    W0025,
    W0034,
    W0035,
    W0036,
    W0037,
    W0038,
    W0039,
    W0040,
    W0041,
    W0042,
    W0043,
    W0044,
    W0045,
    W0046,
    W0047,
    W0048,
    W0049,
}

impl DiagnosticCode {
    pub fn as_str(self) -> &'static str {
        match self {
            DiagnosticCode::E0001 => "E0001",
            DiagnosticCode::E0003 => "E0003",
            DiagnosticCode::E0005 => "E0005",
            DiagnosticCode::E0007 => "E0007",
            DiagnosticCode::E0008 => "E0008",
            DiagnosticCode::E0009 => "E0009",
            DiagnosticCode::E0010 => "E0010",
            DiagnosticCode::E0011 => "E0011",
            DiagnosticCode::E0013 => "E0013",
            DiagnosticCode::E0014 => "E0014",
            DiagnosticCode::E0015 => "E0015",
            DiagnosticCode::E0016 => "E0016",
            DiagnosticCode::E0100 => "E0100",
            DiagnosticCode::E0101 => "E0101",
            DiagnosticCode::E0102 => "E0102",
            DiagnosticCode::E0103 => "E0103",
            DiagnosticCode::E0104 => "E0104",
            DiagnosticCode::E0105 => "E0105",
            DiagnosticCode::E0106 => "E0106",
            DiagnosticCode::E0107 => "E0107",
            DiagnosticCode::E0108 => "E0108",
            DiagnosticCode::E0109 => "E0109",
            DiagnosticCode::E0110 => "E0110",
            DiagnosticCode::W0001 => "W0001",
            DiagnosticCode::W0002 => "W0002",
            DiagnosticCode::W0004 => "W0004",
            DiagnosticCode::W0005 => "W0005",
            DiagnosticCode::W0006 => "W0006",
            DiagnosticCode::W0007 => "W0007",
            DiagnosticCode::W0008 => "W0008",
            DiagnosticCode::W0009 => "W0009",
            DiagnosticCode::W0010 => "W0010",
            DiagnosticCode::W0011 => "W0011",
            DiagnosticCode::W0012 => "W0012",
            DiagnosticCode::W0013 => "W0013",
            DiagnosticCode::W0014 => "W0014",
            DiagnosticCode::W0017 => "W0017",
            DiagnosticCode::W0018 => "W0018",
            DiagnosticCode::W0019 => "W0019",
            DiagnosticCode::W0021 => "W0021",
            DiagnosticCode::W0024 => "W0024",
            DiagnosticCode::W0025 => "W0025",
            DiagnosticCode::W0034 => "W0034",
            DiagnosticCode::W0035 => "W0035",
            DiagnosticCode::W0036 => "W0036",
            DiagnosticCode::W0037 => "W0037",
            DiagnosticCode::W0038 => "W0038",
            DiagnosticCode::W0039 => "W0039",
            DiagnosticCode::W0040 => "W0040",
            DiagnosticCode::W0041 => "W0041",
            DiagnosticCode::W0042 => "W0042",
            DiagnosticCode::W0043 => "W0043",
            DiagnosticCode::W0044 => "W0044",
            DiagnosticCode::W0045 => "W0045",
            DiagnosticCode::W0046 => "W0046",
            DiagnosticCode::W0047 => "W0047",
            DiagnosticCode::W0048 => "W0048",
            DiagnosticCode::W0049 => "W0049",
        }
    }

    pub fn warning_category(self) -> Option<WarningCategory> {
        match self {
            DiagnosticCode::W0001 | DiagnosticCode::W0002 => {
                Some(WarningCategory::Unused)
            }
            DiagnosticCode::W0004 | DiagnosticCode::W0005 | DiagnosticCode::W0021 => {
                Some(WarningCategory::Type)
            }
            DiagnosticCode::W0007 | DiagnosticCode::W0008 | DiagnosticCode::W0009 => {
                Some(WarningCategory::Performance)
            }
            DiagnosticCode::W0006
            | DiagnosticCode::W0012
            | DiagnosticCode::W0013
            | DiagnosticCode::W0014
            | DiagnosticCode::W0024 => Some(WarningCategory::Style),
            DiagnosticCode::W0011 | DiagnosticCode::W0018 => Some(WarningCategory::Complexity),
            DiagnosticCode::W0017 | DiagnosticCode::W0019 | DiagnosticCode::W0036
            | DiagnosticCode::W0038
            | DiagnosticCode::W0040 => Some(WarningCategory::Logic),
            DiagnosticCode::W0041
            | DiagnosticCode::W0042
            | DiagnosticCode::W0043
            | DiagnosticCode::W0044
            | DiagnosticCode::W0045
            | DiagnosticCode::W0046
            | DiagnosticCode::W0047 => Some(WarningCategory::Complexity),
            DiagnosticCode::W0048 => Some(WarningCategory::Style),
            DiagnosticCode::W0049 => Some(WarningCategory::Logic),
            DiagnosticCode::W0025 => Some(WarningCategory::Memory),
            DiagnosticCode::W0010 => Some(WarningCategory::Deprecated),
            DiagnosticCode::W0034 | DiagnosticCode::W0035 | DiagnosticCode::W0037 => {
                Some(WarningCategory::Style)
            }
            DiagnosticCode::W0039 => Some(WarningCategory::Complexity),
            _ => None,
        }
    }

    pub fn is_warning(self) -> bool {
        self.as_str().starts_with('W')
    }

    pub fn name(self) -> &'static str {
        match self {
            DiagnosticCode::W0001 => "unused_variables",
            DiagnosticCode::W0002 => "dead_code",
            DiagnosticCode::W0004 => "implicit_type",
            DiagnosticCode::W0005 => "implicit_return_type",
            DiagnosticCode::W0006 => "empty_body",
            DiagnosticCode::W0007 => "multiply_by_zero",
            DiagnosticCode::W0008 => "divide_by_zero",
            DiagnosticCode::W0009 => "modulo_by_zero",
            DiagnosticCode::W0010 => "deprecated_syntax",
            DiagnosticCode::W0011 => "long_function",
            DiagnosticCode::W0012 => "many_parameters",
            DiagnosticCode::W0013 => "empty_loop_body",
            DiagnosticCode::W0014 => "empty_if_branches",
            DiagnosticCode::W0017 => "unreachable_loop",
            DiagnosticCode::W0018 => "deep_loop_nesting",
            DiagnosticCode::W0019 => "break_outside_loop",
            DiagnosticCode::W0021 => "negative_index",
            DiagnosticCode::W0024 => "long_string_literal",
            DiagnosticCode::W0025 => "large_literal",
            DiagnosticCode::W0034 => "non_snake_case_variable",
            DiagnosticCode::W0035 => "non_snake_case_function",
            DiagnosticCode::W0036 => "self_comparison",
            DiagnosticCode::W0037 => "excessive_arguments",
            DiagnosticCode::W0038 => "duplicate_match_pattern",
            DiagnosticCode::W0039 => "variable_shadowing",
            DiagnosticCode::W0040 => "missing_explicit_return",
            DiagnosticCode::W0041 => "uninitialized_variable",
            DiagnosticCode::W0042 => "infinite_while_true",
            DiagnosticCode::W0043 => "deeply_nested_if",
            DiagnosticCode::W0044 => "unnecessary_mutable",
            DiagnosticCode::W0045 => "redundant_bool_compare",
            DiagnosticCode::W0046 => "simplifiable_if_return_bool",
            DiagnosticCode::W0047 => "string_concat_in_loop",
            DiagnosticCode::W0048 => "unused_mutable_binding",
            DiagnosticCode::W0049 => "empty_match_body",
            DiagnosticCode::E0100 => "precision_loss",
            DiagnosticCode::E0101 => "unsigned_precision_loss",
            DiagnosticCode::E0102 => "float_precision_loss",
            DiagnosticCode::E0103 => "int_to_float_requires_cast",
            DiagnosticCode::E0104 => "float_to_int_requires_cast",
            DiagnosticCode::E0105 => "sign_mismatch",
            DiagnosticCode::E0106 => "type_mismatch",
            DiagnosticCode::E0107 => "literal_out_of_range",
            DiagnosticCode::E0108 => "invalid_char",
            DiagnosticCode::E0109 => "str_bytes_mismatch",
            DiagnosticCode::E0110 => "numeric_kind_mismatch",
            _ => self.as_str(),
        }
    }
}

use crate::error::Span;

#[derive(Debug, Clone)]
pub struct Label {
    pub span: Span,
    pub length: usize,
    pub message: String,
    pub style: LabelStyle,
}

#[derive(Debug, Clone)]
pub struct Suggestion {
    pub message: String,
    pub replacement: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: DiagnosticCode,
    pub message: String,
    pub title: String,
    pub span: Span,
    pub labels: Vec<Label>,
    pub notes: Vec<String>,
    pub help: Option<String>,
    pub suggestions: Vec<Suggestion>,
    pub source: Option<String>,
    pub filename: Option<String>,
}

impl Diagnostic {
    pub fn new(
        severity: Severity,
        code: DiagnosticCode,
        title: impl Into<String>,
        message: impl Into<String>,
        span: Span,
    ) -> Self {
        Self {
            severity,
            code,
            title: title.into(),
            message: message.into(),
            span,
            labels: Vec::new(),
            notes: Vec::new(),
            help: None,
            suggestions: Vec::new(),
            source: None,
            filename: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub enum WarningFilter {
    #[default]
    Off,
    All,
    Codes(HashSet<DiagnosticCode>),
}

impl WarningFilter {
    pub fn matches(&self, code: DiagnosticCode) -> bool {
        match self {
            WarningFilter::Off => false,
            WarningFilter::All => code.is_warning(),
            WarningFilter::Codes(codes) => codes.contains(&code),
        }
    }
}
