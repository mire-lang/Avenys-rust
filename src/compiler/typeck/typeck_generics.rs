use super::*;
use crate::error::type_error_at_span;

impl TypeChecker {
    pub(super) fn generic_bindings_from_args(
        &self,
        sig: &FunctionSig,
        resolved_type_args: &[DataType],
    ) -> HashMap<String, DataType> {
        let mut bindings = HashMap::new();
        for (idx, param_name) in sig.type_params.iter().enumerate() {
            if let Some(arg) = resolved_type_args.get(idx) {
                bindings.insert(param_name.clone(), arg.clone());
            }
        }
        bindings
    }

    pub(super) fn substitute_generics(
        &self,
        data_type: &DataType,
        bindings: &HashMap<String, DataType>,
    ) -> DataType {
        match data_type {
            DataType::Generic(name) => bindings.get(name).cloned().unwrap_or(DataType::Unknown),
            DataType::Vector {
                element_type,
                dynamic,
            } => DataType::Vector {
                element_type: Box::new(self.substitute_generics(element_type, bindings)),
                dynamic: *dynamic,
            },
            DataType::Map {
                key_type,
                value_type,
            } => DataType::Map {
                key_type: Box::new(self.substitute_generics(key_type, bindings)),
                value_type: Box::new(self.substitute_generics(value_type, bindings)),
            },
            DataType::Array { element_type, size } => DataType::Array {
                element_type: Box::new(self.substitute_generics(element_type, bindings)),
                size: *size,
            },
            DataType::Slice { element_type } => DataType::Slice {
                element_type: Box::new(self.substitute_generics(element_type, bindings)),
            },
            DataType::Ref { inner } => DataType::Ref {
                inner: Box::new(self.substitute_generics(inner, bindings)),
            },
            DataType::RefMut { inner } => DataType::RefMut {
                inner: Box::new(self.substitute_generics(inner, bindings)),
            },
            DataType::Result { ok, err } => DataType::Result {
                ok: Box::new(self.substitute_generics(ok, bindings)),
                err: Box::new(self.substitute_generics(err, bindings)),
            },
            other => other.clone(),
        }
    }

    pub(super) fn infer_generic_from_pair(
        &self,
        param_type: &DataType,
        arg_type: &DataType,
        inferred: &mut HashMap<String, DataType>,
    ) -> Result<()> {
        match (param_type, arg_type) {
            (DataType::Generic(name), actual) => {
                if let Some(existing) = inferred.get(name) {
                    if !self.is_assignable(existing, actual)
                        || !self.is_assignable(actual, existing)
                    {
                        return Err(type_error_at_span(
                            self.current_span,
                            format!(
                                "Conflicting inference for generic '{}': {:?} vs {:?}",
                                name, existing, actual
                            ),
                        ));
                    }
                } else {
                    inferred.insert(name.clone(), actual.clone());
                }
                Ok(())
            }
            (
                DataType::Vector {
                    element_type: a, ..
                },
                DataType::Vector {
                    element_type: b, ..
                },
            )
            | (
                DataType::Array {
                    element_type: a, ..
                },
                DataType::Array {
                    element_type: b, ..
                },
            )
            | (DataType::Slice { element_type: a }, DataType::Slice { element_type: b }) => {
                self.infer_generic_from_pair(a, b, inferred)
            }
            (
                DataType::Map {
                    key_type: ak,
                    value_type: av,
                },
                DataType::Map {
                    key_type: bk,
                    value_type: bv,
                },
            ) => {
                self.infer_generic_from_pair(ak, bk, inferred)?;
                self.infer_generic_from_pair(av, bv, inferred)
            }
            _ => Ok(()),
        }
    }

