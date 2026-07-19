//! Módulo dedicado de tipos de Mire.
//!
//! Centraliza la teoría de tipos reales del lenguaje: mapeo a LLVM IR con anchos
//! reales (sin "maquillaje" sobre `i64`), utilidades de codegen, y construcción
//! de errores de tipado ultra-específicos.
//!
//! El enum `DataType` vive en `parser::ast` (usado por todo el parser); aquí se
//! re-exporta para que el resto del compilador pueda importar tipos desde un único
//! lugar sin romper los ~30 archivos que ya dependen de `parser::ast::DataType`.

pub use crate::parser::ast::DataType;

pub mod codegen;
pub mod errors;
#[cfg(test)]
pub mod tests;
pub mod unify;

pub use codegen::{llvm_elem_type_str, llvm_type_str, render_struct_llvm_type};
pub use unify::{
    is_assignable, is_bool_like, is_integer_type, is_numeric, literal_type, promote_numeric,
    resolve_binary_type, unify_types, validate_int_literal_range,
};
