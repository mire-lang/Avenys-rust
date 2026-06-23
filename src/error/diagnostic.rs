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
    E0002,
    E0003,
    E0004,
    E0005,
    E0006,
    E0007,
    E0008,
    E0009,
    E0010,
    E0011,
    E0012,
    E0013,
    E0014,
    E0015,
    W0001,
    W0002,
    W0003,
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
    W0015,
    W0016,
    W0017,
    W0018,
    W0019,
    W0020,
    W0021,
    W0022,
    W0023,
    W0024,
    W0025,
    W0026,
    W0027,
    W0028,
    W0029,
    W0030,
    W0031,
    W0032,
    W0033,
    W0034,
    W0035,
    W0036,
    W0037,
    W0038,
    W0039,
    W0040,
}

impl DiagnosticCode {
    pub fn as_str(self) -> &'static str {
        match self {
            DiagnosticCode::E0001 => "E0001",
            DiagnosticCode::E0002 => "E0002",
            DiagnosticCode::E0003 => "E0003",
            DiagnosticCode::E0004 => "E0004",
            DiagnosticCode::E0005 => "E0005",
            DiagnosticCode::E0006 => "E0006",
            DiagnosticCode::E0007 => "E0007",
            DiagnosticCode::E0008 => "E0008",
            DiagnosticCode::E0009 => "E0009",
            DiagnosticCode::E0010 => "E0010",
            DiagnosticCode::E0011 => "E0011",
            DiagnosticCode::E0012 => "E0012",
            DiagnosticCode::E0013 => "E0013",
            DiagnosticCode::E0014 => "E0014",
            DiagnosticCode::E0015 => "E0015",
            DiagnosticCode::W0001 => "W0001",
            DiagnosticCode::W0002 => "W0002",
            DiagnosticCode::W0003 => "W0003",
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
            DiagnosticCode::W0015 => "W0015",
            DiagnosticCode::W0016 => "W0016",
            DiagnosticCode::W0017 => "W0017",
            DiagnosticCode::W0018 => "W0018",
            DiagnosticCode::W0019 => "W0019",
            DiagnosticCode::W0020 => "W0020",
            DiagnosticCode::W0021 => "W0021",
            DiagnosticCode::W0022 => "W0022",
            DiagnosticCode::W0023 => "W0023",
            DiagnosticCode::W0024 => "W0024",
            DiagnosticCode::W0025 => "W0025",
            DiagnosticCode::W0026 => "W0026",
            DiagnosticCode::W0027 => "W0027",
            DiagnosticCode::W0028 => "W0028",
            DiagnosticCode::W0029 => "W0029",
            DiagnosticCode::W0030 => "W0030",
            DiagnosticCode::W0031 => "W0031",
            DiagnosticCode::W0032 => "W0032",
            DiagnosticCode::W0033 => "W0033",
            DiagnosticCode::W0034 => "W0034",
            DiagnosticCode::W0035 => "W0035",
            DiagnosticCode::W0036 => "W0036",
            DiagnosticCode::W0037 => "W0037",
            DiagnosticCode::W0038 => "W0038",
            DiagnosticCode::W0039 => "W0039",
            DiagnosticCode::W0040 => "W0040",
        }
    }

    pub fn warning_category(self) -> Option<WarningCategory> {
        match self {
            DiagnosticCode::W0001 | DiagnosticCode::W0002 | DiagnosticCode::W0003 => {
                Some(WarningCategory::Unused)
            }
            DiagnosticCode::W0004
            | DiagnosticCode::W0005
            | DiagnosticCode::W0020
            | DiagnosticCode::W0021 => Some(WarningCategory::Type),
            DiagnosticCode::W0007
            | DiagnosticCode::W0008
            | DiagnosticCode::W0009
            | DiagnosticCode::W0027
            | DiagnosticCode::W0033 => Some(WarningCategory::Performance),
            DiagnosticCode::W0006
            | DiagnosticCode::W0012
            | DiagnosticCode::W0013
            | DiagnosticCode::W0014
            | DiagnosticCode::W0022
            | DiagnosticCode::W0023
            | DiagnosticCode::W0024
            | DiagnosticCode::W0026 => Some(WarningCategory::Style),
            DiagnosticCode::W0011 | DiagnosticCode::W0018 => Some(WarningCategory::Complexity),
            DiagnosticCode::W0015
            | DiagnosticCode::W0016
            | DiagnosticCode::W0017
            | DiagnosticCode::W0019
            | DiagnosticCode::W0028
            | DiagnosticCode::W0029
            | DiagnosticCode::W0030
            | DiagnosticCode::W0031
            | DiagnosticCode::W0032
            | DiagnosticCode::W0036
            | DiagnosticCode::W0038
            | DiagnosticCode::W0040 => Some(WarningCategory::Logic),
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
            DiagnosticCode::W0003 => "unused_imports",
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
            DiagnosticCode::W0015 => "W0015",
            DiagnosticCode::W0016 => "infinite_loop",
            DiagnosticCode::W0017 => "unreachable_loop",
            DiagnosticCode::W0018 => "deep_loop_nesting",
            DiagnosticCode::W0019 => "break_outside_loop",
            DiagnosticCode::W0020 => "unknown_function",
            DiagnosticCode::W0021 => "negative_index",
            DiagnosticCode::W0022 => "negative_literal",
            DiagnosticCode::W0023 => "W0023",
            DiagnosticCode::W0024 => "long_string_literal",
            DiagnosticCode::W0025 => "large_literal",
            DiagnosticCode::W0026 => "magic_number",
            DiagnosticCode::W0027 => "unnecessary_clone",
            DiagnosticCode::W0028 => "implicit_move",
            DiagnosticCode::W0029 => "implicit_copy",
            DiagnosticCode::W0030 => "implicit_drop",
            DiagnosticCode::W0031 => "unclear_ownership",
            DiagnosticCode::W0032 => "W0032",
            DiagnosticCode::W0033 => "W0033",
            DiagnosticCode::W0034 => "non_snake_case_variable",
            DiagnosticCode::W0035 => "non_snake_case_function",
            DiagnosticCode::W0036 => "self_comparison",
            DiagnosticCode::W0037 => "excessive_arguments",
            DiagnosticCode::W0038 => "duplicate_match_pattern",
            DiagnosticCode::W0039 => "variable_shadowing",
            DiagnosticCode::W0040 => "missing_explicit_return",
            _ => self.as_str(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Label {
    pub line: usize,
    pub column: usize,
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
    pub line: usize,
    pub column: usize,
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
        line: usize,
        column: usize,
    ) -> Self {
        Self {
            severity,
            code,
            title: title.into(),
            message: message.into(),
            line,
            column,
            labels: Vec::new(),
            notes: Vec::new(),
            help: None,
            suggestions: Vec::new(),
            source: None,
            filename: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum WarningFilter {
    Default,
    All,
    Codes(HashSet<DiagnosticCode>),
}

impl WarningFilter {
    pub fn default_codes() -> HashSet<DiagnosticCode> {
        [
            DiagnosticCode::W0001,
            DiagnosticCode::W0002,
            DiagnosticCode::W0003,
            DiagnosticCode::W0004,
            DiagnosticCode::W0005,
        ]
        .into_iter()
        .collect()
    }

    pub fn matches(&self, code: DiagnosticCode) -> bool {
        match self {
            WarningFilter::Default => Self::default_codes().contains(&code),
            WarningFilter::All => code.is_warning(),
            WarningFilter::Codes(codes) => codes.contains(&code),
        }
    }
}