    pub(super) fn resolve_generic_type_args(
        &self,
        sig: &FunctionSig,
        explicit_type_args: &[DataType],
        arg_types: &[DataType],
    ) -> Result<Vec<DataType>> {
        if explicit_type_args.is_empty() {
            let mut inferred = HashMap::new();
            for (param, arg) in sig.params.iter().zip(arg_types.iter()) {
                self.infer_generic_from_pair(param, arg, &mut inferred)?;
            }
            let mut resolved = Vec::with_capacity(sig.type_params.len());
            for param in &sig.type_params {
                let inferred_type = inferred.get(param).cloned().ok_or_else(|| {
                    type_error_at_span(
                        self.current_span,
                        format!(
                            "Could not infer generic type '{}'; specify it explicitly",
                            param
                        ),
                    )
                })?;
                resolved.push(inferred_type);
            }
            return Ok(resolved);
        }

        if explicit_type_args.len() != sig.type_params.len() {
            return Err(type_error_at_span(
                self.current_span,
                format!(
                    "Function generic arity mismatch: expected {}, got {}",
                    sig.type_params.len(),
                    explicit_type_args.len()
                ),
            ));
        }
        Ok(explicit_type_args.to_vec())
    }

    pub(super) fn validate_generic_bounds(
        &self,
        fn_name: &str,
        sig: &FunctionSig,
        resolved_type_args: &[DataType],
    ) -> Result<()> {
        if sig.type_param_bounds.is_empty() {
            return Ok(());
        }
        let bindings = self.generic_bindings_from_args(sig, resolved_type_args);
        for (param, bounds) in &sig.type_param_bounds {
            let actual = bindings.get(param).cloned().unwrap_or(DataType::Unknown);
            for bound in bounds {
                if !self.traits.contains_key(bound) {
                    return Err(type_error_at_span(
                        self.current_span,
                        format!(
                            "Function '{}' generic bound refers to unknown trait '{}'",
                            fn_name, bound
                        ),
                    ));
                }
                let type_name = match &actual {
                    DataType::StructNamed(name) | DataType::EnumNamed(name) => {
                        Self::split_nominal_type_args(name).0.to_string()
                    }
                    _ => {
                        return Err(type_error_at_span(
                            self.current_span,
                            format!(
                                "Function '{}' requires '{}' to implement trait '{}'",
                                fn_name, param, bound
                            ),
                        ));
                    }
                };
                let ok = self
                    .impl_traits
                    .get(&type_name)
                    .is_some_and(|set| set.contains(bound));
                if !ok {
                    return Err(type_error_at_span(
                        self.current_span,
                        format!(
                            "Function '{}' requires '{}' to implement trait '{}'",
                            fn_name, param, bound
                        ),
                    ));
                }
            }
        }
        Ok(())
    }

    pub(super) fn validate_nominal_generic_bounds(
        &self,
        nominal_name: &str,
        bounds: &[(String, Vec<String>)],
        bindings: &HashMap<String, DataType>,
    ) -> Result<()> {
        for (param, trait_bounds) in bounds {
            let actual = bindings.get(param).cloned().unwrap_or(DataType::Unknown);
            for bound in trait_bounds {
                if !self.traits.contains_key(bound) {
                    return Err(type_error_at_span(
                        self.current_span,
                        format!(
                            "Type '{}' generic bound refers to unknown trait '{}'",
                            nominal_name, bound
                        ),
                    ));
                }
                let type_name = match &actual {
                    DataType::StructNamed(name) | DataType::EnumNamed(name) => {
                        Self::split_nominal_type_args(name).0.to_string()
                    }
                    _ => {
                        return Err(type_error_at_span(
                            self.current_span,
                            format!(
                                "Type '{}' requires '{}' to implement trait '{}'",
                                nominal_name, param, bound
                            ),
                        ));
                    }
                };
                let ok = self
                    .impl_traits
                    .get(&type_name)
                    .is_some_and(|set| set.contains(bound));
                if !ok {
                    return Err(type_error_at_span(
                        self.current_span,
                        format!(
                            "Type '{}' requires '{}' to implement trait '{}'",
                            nominal_name, param, bound
                        ),
                    ));
                }
            }
        }
        Ok(())
    }
}
