use super::*;
use crate::error::type_error_at_span;

impl TypeChecker {
    fn all_fields<'a>(&'a self, class_sig: &'a ClassSig) -> Vec<&'a ClassFieldSig> {
        let mut fields: Vec<&ClassFieldSig> = Vec::new();
        if let Some(parent_name) = &class_sig.parent {
            if let Some(parent_sig) = self.classes.get(parent_name) {
                fields.extend(self.all_fields(parent_sig));
            }
        }
        for child_field in &class_sig.fields {
            if !fields.iter().any(|f| f.name == child_field.name) {
                fields.push(child_field);
            }
        }
        fields
    }

    pub(super) fn check_class_constructor_call_with_bindings(
        &self,
        class_name: &str,
        class_sig: &ClassSig,
        bindings: &HashMap<String, DataType>,
        args: &[Expression],
        arg_types: &[DataType],
    ) -> Result<()> {
        let all_fields = self.all_fields(class_sig);
        let has_named = args
            .iter()
            .any(|arg| matches!(arg, Expression::NamedArg { .. }));
        let has_positional = args
            .iter()
            .any(|arg| !matches!(arg, Expression::NamedArg { .. }));

        if has_named && has_positional {
            return Err(type_error_at_span(
                self.current_span,
                format!(
                    "Constructor '{}' cannot mix named and positional arguments",
                    class_name
                ),
            ));
        }

        if has_named {
            let mut seen = HashSet::new();
            for (index, arg) in args.iter().enumerate() {
                let Expression::NamedArg { name, .. } = arg else {
                    continue;
                };

                if !seen.insert(name.clone()) {
                    return Err(type_error_at_span(
                        self.current_span,
                        format!(
                            "Constructor '{}' received duplicate field '{}'",
                            class_name, name
                        ),
                    ));
                }

                let field = all_fields
                    .iter()
                    .find(|field| field.name == *name)
                    .ok_or_else(|| {
                        type_error_at_span(
                            self.current_span,
                            format!("Constructor '{}' has no field '{}'", class_name, name),
                        )
                    })?;

                let actual = arg_types.get(index).cloned().unwrap_or(DataType::Unknown);
                let expected = self.substitute_generics(&field.data_type, bindings);
                if !self.is_assignable(&expected, &actual) {
                    return Err(type_error_at_span(
                        self.current_span,
                        format!(
                            "Constructor '{}.{}' expects {:?}, got {:?}",
                            class_name, name, expected, actual
                        ),
                    ));
                }
            }

            for field in &all_fields {
                if !field.has_default && !seen.contains(&field.name) {
                    return Err(type_error_at_span(
                        self.current_span,
                        format!(
                            "Constructor '{}' is missing required field '{}'",
                            class_name, field.name
                        ),
                    ));
                }
            }
        } else {
            if arg_types.len() > all_fields.len() {
                return Err(type_error_at_span(
                    self.current_span,
                    format!(
                        "Constructor '{}' expects at most {} values, got {}",
                        class_name,
                        all_fields.len(),
                        arg_types.len()
                    ),
                ));
            }

            for (index, actual) in arg_types.iter().enumerate() {
                let Some(field) = all_fields.get(index) else {
                    break;
                };
                let expected = self.substitute_generics(&field.data_type, bindings);
                if !self.is_assignable(&expected, actual) {
                    return Err(type_error_at_span(
                        self.current_span,
                        format!(
                            "Constructor '{}.{}' expects {:?}, got {:?}",
                            class_name, field.name, expected, actual
                        ),
                    ));
                }
            }

            for field in all_fields.iter().skip(arg_types.len()) {
                if !field.has_default {
                    return Err(type_error_at_span(
                        self.current_span,
                        format!(
                            "Constructor '{}' is missing required field '{}'",
                            class_name, field.name
                        ),
                    ));
                }
            }
        }

        Ok(())
    }

