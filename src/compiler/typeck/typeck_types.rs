use super::*;
use crate::types::unify as ty;

impl TypeChecker {
    pub(super) fn resolve_binary_type(
        &self,
        operator: &str,
        left: &DataType,
        right: &DataType,
    ) -> Result<DataType> {
        ty::resolve_binary_type(operator, left, right)
    }

    #[allow(dead_code)]
    pub(super) fn is_integer_type(ty: &DataType) -> bool {
        ty::is_integer_type(ty)
    }

    pub(super) fn is_logical_operator(operator: &str) -> bool {
        ty::is_logical_operator(operator)
    }

    pub(super) fn is_match_identifier_pattern(expression: &Expression) -> bool {
        ty::is_match_identifier_pattern(expression)
    }

    pub(super) fn literal_type(lit: &Literal) -> DataType {
        ty::literal_type(lit)
    }

    pub(super) fn validate_int_literal_range(
        data_type: &DataType,
        value: i64,
        line: usize,
        column: usize,
    ) -> Result<()> {
        ty::validate_int_literal_range(data_type, value, line, column)
    }

    pub(super) fn unify_types(left: &DataType, right: &DataType) -> Result<DataType> {
        ty::unify_types(left, right)
    }

    #[allow(dead_code)]
    pub(super) fn promote_numeric(left: &DataType, right: &DataType) -> DataType {
        ty::promote_numeric(left, right)
    }

    pub(super) fn is_numeric(dtype: &DataType) -> bool {
        ty::is_numeric(dtype)
    }

    pub(super) fn is_bool_like(dtype: &DataType) -> bool {
        ty::is_bool_like(dtype)
    }

    pub(super) fn is_assignable(&self, expected: &DataType, actual: &DataType) -> bool {
        ty::is_assignable(expected, actual)
    }
}

// Re-exports the canonical codegen functions to maintain compatibility with
// call sites that use `crate::compiler::typeck::...` or direct imports.
