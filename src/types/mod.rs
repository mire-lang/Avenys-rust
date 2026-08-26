//! Mire's dedicated types module.
//!
//! Centralizes the language's real type theory: mapping to LLVM IR with actual
//! widths (no `i64` masking), codegen utilities, and construction of
//! highly-specific typing errors.
//!
//! The `DataType` enum lives in `parser::ast` (used by the entire parser); it is
//! re-exported here so the rest of the compiler can import types from a single
//! location without breaking the ~30 files that already depend on
//! `parser::ast::DataType`.

pub use crate::parser::ast::DataType;

pub mod codegen;
pub mod errors;
#[cfg(test)]
pub mod tests;
pub mod unify;

pub use codegen::{llvm_elem_type_str, llvm_type_byte_size, llvm_type_str, render_struct_llvm_type};
pub use unify::{
    is_assignable, is_bool_like, is_integer_type, is_numeric, literal_type, promote_numeric,
    resolve_binary_type, unify_types, validate_int_literal_range,
};
