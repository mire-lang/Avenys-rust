//! Teoría de tipos de Mire: unificación, coerción, promoción, rangos y predicados.
//!
//! Este módulo es la FUENTE DE VERDAD de la teoría de tipos. El typechecker
//! (`compiler::typeck`) delega aquí. Las funciones son `pub(crate)` y NO dependen
//! de `TypeChecker`, de modo que pueden testearse de forma aislada (ver `tests`).

use crate::error::{Result, type_error};
use crate::parser::ast::{DataType, Expression, Literal};

/// Resuelve el tipo resultante de un operador binario.
///
/// Reglas:
/// - Aritmética entre numéricos → tipo promovido (ver [`promote_numeric`]).
/// - `+` entre `str` → `str` (concatenación).
/// - `+` entre vectores → vector unificado.
/// - Comparadores → `bool`.
/// - Lógicos (`&&`, `||`) → `bool`.
/// - Bitwise / shifts → tipo entero del operando izquierdo.
pub fn resolve_binary_type(operator: &str, left: &DataType, right: &DataType) -> Result<DataType> {
    match operator {
        "+" | "-" | "*" | "/" | "%" => {
            if operator == "+" && left == &DataType::Str && right == &DataType::Str {
                return Ok(DataType::Str);
            }

            if operator == "+" {
                match (left, right) {
                    (
                        DataType::Vector {
                            element_type: l_elem,
                            dynamic: l_dyn,
                        },
                        DataType::Vector {
                            element_type: r_elem,
                            dynamic: r_dyn,
                        },
                    ) => {
                        let unified_elem = unify_types(l_elem, r_elem)?;
                        return Ok(DataType::Vector {
                            element_type: Box::new(unified_elem),
                            dynamic: *l_dyn || *r_dyn,
                        });
                    }
                    (DataType::Vector { .. }, DataType::List)
                    | (DataType::List, DataType::Vector { .. })
                    | (DataType::List, DataType::List) => {
                        return Ok(DataType::Vector {
                            element_type: Box::new(DataType::Unknown),
                            dynamic: true,
                        });
                    }
                    _ => {}
                }
            }

            if is_numeric(left) && is_numeric(right) {
                return Ok(promote_numeric(left, right));
            }

            Err(type_error(
                0,
                0,
                format!(
                    "Operator '{}' not supported for {:?} and {:?}",
                    operator, left, right
                ),
            ))
        }
        "==" | "!=" | "<" | "<=" | ">" | ">=" => Ok(DataType::Bool),
        "&&" | "||" => {
            if left == &DataType::Unknown || right == &DataType::Unknown {
                return Ok(DataType::Bool);
            }
            if is_bool_like(left) && is_bool_like(right) {
                Ok(DataType::Bool)
            } else {
                Err(type_error(
                    0,
                    0,
                    format!(
                        "Logical operator '{}' requires bool operands, got {:?} and {:?}",
                        operator, left, right
                    ),
                ))
            }
        }
        "^" => {
            if left == &DataType::Unknown || right == &DataType::Unknown {
                return Ok(DataType::Unknown);
            }
            if is_bool_like(left) && is_bool_like(right) {
                Ok(DataType::Bool)
            } else if is_integer_type(left) && is_integer_type(right) {
                Ok(left.clone())
            } else {
                Err(type_error(
                    0,
                    0,
                    format!(
                        "XOR operator '^' requires either bool or integer operands, got {:?} and {:?}",
                        left, right
                    ),
                ))
            }
        }
        "&" | "|" | "<<" | ">>" => {
            if left == &DataType::Unknown || right == &DataType::Unknown {
                return Ok(DataType::Unknown);
            }
            if is_integer_type(left) && is_integer_type(right) {
                Ok(left.clone())
            } else {
                Err(type_error(
                    0,
                    0,
                    format!(
                        "Bitwise operator '{}' requires integer operands, got {:?} and {:?}",
                        operator, left, right
                    ),
                ))
            }
        }
        _ => Ok(DataType::Unknown),
    }
}

pub fn is_integer_type(ty: &DataType) -> bool {
    matches!(
        ty,
        DataType::I64
            | DataType::I32
            | DataType::I16
            | DataType::I8
            | DataType::I128
            | DataType::U64
            | DataType::U32
            | DataType::U16
            | DataType::U8
            | DataType::U128
    )
}

pub fn is_logical_operator(operator: &str) -> bool {
    matches!(operator, "&&" | "||")
}

pub fn is_match_identifier_pattern(expression: &Expression) -> bool {
    matches!(expression, Expression::Identifier(_))
}

