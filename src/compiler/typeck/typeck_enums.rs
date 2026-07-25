use super::*;
use crate::error::type_error_at_span;

impl TypeChecker {
    pub(super) fn check_enum_variant_call(
        &self,
        variant_name: &str,
        variant_sig: &EnumVariantSig,
        arg_types: &[DataType],
    ) -> Result<()> {
        if variant_sig.payload_types.len() != arg_types.len() {
            return Err(type_error_at_span(
                self.current_span,
                format!(
                    "Enum variant '{}' expects {} values, got {}",
                    variant_name,
                    variant_sig.payload_types.len(),
                    arg_types.len()
                ),
            ));
        }

        for (index, (expected, actual)) in variant_sig
            .payload_types
            .iter()
            .zip(arg_types.iter())
            .enumerate()
        {
            if !self.is_assignable(expected, actual) {
                return Err(type_error_at_span(
                    self.current_span,
                    format!(
                        "Enum variant '{}' value {} expects {:?}, got {:?}",
                        variant_name,
                        index + 1,
                        expected,
                        actual
                    ),
                ));
            }
        }

        Ok(())
    }

    pub(super) fn check_enum_variant_call_with_bindings(
        &self,
        variant_name: &str,
        variant_sig: &EnumVariantSig,
        bindings: &HashMap<String, DataType>,
        arg_types: &[DataType],
    ) -> Result<()> {
        if variant_sig.payload_types.len() != arg_types.len() {
            return Err(type_error_at_span(
                self.current_span,
                format!(
                    "Enum variant '{}' expects {} values, got {}",
                    variant_name,
                    variant_sig.payload_types.len(),
                    arg_types.len()
                ),
            ));
        }
        for (index, (expected, actual)) in variant_sig
            .payload_types
            .iter()
            .map(|ty| self.substitute_generics(ty, bindings))
            .zip(arg_types.iter())
            .enumerate()
        {
            if !self.is_assignable(&expected, actual) {
                return Err(type_error_at_span(
                    self.current_span,
                    format!(
                        "Enum variant '{}' value {} expects {:?}, got {:?}",
                        variant_name,
                        index + 1,
                        expected,
                        actual
                    ),
                ));
            }
        }
        Ok(())
    }

    pub(super) fn normalize_enum_variant_payloads(
        &mut self,
        variant_name: &str,
        variant_sig: &EnumVariantSig,
        payloads: &mut Vec<Expression>,
    ) -> Result<Vec<DataType>> {
        let has_named = payloads
            .iter()
            .any(|arg| matches!(arg, Expression::NamedArg { .. }));
        let has_positional = payloads
            .iter()
            .any(|arg| !matches!(arg, Expression::NamedArg { .. }));

        if has_named && has_positional {
            return Err(type_error_at_span(
                self.current_span,
                format!(
                    "Enum variant '{}' cannot mix named and positional arguments",
                    variant_name
                ),
            ));
        }

        if !has_named {
            let mut arg_types = Vec::with_capacity(payloads.len());
            for payload in payloads.iter_mut() {
                arg_types.push(self.check_expression(payload)?);
            }
            self.check_enum_variant_call(variant_name, variant_sig, &arg_types)?;
            return Ok(arg_types);
        }

        let mut seen = HashSet::new();
        let mut named_values: HashMap<String, Expression> = HashMap::new();

        for payload in std::mem::take(payloads) {
            let Expression::NamedArg { name, value, .. } = payload else {
                unreachable!("named enum payload validation should reject mixed arguments");
            };

            if !seen.insert(name.clone()) {
                return Err(type_error_at_span(
                    self.current_span,
                    format!(
                        "Enum variant '{}' received duplicate field '{}'",
                        variant_name, name
                    ),
                ));
            }

            if !variant_sig.payload_names.iter().any(|field| field == &name) {
                return Err(type_error_at_span(
                    self.current_span,
                    format!("Enum variant '{}' has no field '{}'", variant_name, name),
                ));
            }

            named_values.insert(name, *value);
        }

        for field in &variant_sig.payload_names {
            if !named_values.contains_key(field) {
                return Err(type_error_at_span(
                    self.current_span,
                    format!(
                        "Enum variant '{}' is missing required field '{}'",
                        variant_name, field
                    ),
                ));
            }
        }

        let mut reordered_payloads = Vec::with_capacity(variant_sig.payload_names.len());
        let mut arg_types = Vec::with_capacity(variant_sig.payload_names.len());
        for field in &variant_sig.payload_names {
            let mut value = named_values
                .remove(field)
                .expect("enum payload field validated before reorder");
            let value_type = self.check_expression(&mut value)?;
            reordered_payloads.push(value);
            arg_types.push(value_type);
        }

        self.check_enum_variant_call(variant_name, variant_sig, &arg_types)?;
        *payloads = reordered_payloads;
        Ok(arg_types)
    }