    pub(super) fn resolve_pipeline_stage_type(
        &mut self,
        stage: &mut Expression,
        input_type: &DataType,
    ) -> Result<Option<DataType>> {
        match stage {
            Expression::Call {
                name,
                args,
                type_args: _,
                name_line: _,
                name_column: _,
                data_type,
            } => {
                let arg_types: Vec<DataType> = std::iter::once(Ok(input_type.clone()))
                    .chain(args.iter_mut().map(|arg| self.check_expression(arg)))
                    .collect::<Result<_>>()?;
                if name == "len" {
                    *data_type = DataType::I64;
                    return Ok(Some(DataType::I64));
                }
                if let Some(resolved) = self.resolve_instance_method_call(name, &arg_types[1..])? {
                    *data_type = resolved.clone();
                    return Ok(Some(resolved));
                }
                if let Some(sig) = self.functions.get(name).cloned()
                    && sig.params.len() == arg_types.len()
                    && sig
                        .params
                        .iter()
                        .zip(arg_types.iter())
                        .all(|(expected, actual)| self.is_assignable(expected, actual))
                {
                    *data_type = sig.return_type.clone();
                    return Ok(Some(sig.return_type));
                }
                if let Some(ret) = self.builtin_returns.get(name).cloned() {
                    *data_type = ret.clone();
                    return Ok(Some(ret));
                }
                {
                    let mut stripped = name.to_string();
                    while let Some(next) = Self::strip_root_namespace(&stripped) {
                        if next == stripped {
                            break;
                        }
                        if let Some(sig) = self.functions.get(&next).cloned()
                            && sig.params.len() == arg_types.len()
                            && sig
                                .params
                                .iter()
                                .zip(arg_types.iter())
                                .all(|(expected, actual)| self.is_assignable(expected, actual))
                        {
                            *data_type = sig.return_type.clone();
                            return Ok(Some(sig.return_type));
                        }
                        stripped = next;
                    }
                }
                Ok(None)
            }
            Expression::Identifier(Identifier {
                name, data_type, ..
            }) => {
                if name == "len" {
                    *data_type = DataType::Function;
                    return Ok(Some(DataType::I64));
                }
                if let Some(sig) = self.functions.get(name).cloned()
                    && sig.params.len() == 1
                    && self.is_assignable(&sig.params[0], input_type)
                {
                    *data_type = sig.return_type.clone();
                    return Ok(Some(sig.return_type));
                }
                if let Some(ret) = self.builtin_returns.get(name).cloned() {
                    *data_type = ret.clone();
                    return Ok(Some(ret));
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    pub(super) fn resolve_instance_method_call(
        &self,
        name: &str,
        arg_types: &[DataType],
    ) -> Result<Option<DataType>> {
        let Some((receiver_name, method_name)) = name.split_once('.') else {
            return Ok(None);
        };

        // Try concrete type resolution first
        if let Some(struct_name) = self.lookup_struct_name(receiver_name) {
            return self.resolve_concrete_method_call(&struct_name, method_name, arg_types);
        }

        // Try generic type with trait bounds
        if let Some((DataType::Generic(param), _)) = self.lookup_var(receiver_name) {
            return self.resolve_trait_bound_method_call(&param, method_name, arg_types);
        }

        Ok(None)
    }

    fn resolve_concrete_method_call(
        &self,
        struct_name: &str,
        method_name: &str,
        arg_types: &[DataType],
    ) -> Result<Option<DataType>> {
        // Try non-trait inherent method first: "Type.method"
        let inherent_key = format!("{}.{}", struct_name, method_name);
        if let Some(sig) = self.functions.get(&inherent_key) {
            return self.check_method_sig(struct_name, method_name, sig, &HashMap::new(), arg_types);
        }

        // Try trait methods: "Trait::Type::method"
        let (base_name, _) = Self::split_nominal_type_args(struct_name);
        let mut found: Option<(FunctionSig, HashMap<String, DataType>)> = None;

        let default_bindings = self.infer_bindings_for_struct(struct_name, base_name);

        if let Some(traits) = self.impl_traits.get(base_name) {
            for trait_name in traits.iter() {
                let trait_key = format!("{}::{}::{}", trait_name, struct_name, method_name);
                if let Some(sig) = self.functions.get(&trait_key) {
                    if found.is_some() {
                        return Err(type_error_at_span(
                            self.current_span,
                            format!(
                                "Ambiguous method '{}': multiple traits define it for type '{}'",
                                method_name, struct_name
                            ),
                        ));
                    }
                    found = Some((sig.clone(), default_bindings.clone()));
                }
            }
        }

        // Also search for methods with matching base (handles generic type args in struct name)
        if found.is_none() {
            for (candidate_name, candidate_sig) in &self.functions {
                let Some((owner, method)) = candidate_name.split_once('.') else {
                    continue;
                };
                if method != method_name {
                    continue;
                }
                let (owner_base, _) = Self::split_nominal_type_args(owner);
                if owner_base != base_name {
                    continue;
                }
                found = Some((candidate_sig.clone(), default_bindings.clone()));
                break;
            }
        }

        // Also search trait-qualified keys with matching type base
        if found.is_none() {
            for (candidate_name, candidate_sig) in &self.functions {
                let Some((rest, method)) = candidate_name.rsplit_once("::") else {
                    continue;
                };
                if method != method_name {
                    continue;
                }
                let Some((_trait, owner)) = rest.split_once("::") else {
                    continue;
                };
                let (owner_base, _) = Self::split_nominal_type_args(owner);
                if owner_base != base_name {
                    continue;
                }
                found = Some((candidate_sig.clone(), default_bindings.clone()));
                break;
            }
        }

        let Some((sig, bindings)) = found else {
            return Err(type_error_at_span(
                self.current_span,
                format!("Type '{}' has no method '{}'", struct_name, method_name),
            ));
        };

        self.check_method_sig(struct_name, method_name, &sig, &bindings, arg_types)
    }

    fn infer_bindings_for_struct(&self, struct_name: &str, base_name: &str) -> HashMap<String, DataType> {
        let (_, concrete_type_args) = Self::split_nominal_type_args(struct_name);
        if concrete_type_args.is_empty() {
            return HashMap::new();
        }
        if let Some(class_sig) = self.classes.get(base_name) {
            if !class_sig.type_params.is_empty()
                && class_sig.type_params.len() == concrete_type_args.len()
            {
                if let Ok(b) = self.bindings_for_nominal_type_args(&class_sig.type_params, &concrete_type_args) {
                    return b;
                }
            }
        }
        HashMap::new()
    }

    fn resolve_trait_bound_method_call(
        &self,
        param_name: &str,
        method_name: &str,
        arg_types: &[DataType],
    ) -> Result<Option<DataType>> {
        // Find which traits this generic param implements
        let mut found_trait: Option<String> = None;
        let mut found_method: Option<TraitMethodSig> = None;

        for (param, bounds) in &self.current_function_type_bounds {
            if param != param_name {
                continue;
            }
            for bound in bounds {
                if let Some(trait_sig) = self.traits.get(bound) {
                    if let Some(method) = trait_sig.methods.iter().find(|m| m.name == method_name) {
                        if found_trait.is_some() {
                            return Err(type_error_at_span(
                                self.current_span,
                                format!(
                                    "Ambiguous method '{}': multiple traits define it for generic parameter '{}'",
                                    method_name, param_name
                                ),
                            ));
                        }
                        found_trait = Some(bound.clone());
                        found_method = Some(method.clone());
                    }
                }
            }
        }

        let Some(method) = found_method else {
            return Err(type_error_at_span(
                self.current_span,
                format!(
                    "Generic parameter '{}' has no method '{}'",
                    param_name, method_name
                ),
            ));
        };

        // Build a signature from the trait method, substituting Self -> Generic(param)
        let generic_self = DataType::Generic(param_name.to_string());
        let params: Vec<DataType> = method
            .params
            .iter()
            .map(|(_, t)| {
                if t == &DataType::Unknown {
                    generic_self.clone()
                } else {
                    t.clone()
                }
            })
            .collect();
        let return_type = if method.return_type == DataType::Unknown {
            generic_self
        } else {
            method.return_type.clone()
        };

        let sig = FunctionSig {
            type_params: Vec::new(),
            type_param_bounds: Vec::new(),
            params,
            return_type,
        };

        // Check method signature against arguments (skip self param)
        let expected_args: Vec<DataType> = sig.params.get(1..).unwrap_or(&[]).to_vec();
        if expected_args.len() != arg_types.len() {
            return Err(type_error_at_span(
                self.current_span,
                format!(
                    "Method '{}' expects {} arguments, got {}",
                    method_name,
                    expected_args.len(),
                    arg_types.len()
                ),
            ));
        }
        for (idx, (expected, actual)) in expected_args.iter().zip(arg_types.iter()).enumerate() {
            if !self.is_assignable(expected, actual) {
                return Err(type_error_at_span(
                    self.current_span,
                    format!(
                        "Method '{}' argument {} expects {:?}, got {:?}",
                        method_name,
                        idx + 1,
                        expected,
                        actual
                    ),
                ));
            }
        }

        Ok(Some(sig.return_type))
    }

    fn check_method_sig(
        &self,
        struct_name: &str,
        method_name: &str,
        sig: &FunctionSig,
        bindings: &HashMap<String, DataType>,
        arg_types: &[DataType],
    ) -> Result<Option<DataType>> {
        if !sig.params.first().is_some_and(|t| t.is_struct_like() || t.is_enum_like()) {
            return Ok(None);
        }

        let expected_args: Vec<DataType> = sig
            .params
            .get(1..)
            .unwrap_or(&[])
            .iter()
            .map(|ty| self.substitute_generics(ty, bindings))
            .collect();

        if expected_args.len() != arg_types.len() {
            return Err(type_error_at_span(
                self.current_span,
                format!(
                    "Method '{}.{}' expects {} arguments, got {}",
                    struct_name,
                    method_name,
                    expected_args.len(),
                    arg_types.len()
                ),
            ));
        }

        for (idx, (expected, actual)) in expected_args.iter().zip(arg_types.iter()).enumerate() {
            if !self.is_assignable(expected, actual) {
                return Err(type_error_at_span(
                    self.current_span,
                    format!(
                        "Method '{}.{}' argument {} expects {:?}, got {:?}",
                        struct_name,
                        method_name,
                        idx + 1,
                        expected,
                        actual
                    ),
                ));
            }
        }

        Ok(Some(self.substitute_generics(&sig.return_type, bindings)))
    }

    pub(super) fn check_list_hof(
        &mut self,
        name: &str,
        args: &mut [Expression],
        data_type: &mut DataType,
    ) -> Result<DataType> {
        match name {
            "lists.fold" => {
                if args.len() != 3 {
                    return Err(type_error_at_span(
                        self.current_span,
                        "lists.fold expects 3 arguments".to_string(),
                    ));
                }
                // Mire currently defines the order as `(acc, closure, list)`.
                let acc_type = self.check_expression(&mut args[0])?;
                let list_type = self.check_expression(&mut args[2])?;
                let elem_type = Self::infer_list_element_type(list_type)?;
                let closure_return = self.check_closure_with_expected_params(
                    &mut args[1],
                    &[acc_type.clone(), elem_type],
                    "lists.fold",
                )?;
                if closure_return != DataType::Unknown
                    && !self.is_assignable(&acc_type, &closure_return)
                {
                    return Err(type_error_at_span(
                        self.current_span,
                        format!(
                            "lists.fold closure must return {:?}, got {:?}",
                            acc_type, closure_return
                        ),
                    ));
                }
                *data_type = acc_type.clone();
                Ok(acc_type)
            }
            "lists.map" => {
                if args.len() != 2 {
                    return Err(type_error_at_span(
                        self.current_span,
                        "lists.map expects 2 arguments".to_string(),
                    ));
                }
                let list_type = self.check_expression(&mut args[1])?;
                let elem_type = Self::infer_list_element_type(list_type)?;
                let mapped_type = self.check_closure_with_expected_params(
                    &mut args[0],
                    &[elem_type],
                    "lists.map",
                )?;
                if mapped_type == DataType::Unknown {
                    return Err(type_error_at_span(
                        self.current_span,
                        "lists.map closure must return a value".to_string(),
                    ));
                }
                let result = DataType::Vector {
                    element_type: Box::new(mapped_type),
                    dynamic: true,
                };
                *data_type = result.clone();
                Ok(result)
            }
            "lists.filter" => {
                if args.len() != 2 {
                    return Err(type_error_at_span(
                        self.current_span,
                        "lists.filter expects 2 arguments".to_string(),
                    ));
                }
                let list_type = self.check_expression(&mut args[1])?;
                let elem_type = Self::infer_list_element_type(list_type)?;
                let predicate_type = self.check_closure_with_expected_params(
                    &mut args[0],
                    std::slice::from_ref(&elem_type),
                    "lists.filter",
                )?;
                if !Self::is_bool_like(&predicate_type) {
                    return Err(type_error_at_span(
                        self.current_span,
                        format!(
                            "lists.filter closure must return bool, got {:?}",
                            predicate_type
                        ),
                    ));
                }
                let result = DataType::Vector {
                    element_type: Box::new(elem_type),
                    dynamic: true,
                };
                *data_type = result.clone();
                Ok(result)
            }
            _ => unreachable!(),
        }
    }
}
