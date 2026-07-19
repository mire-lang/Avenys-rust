//! Mapeo de tipos a LLVM IR.
//!
//! Delegado a `crate::types::codegen` (fuente de verdad, con anchos reales).
//! Mantenido por compatibilidad con call sites existentes.

pub(crate) use crate::types::codegen::{llvm_type_str, render_struct_llvm_type};