pub fn literal_type(lit: &Literal) -> DataType {
    match lit {
        Literal::Int(_) => DataType::I64,
        Literal::Float(_) => DataType::F64,
        Literal::Char(_) => DataType::Char,
        Literal::Str(_) => DataType::Str,
        Literal::Bool(_) => DataType::Bool,
        Literal::None => DataType::None,
        Literal::List(_) => DataType::Vector {
            element_type: Box::new(DataType::Unknown),
            dynamic: false,
        },
        Literal::Dict(_) => DataType::Map {
            key_type: Box::new(DataType::Unknown),
            value_type: Box::new(DataType::Unknown),
        },
        Literal::Tuple(_) => DataType::Tuple,
    }
}

/// Valida que un literal entero cabe en el rango del tipo declarado.
pub fn validate_int_literal_range(
    data_type: &DataType,
    value: i64,
    line: usize,
    column: usize,
) -> Result<()> {
    let (min, max) = match data_type {
        DataType::I8 => (-128, 127),
        DataType::I16 => (-32768, 32767),
        DataType::I32 => (-2147483648, 2147483647),
        DataType::U8 => (0, 255),
        DataType::U16 => (0, 65535),
        DataType::U32 => (0, 4294967295),
        DataType::I128 | DataType::U128 => return Ok(()),
        _ => return Ok(()),
    };
    if !(min..=max).contains(&value) {
        return Err(crate::types::errors::literal_out_of_range(
            line, column, value, data_type, min, max,
        ));
    }
    Ok(())
}

/// Unifica dos tipos, devolviendo el tipo común o un error si son incompatibles.
pub fn unify_types(left: &DataType, right: &DataType) -> Result<DataType> {
    if left == right {
        return Ok(left.clone());
    }

    if left.is_struct_like() && right.is_struct_like() {
        return match (left.struct_name(), right.struct_name()) {
            (Some(left_name), Some(right_name)) if left_name != right_name => Err(type_error(
                0,
                0,
                format!(
                    "Cannot unify incompatible struct types {:?} and {:?}",
                    left, right
                ),
            )),
            (Some(_), _) => Ok(left.clone()),
            (_, Some(_)) => Ok(right.clone()),
            _ => Ok(DataType::Struct),
        };
    }

    if left.is_enum_like() && right.is_enum_like() {
        return match (left.enum_name(), right.enum_name()) {
            (Some(left_name), Some(right_name)) if left_name != right_name => Err(type_error(
                0,
                0,
                format!(
                    "Cannot unify incompatible enum types {:?} and {:?}",
                    left, right
                ),
            )),
            (Some(_), _) => Ok(left.clone()),
            (_, Some(_)) => Ok(right.clone()),
            _ => Ok(DataType::Enum),
        };
    }

    if left == &DataType::Unknown || left == &DataType::None {
        return Ok(right.clone());
    }
    if right == &DataType::Unknown || right == &DataType::None {
        return Ok(left.clone());
    }

    if is_numeric(left) && is_numeric(right) {
        return Ok(promote_numeric(left, right));
    }

    match (left, right) {
        (
            DataType::Vector {
                element_type: left_elem,
                dynamic: left_dynamic,
            },
            DataType::Vector {
                element_type: right_elem,
                dynamic: right_dynamic,
            },
        ) => {
            let element_type = unify_types(left_elem, right_elem)?;
            return Ok(DataType::Vector {
                element_type: Box::new(element_type),
                dynamic: *left_dynamic || *right_dynamic,
            });
        }
        (
            DataType::Map {
                key_type: left_key,
                value_type: left_value,
            },
            DataType::Map {
                key_type: right_key,
                value_type: right_value,
            },
        ) => {
            let key_type = unify_types(left_key, right_key)?;
            let value_type = unify_types(left_value, right_value)?;
            return Ok(DataType::Map {
                key_type: Box::new(key_type),
                value_type: Box::new(value_type),
            });
        }
        _ => {}
    }

    match (left, right) {
        (
            DataType::Result {
                ok: left_ok,
                err: left_err,
            },
            DataType::Result {
                ok: right_ok,
                err: right_err,
            },
        ) => {
            let ok = unify_types(left_ok, right_ok)?;
            let err = unify_types(left_err, right_err)?;
            return Ok(DataType::Result {
                ok: Box::new(ok),
                err: Box::new(err),
            });
        }
        (DataType::Result { ok, .. }, _) if *right == DataType::Unknown => {
            return Ok(DataType::Result {
                ok: ok.clone(),
                err: Box::new(DataType::Str),
            });
        }
        (_, DataType::Result { ok, .. }) if *left == DataType::Unknown => {
            return Ok(DataType::Result {
                ok: ok.clone(),
                err: Box::new(DataType::Str),
            });
        }
        (
            DataType::Ref { inner: left_inner } | DataType::RefMut { inner: left_inner },
            DataType::Ref { inner: right_inner } | DataType::RefMut { inner: right_inner },
        ) => {
            let inner = unify_types(left_inner, right_inner)?;
            let same_kind = std::mem::discriminant(left) == std::mem::discriminant(right);
            return Ok(if same_kind {
                if matches!(left, DataType::Ref { .. }) {
                    DataType::Ref {
                        inner: Box::new(inner),
                    }
                } else {
                    DataType::RefMut {
                        inner: Box::new(inner),
                    }
                }
            } else {
                DataType::Ref {
                    inner: Box::new(inner),
                }
            });
        }
        (DataType::Ref { inner } | DataType::RefMut { inner }, other)
        | (other, DataType::Ref { inner } | DataType::RefMut { inner }) => {
            return unify_types(inner, other);
        }
        _ => {}
    }

    Err(type_error(
        0,
        0,
        format!("Cannot unify incompatible types {:?} and {:?}", left, right),
    ))
}