    pub(super) fn check_match_pattern(&mut self, pattern: &mut Expression) -> Result<DataType> {
        match pattern {
            Expression::Call { name, args, .. } if name == "__match_guard" => {
                if args.len() != 2 {
                    return Err(type_error_at_span(
                        self.current_span,
                        "__match_guard expects pattern and guard".to_string(),
                    ));
                }
                let pattern_type = self.check_match_pattern(&mut args[0])?;
                let guard_type = self.check_expression(&mut args[1])?;
                if guard_type != DataType::Bool && guard_type != DataType::Unknown {
                    return Err(type_error_at_span(
                        self.current_span,
                        format!("match guard must be bool, got {:?}", guard_type),
                    ));
                }
                Ok(pattern_type)
            }
            Expression::Call { name, args, .. } if name == "__match_or" => {
                if args.len() != 2 {
                    return Err(type_error_at_span(
                        self.current_span,
                        "__match_or expects two patterns".to_string(),
                    ));
                }
                let left = self.check_match_pattern(&mut args[0])?;
                let right = self.check_match_pattern(&mut args[1])?;
                Self::unify_types(&left, &right)
            }
            Expression::Call { name, args, .. } if name == "__match_range" => {
                if args.len() != 2 {
                    return Err(type_error_at_span(
                        self.current_span,
                        "__match_range expects start and end".to_string(),
                    ));
                }
                let start_ty = self.check_expression(&mut args[0])?;
                let end_ty = self.check_expression(&mut args[1])?;
                if !Self::is_numeric(&start_ty) || !Self::is_numeric(&end_ty) {
                    return Err(type_error_at_span(
                        self.current_span,
                        format!(
                            "match range bounds must be numeric, got {:?} and {:?}",
                            start_ty, end_ty
                        ),
                    ));
                }
                Self::unify_types(&start_ty, &end_ty)
            }
            Expression::EnumVariantPath {
                enum_name,
                variant_name,
                data_type,
                ..
            } => {
                let full_name = format!("{}.{}", enum_name, variant_name);
                if !self.enum_variants.contains_key(&full_name) {
                    return Err(type_error_at_span(
                        self.current_span,
                        format!("Unknown enum variant '{}'", full_name),
                    ));
                }
                *data_type = DataType::EnumNamed(enum_name.clone());
                Ok(DataType::EnumNamed(enum_name.clone()))
            }
            Expression::EnumVariant {
                enum_name,
                variant_name,
                payloads,
                data_type,
                ..
            } => {
                let full_name =
                    Self::canonical_enum_variant_name(&format!("{}.{}", enum_name, variant_name));
                let variant_sig = self.enum_variants.get(&full_name).cloned().ok_or_else(|| {
                    type_error_at_span(
                        self.current_span,
                        format!("Unknown enum variant '{}'", full_name),
                    )
                })?;
                let (_, call_type_args) = Self::split_nominal_type_args(enum_name);
                let bindings =
                    self.bindings_for_nominal_type_args(&variant_sig.type_params, &call_type_args)?;
                let mut arg_types = Vec::with_capacity(payloads.len());
                for (index, payload) in payloads.iter_mut().enumerate() {
                    if matches!(payload, Expression::Identifier(_)) {
                        arg_types.push(
                            self.substitute_generics(
                                variant_sig
                                    .payload_types
                                    .get(index)
                                    .cloned()
                                    .as_ref()
                                    .unwrap_or(&DataType::Unknown),
                                &bindings,
                            ),
                        );
                    } else {
                        arg_types.push(self.check_expression(payload)?);
                    }
                }
                self.check_enum_variant_call_with_bindings(
                    &full_name,
                    &variant_sig,
                    &bindings,
                    &arg_types,
                )?;
                *data_type = DataType::EnumNamed(enum_name.clone());
                Ok(DataType::EnumNamed(enum_name.clone()))
            }
            _ => self.check_expression(pattern),
        }
    }

    pub(super) fn insert_match_pattern_bindings(&mut self, case_expr: &Expression) {
        match case_expr {
            Expression::EnumVariant {
                enum_name,
                variant_name,
                payloads,
                ..
            } => {
                let full_name =
                    Self::canonical_enum_variant_name(&format!("{}.{}", enum_name, variant_name));
                if let Some(variant_sig) = self.enum_variants.get(&full_name).cloned() {
                    let (_, call_type_args) = Self::split_nominal_type_args(enum_name);
                    let bindings = self
                        .bindings_for_nominal_type_args(&variant_sig.type_params, &call_type_args)
                        .unwrap_or_default();
                    for (payload_expr, payload_type) in
                        payloads.iter().zip(variant_sig.payload_types.iter())
                    {
                        if let Expression::Identifier(id) = payload_expr {
                            self.insert_var(
                                id.name.clone(),
                                self.substitute_generics(payload_type, &bindings),
                                true,
                            );
                        }
                    }
                }
            }
            Expression::Call { name, args, .. } if name == "__match_guard" => {
                if let Some(inner) = args.first() {
                    self.insert_match_pattern_bindings(inner);
                }
            }
            Expression::Call { name, args, .. } if name == "__match_or" => {
                if let Some(inner) = args.first() {
                    self.insert_match_pattern_bindings(inner);
                }
            }
            _ => {}
        }
    }
}
