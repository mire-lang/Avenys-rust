//! Type mapping to LLVM IR.
//!
//! Delegated to `crate::types::codegen` (source of truth, with actual widths).
//! Maintained for compatibility with existing call sites.

pub(crate) use crate::types::codegen::{llvm_type_str, render_struct_llvm_type};
