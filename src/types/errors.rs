//! Highly-specific typing error constructors.
//!
//! Each function builds a `MireError` with a dedicated diagnostic code,
//! clear message, exact location (line/column injected by the typechecker),
//! and useful notes (not generic garbage) with a correction suggestion when
//! applicable.
//!
//! Rule: minimum two codes per type family to cover the two failure modes
//! (assignment with precision loss, and literal out of range / incompatible type).
//! The codes live in `crate::error::diagnostic::DiagnosticCode`.

use crate::error::{DiagnosticCode, MireError, type_error_code};
use crate::parser::ast::DataType;

/// Assignment with precision loss (e.g. `i64` -> `i8`, `f64` -> `f32`).
pub fn precision_loss(
    line: usize,
    column: usize,
    expected: &DataType,
    actual: &DataType,
    suggestion: Option<&str>,
) -> MireError {
    let msg = format!(
        "implicit conversion from `{}` to `{}` loses precision; an explicit cast is required",
        pretty(actual),
        pretty(expected)
    );
    let mut err = type_error_code(line, column, DiagnosticCode::E0100, msg).with_explanation(format!(
        "Mire does not perform silent widening/narrowing. Use an explicit type ascription \
         (e.g. `(value :{} )`) or convert the value first.",
        pretty(expected)
    ));
    if let Some(s) = suggestion {
        err = err.with_suggestion("suggested fix".to_string(), Some(s.to_string()));
    }
    err
}

/// Literal out of range of the declared type (e.g. `300 :i8`).
pub fn literal_out_of_range(
    line: usize,
    column: usize,
    value: i64,
    target: &DataType,
    min: i64,
    max: i64,
) -> MireError {
    type_error_code(
        line,
        column,
        DiagnosticCode::E0107,
        format!(
            "integer literal `{}` does not fit in `{}` (valid range {}..={})",
            value,
            pretty(target),
            min,
            max
        ),
    )
    .with_explanation(format!(
        "Choose a value within {}..={}, or use a wider integer type such as `i64` or `i128`.",
        min, max
    ))
}

/// Incompatible type in assignment/operation (non-numeric, non-unifiable).
pub fn type_mismatch(
    line: usize,
    column: usize,
    expected: &DataType,
    actual: &DataType,
    context: &str,
) -> MireError {
    type_error_code(
        line,
        column,
        DiagnosticCode::E0101,
        format!(
            "expected `{}`, found `{}` ({})",
            pretty(expected),
            pretty(actual),
            context
        ),
    )
    .with_explanation(format!(
        "The declared type is `{}` but the expression produced `{}`.",
        pretty(expected),
        pretty(actual)
    ))
}

/// Float to integer conversion without ascription (fractional loss).
pub fn float_to_int_requires_cast(
    line: usize,
    column: usize,
    expected: &DataType,
    actual: &DataType,
) -> MireError {
    type_error_code(
        line,
        column,
        DiagnosticCode::E0102,
        format!(
            "cannot implicitly convert `{}` to `{}` (fractional part would be discarded)",
            pretty(actual),
            pretty(expected)
        ),
    )
    .with_explanation(format!(
        "Use an explicit cast `(value :{})` if truncation is intended.",
        pretty(expected)
    ))
}

/// Signed/unsigned boundary crossing that requires explicit ascription.
pub fn sign_mismatch(
    line: usize,
    column: usize,
    expected: &DataType,
    actual: &DataType,
) -> MireError {
    type_error_code(
        line,
        column,
        DiagnosticCode::E0103,
        format!(
            "cannot implicitly convert `{}` (signed/unsigned mismatch) to `{}`",
            pretty(actual),
            pretty(expected)
        ),
    )
    .with_explanation(
        "Mire preserves signedness. Use an explicit cast if the bit reinterpretation is intended."
            .to_string(),
    )
}

/// Human-readable type name for error messages.
pub fn pretty(dt: &DataType) -> String {
    match dt {
        DataType::I8 => "i8".into(),
        DataType::I16 => "i16".into(),
        DataType::I32 => "i32".into(),
        DataType::I64 => "i64".into(),
        DataType::I128 => "i128".into(),
        DataType::U8 => "u8".into(),
        DataType::U16 => "u16".into(),
        DataType::U32 => "u32".into(),
        DataType::U64 => "u64".into(),
        DataType::U128 => "u128".into(),
        DataType::F32 => "f32".into(),
        DataType::F64 => "f64".into(),
        DataType::Bool => "bool".into(),
        DataType::Char => "char".into(),
        DataType::Str => "str".into(),
        other => format!("{:?}", other),
    }
}

/// Real type error codes (Phase 0). Registered in `DiagnosticCode`.
pub const TYPE_ERROR_CODES: &[DiagnosticCode] = &[
    DiagnosticCode::E0100,
    DiagnosticCode::E0101,
    DiagnosticCode::E0102,
    DiagnosticCode::E0103,
    DiagnosticCode::E0104,
    DiagnosticCode::E0105,
    DiagnosticCode::E0106,
    DiagnosticCode::E0107,
    DiagnosticCode::E0108,
    DiagnosticCode::E0109,
    DiagnosticCode::E0110,
];