/// Ancho en bits de un tipo entero/float Mire (utilidad compartida).
pub fn numeric_bit_width(dt: &DataType) -> Option<u32> {
    match dt {
        DataType::I8 | DataType::U8 => Some(8),
        DataType::I16 | DataType::U16 => Some(16),
        DataType::I32 | DataType::U32 => Some(32),
        DataType::I64 | DataType::U64 => Some(64),
        DataType::I128 | DataType::U128 => Some(128),
        DataType::F32 => Some(32),
        DataType::F64 => Some(64),
        DataType::Char => Some(32),
        _ => None,
    }
}

/// Promueve dos tipos numéricos al tipo común de mayor ancho (sin pérdida).
///
/// Reglas:
/// - Si alguno es `f64`, el resultado es `f64`.
/// - Si alguno es `f32` (y ninguno `f64`), el resultado es `f32`.
/// - Si ambos son enteros, el resultado es el entero de mayor ancho/rango.
///
/// Esta función SÓLO se usa cuando la promoción no pierde información (p.ej. en
/// operadores aritméticos). La asignación con pérdida de precisión es un error
/// aparte (ver `is_assignable`).
pub fn promote_numeric(left: &DataType, right: &DataType) -> DataType {
    use DataType::*;
    let is_float = |t: &DataType| matches!(t, F32 | F64);
    if is_float(left) || is_float(right) {
        if *left == F64 || *right == F64 {
            return F64;
        }
        return F32;
    }
    let rank = |t: &DataType| -> u32 {
        match t {
            I128 => 11,
            U128 => 10,
            I64 => 9,
            U64 => 8,
            I32 => 7,
            U32 => 6,
            I16 => 5,
            U16 => 4,
            I8 => 3,
            U8 => 2,
            Char => 3,
            _ => 0,
        }
    };
    if rank(left) >= rank(right) {
        left.clone()
    } else {
        right.clone()
    }
}

pub fn is_numeric(dtype: &DataType) -> bool {
    matches!(
        dtype,
        DataType::I8
            | DataType::I16
            | DataType::I32
            | DataType::I64
            | DataType::I128
            | DataType::U8
            | DataType::U16
            | DataType::U32
            | DataType::U64
            | DataType::U128
            | DataType::F32
            | DataType::F64
    )
}

pub fn is_bool_like(dtype: &DataType) -> bool {
    matches!(dtype, DataType::Bool | DataType::Anything | DataType::Unknown)
}

