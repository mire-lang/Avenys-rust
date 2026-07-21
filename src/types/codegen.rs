//! Mapeo canónico de `DataType` -> representación LLVM IR.
//!
//! Regla de oro: el ancho en LLVM debe coincidir EXACTAMENTE con el ancho del tipo
//! Mire declarado. No hay "maquillaje" (p.ej. `u8` NUNCA se codegen como `i64`).
//! Si un tipo no tiene representación directa en LLVM, se documenta explícitamente.
//!
//! Tabla de verdad (auditable):
//! | Tipo Mire   | LLVM IR     | Ancho | Notas                              |
//! |-------------|-------------|-------|------------------------------------|
//! | `i8`        | `i8`        | 8b   |                                    |
//! | `i16`       | `i16`       | 16b  |                                    |
//! | `i32`       | `i32`       | 32b  |                                    |
//! | `i64`       | `i64`       | 64b  |                                    |
//! | `i128`      | `i128`      | 128b |                                    |
//! | `u8`        | `i8`        | 8b   | sin signo preservado en operaciones|
//! | `u16`       | `i16`       | 16b  |                                    |
//! | `u32`       | `i32`       | 32b  |                                    |
//! | `u64`       | `i64`       | 64b  |                                    |
//! | `u128`      | `i128`      | 128b |                                    |
//! | `f32`       | `float`     | 32b  |                                    |
//! | `f64`       | `double`    | 64b  |                                    |
//! | `bool`      | `i1`        | 1b   |                                    |
//! | `char`      | `i32`       | 32b  | codepoint UTF-32                   |
//! | `str`       | `ptr`       | ---  | puntero a buffer UTF-8 gestionado |

use crate::parser::ast::DataType;

/// Ancho en bits de un tipo entero/flotante Mire real (None para tipos no escalares).
#[allow(dead_code)]
pub(crate) fn integer_bit_width(dt: &DataType) -> Option<u32> {
    match dt {
        DataType::I8 | DataType::U8 => Some(8),
        DataType::I16 | DataType::U16 => Some(16),
        DataType::I32 | DataType::U32 => Some(32),
        DataType::I64 | DataType::U64 => Some(64),
        DataType::I128 | DataType::U128 => Some(128),
        DataType::Char => Some(32),
        DataType::F32 => Some(32),
        DataType::F64 => Some(64),
        DataType::Bool => Some(1),
        _ => None,
    }
}

/// ¿El tipo es de punto flotante?
#[allow(dead_code)]
pub(crate) fn is_float_type(dt: &DataType) -> bool {
    matches!(dt, DataType::F32 | DataType::F64)
}

/// Mapea un `DataType` a su tipo escalar LLVM IR (string).
///
/// Para tipos compuestos (`Array`, `Slice`, `Pointer`, `Struct`, `Closure`, etc.)
/// se usa el tipo correspondiente; los tipos de caja (managed) son punteros.
pub fn llvm_type_str(dt: &DataType) -> String {
    match dt {
        DataType::I8 => "i8".to_string(),
        DataType::I16 => "i16".to_string(),
        DataType::I32 => "i32".to_string(),
        DataType::I64 => "i64".to_string(),
        DataType::I128 => "i128".to_string(),
        DataType::U8 => "i8".to_string(),
        DataType::U16 => "i16".to_string(),
        DataType::U32 => "i32".to_string(),
        DataType::U64 => "i64".to_string(),
        DataType::U128 => "i128".to_string(),
        DataType::F32 => "float".to_string(),
        DataType::F64 => "double".to_string(),
        DataType::Bool => "i1".to_string(),
        DataType::Char => "i32".to_string(),
        DataType::None => "i64".to_string(),
        DataType::Array { element_type, size } => {
            format!("[{} x {}]", size, llvm_type_str(element_type))
        }
        DataType::Slice { element_type } => llvm_type_str(element_type),
        DataType::Pointer(_) => "ptr".to_string(),
        DataType::EnumNamed(_) => "i64".to_string(),
        DataType::Generic(_) => "i64".to_string(),
        DataType::Closure { .. } => "{ ptr, ptr }".to_string(),
        DataType::Function => "ptr".to_string(),
        DataType::Ref { .. } | DataType::RefMut { .. } => "ptr".to_string(),
        _ => "ptr".to_string(),
    }
}

/// Renderiza el tipo LLVM de un struct a partir de sus campos, respetando los
/// anchos reales de cada campo (p.ej. `{ i8, i8, i64 }` para `{ u8, i8, i64 }`).
pub fn render_struct_llvm_type(fields: &[(String, DataType)]) -> String {
    let tys: Vec<String> = fields.iter().map(|(_, dt)| llvm_type_str(dt)).collect();
    format!("{{ {} }}", tys.join(", "))
}

/// Tipo de elemento para GEP/codegen de colecciones, con anchos reales.
/// Debe coincidir con `llvm_type_str` para escalares.
pub fn llvm_type_byte_size(llvm_type: &str) -> i64 {
    match llvm_type {
        "i8" | "i1" => 1,
        "i16" => 2,
        "i32" | "float" => 4,
        "i64" | "double" | "ptr" | "i8*" => 8,
        "i128" => 16,
        _ => {
            if llvm_type.contains('*') {
                8
            } else {
                8
            }
        }
    }
}

pub fn llvm_elem_type_str(dt: &DataType) -> String {
    match dt {
        DataType::I8 => "i8".to_string(),
        DataType::I16 => "i16".to_string(),
        DataType::I32 => "i32".to_string(),
        DataType::I64 => "i64".to_string(),
        DataType::I128 => "i128".to_string(),
        DataType::U8 => "i8".to_string(),
        DataType::U16 => "i16".to_string(),
        DataType::U32 => "i32".to_string(),
        DataType::U64 => "i64".to_string(),
        DataType::U128 => "i128".to_string(),
        DataType::F32 => "float".to_string(),
        DataType::F64 => "double".to_string(),
        DataType::Bool => "i1".to_string(),
        DataType::Char => "i32".to_string(),
        DataType::None => "i64".to_string(),
        DataType::StructNamed(name) => format!("struct:{}", name),
        _ => "i64".to_string(),
    }
}
