use super::*;
use crate::error::type_error_at_span;

impl TypeChecker {
    pub(super) fn validate_explicit_nested_literal(
        expected: &DataType,
        expr: &Expression,
    ) -> Result<()> {
        match (expected, expr) {
            (
                DataType::Vector { element_type, .. } | DataType::Array { element_type, .. },
                Expression::List { elements, .. },
            ) => {
                if Self::requires_explicit_nested_element(element_type) {
                    for element in elements {
                        if !matches!(
                            element,
                            Expression::List { .. }
                                | Expression::Dict { .. }
                                | Expression::Identifier(_)
                        ) {
                            return Err(type_error(
                                0,
                                0,
                                format!(
                                    "Nested literal for {:?} must use explicit inner brackets",
                                    expected
                                ),
                            ));
                        }
                    }
                }
                for element in elements {
                    Self::validate_explicit_nested_literal(element_type, element)?;
                }
                Ok(())
            }
            (DataType::Map { value_type, .. }, Expression::Dict { entries, .. }) => {
                if Self::requires_explicit_nested_element(value_type) {
                    for (_, value) in entries {
                        if !matches!(value, Expression::List { .. } | Expression::Dict { .. }) {
                            return Err(type_error(
                                0,
                                0,
                                format!(
                                    "Nested literal for {:?} must use explicit inner brackets",
                                    expected
                                ),
                            ));
                        }
                    }
                }
                for (_, value) in entries {
                    Self::validate_explicit_nested_literal(value_type, value)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    pub(super) fn validate_trait_impl(
        &self,
        trait_name: &str,
        type_name: &str,
        methods: &[Statement],
    ) -> Result<()> {
        let trait_sig = self
            .traits
            .get(trait_name)
            .ok_or_else(|| type_error(0, 0, format!("Unknown skill/trait '{}'", trait_name)))?;

        for required_method in &trait_sig.methods {
            let implemented = methods.iter().find_map(|statement| match statement {
                Statement::Function {
                    name,
                    params,
                    return_type,
                    ..
                } if name == &required_method.name => Some((params.clone(), return_type.clone())),
                _ => None,
            });

            let Some((implemented_params, implemented_return)) = implemented else {
                return Err(type_error_at_span(
                    self.current_span,
                    format!(
                        "Type '{}' does not implement required method '{}.{}'",
                        type_name, trait_name, required_method.name
                    ),
                ));
            };

            let required_kind = Self::method_kind_for_params(&required_method.params);
            let implemented_kind = Self::method_kind_for_params(&implemented_params);
            if required_kind != implemented_kind {
                return Err(type_error_at_span(
                    self.current_span,
                    format!(
                        "Method '{}.{}' must be implemented as {}, got {}",
                        trait_name,
                        required_method.name,
                        Self::describe_method_kind(required_kind),
                        Self::describe_method_kind(implemented_kind),
                    ),
                ));
            }

            let required_params =
                Self::normalize_trait_impl_params(type_name, &required_method.params);
            let implemented_params =
                Self::normalize_trait_impl_params(type_name, &implemented_params);

            if implemented_params != required_params
                || implemented_return != required_method.return_type
            {
                return Err(type_error_at_span(
                    self.current_span,
                    format!(
                        "Method '{}.{}' implementation signature does not match declaration: expected {:?} -> {:?}, got {:?} -> {:?}",
                        trait_name,
                        required_method.name,
                        required_params,
                        required_method.return_type,
                        implemented_params,
                        implemented_return,
                    ),
                ));
            }
        }

        Ok(())
    }

    pub(super) fn validate_trait_method_declarations(
        &self,
        container_name: &str,
        methods: &[TraitMethodSig],
        container_kind: &str,
    ) -> Result<()> {
        for method in methods {
            Self::validate_self_param_position(
                &method.params,
                format!("{} '{}.{}'", container_kind, container_name, method.name),
            )?;
        }
        Ok(())
    }

    pub(super) fn requires_explicit_nested_element(dtype: &DataType) -> bool {
        matches!(
            dtype,
            DataType::Vector { .. } | DataType::Array { .. } | DataType::Map { .. }
        )
    }
    pub(super) fn validate_impl_method_declarations(
        &self,
        type_name: &str,
        methods: &[Statement],
    ) -> Result<()> {
        for method in methods {
            if let Statement::Function { name, params, .. } = method {
                Self::validate_self_param_position(
                    params,
                    format!("Method '{}.{}'", type_name, name),
                )?;
            }
        }
        Ok(())
    }

    pub(super) fn normalize_trait_impl_params(
        owner_type_name: &str,
        params: &[(String, DataType)],
    ) -> Vec<DataType> {
        params
            .iter()
            .map(|(name, data_type)| {
                if name == "self" && matches!(data_type, DataType::Unknown | DataType::Struct) {
                    DataType::StructNamed(owner_type_name.to_string())
                } else {
                    data_type.clone()
                }
            })
            .collect()
    }

    pub(super) fn validate_self_param_position(
        params: &[(String, DataType)],
        context: String,
    ) -> Result<()> {
        if params.iter().skip(1).any(|(name, _)| name == "self") {
            return Err(type_error(
                0,
                0,
                format!("{} must declare 'self' as the first parameter", context),
            ));
        }
        Ok(())
    }

    pub(super) fn method_kind_for_params(params: &[(String, DataType)]) -> MethodKind {
        if params.first().is_some_and(|(name, _)| name == "self") {
            MethodKind::Instance
        } else {
            MethodKind::Associated
        }
    }

    pub(super) fn describe_method_kind(kind: MethodKind) -> &'static str {
        match kind {
            MethodKind::Instance => "an instance method",
            MethodKind::Associated => "an associated method",
        }
    }
}