/// ¿La asignación `actual` a `expected` es válida?
///
/// Reglas de precisión (Fase 0): una asignación entre tipos numéricos de distinto
/// ancho/signo es válida SOLO si no hay pérdida de información (el tipo destino es
/// más ancho o igual en rango). Si hay pérdida (p.ej. `i64` -> `i8`, `f64` -> `f32`,
/// `i32` -> `u8`), se requiere una ascripción de tipo/conversión explícita; en caso
/// contrario es un error de typecheck (no un widening silencioso).
pub fn is_assignable(expected: &DataType, actual: &DataType) -> bool {
    if matches!(expected, DataType::Generic(_)) || matches!(actual, DataType::Generic(_)) {
        return true;
    }
    if expected == actual {
        return true;
    }

    if expected.is_struct_like() && actual.is_struct_like() {
        return match (expected.struct_name(), actual.struct_name()) {
            (Some(expected_name), Some(actual_name)) => expected_name == actual_name,
            _ => true,
        };
    }

    if expected.is_enum_like() && actual.is_enum_like() {
        return match (expected.enum_name(), actual.enum_name()) {
            (Some(expected_name), Some(actual_name)) => expected_name == actual_name,
            _ => true,
        };
    }

    match (expected, actual) {
        (
            DataType::Ref {
                inner: expected_inner,
            },
            DataType::Ref {
                inner: actual_inner,
            }
            | DataType::RefMut {
                inner: actual_inner,
            },
        ) => {
            return is_assignable(expected_inner, actual_inner);
        }
        (
            DataType::RefMut {
                inner: expected_inner,
            },
            DataType::RefMut {
                inner: actual_inner,
            },
        ) => {
            return is_assignable(expected_inner, actual_inner);
        }
        (DataType::RefMut { .. }, DataType::Ref { .. }) => return false,
        (DataType::Ref { inner, .. } | DataType::RefMut { inner, .. }, _) => {
            return is_assignable(inner, actual);
        }
        _ => {}
    }

    if expected == &DataType::None {
        return true;
    }
    if expected == &DataType::Anything || actual == &DataType::Unknown {
        return true;
    }

    if expected == &DataType::Unknown {
        return true;
    }

    if expected == &DataType::Dict && actual == &DataType::List {
        return true;
    }

    if matches!(expected, DataType::Map { .. }) && actual == &DataType::Dict {
        return true;
    }

    match (expected, actual) {
        (
            DataType::Result {
                ok: expected_ok,
                err: expected_err,
            },
            DataType::Result {
                ok: actual_ok,
                err: actual_err,
            },
        ) => {
            return is_assignable(expected_ok, actual_ok) && is_assignable(expected_err, actual_err);
        }
        (
            DataType::Array {
                element_type: expected_elem,
                ..
            }
            | DataType::Slice {
                element_type: expected_elem,
            },
            DataType::Vector {
                element_type: actual_elem,
                ..
            },
        ) => {
            return is_assignable(expected_elem, actual_elem);
        }
        (DataType::Array { .. } | DataType::Slice { .. }, DataType::List) => return true,
        (
            DataType::Vector {
                element_type: expected_elem,
                ..
            },
            DataType::Vector {
                element_type: actual_elem,
                ..
            },
        ) => {
            return is_assignable(expected_elem, actual_elem);
        }
        (DataType::Vector { .. }, DataType::List) => return true,
        _ => {}
    }

    // Asignación numérica: solo válida sin pérdida de precisión.
    if is_numeric(expected) && is_numeric(actual) {
        return numeric_bit_width(expected) >= numeric_bit_width(actual)
            && float_to_int_preserves_sign(expected, actual);
    }

    false
}

/// Comprueba que al asignar no se cruza la frontera signed/unsigned de forma
/// pérdida (p.ej. `u64` -> `i64` es válido por ancho, pero `u64` -> `i32` no).
fn float_to_int_preserves_sign(expected: &DataType, actual: &DataType) -> bool {
    use DataType::*;
    let is_signed = |t: &DataType| matches!(t, I8 | I16 | I32 | I64 | I128 | F32 | F64 | Char);
    let is_unsigned = |t: &DataType| matches!(t, U8 | U16 | U32 | U64 | U128);
    match (expected, actual) {
        // float -> int siempre requiere conversión explícita (pérdida de fracción).
        (I8 | I16 | I32 | I64 | I128 | U8 | U16 | U32 | U64 | U128, F32 | F64) => false,
        // int -> float es seguro solo si el float es más ancho (f64 cubre i64/u64).
        (F32 | F64, I8 | I16 | I32 | I64 | I128 | U8 | U16 | U32 | U64 | U128) => {
            numeric_bit_width(expected) >= numeric_bit_width(actual)
        }
        // signed -> unsigned de igual ancho: válido (la representación bit a bit es la misma).
        (e, a) if is_unsigned(e) && is_signed(a) && numeric_bit_width(e) == numeric_bit_width(a) => {
            true
        }
        // unsigned -> signed de igual ancho: válido por ancho.
        (e, a) if is_signed(e) && is_unsigned(a) && numeric_bit_width(e) == numeric_bit_width(a) => {
            true
        }
        _ => true,
    }
}
