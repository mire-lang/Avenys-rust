//! Constructores de errores de tipado ultra-específicos.
//!
//! Cada función construye un `MireError` con código de diagnóstico dedicado,
//! mensaje claro, ubicación exacta (línea/columna inyectada por el typechecker)
//! y notas útiles (no basura genérica) con una sugerencia de corrección cuando
//! aplica.
//!
//! Regla: mínimo dos códigos por familia de tipo para cubrir los dos modos de
//! fallo (asignación con pérdida de precisión, y literal fuera de rango / tipo
//! incompatible). Los códigos viven en `crate::error::diagnostic::DiagnosticCode`.

use crate::error::{DiagnosticCode, MireError, type_error_code};
use crate::parser::ast::DataType;

/// Asignación con pérdida de precisión (p.ej. `i64` -> `i8`, `f64` -> `f32`).
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

/// Literal fuera del rango del tipo declarado (p.ej. `300 :i8`).
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

/// Tipo incompatible en asignación/operación (no numérico, no unificable).
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

/// Conversión de flotante a entero sin ascripción (pérdida de fracción).
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

/// Cruce de frontera signed/unsigned que requiere ascripción explícita.
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

/// Nombre legible de un tipo para mensajes de error.
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

/// Códigos de error de tipos reales (Fase 0). Registrados en `DiagnosticCode`.
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
