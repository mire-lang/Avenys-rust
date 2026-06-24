use super::*;

impl TypeChecker {
    pub(super) fn check_class_constructor_call_with_bindings(
        &self,
        class_name: &str,
        class_sig: &ClassSig,
        bindings: &HashMap<String, DataType>,
        args: &[Expression],
        arg_types: &[DataType],
    ) -> Result<()> {
        let has_named = args
            .iter()
            .any(|arg| matches!(arg, Expression::NamedArg { .. }));
        let has_positional = args
            .iter()
            .any(|arg| !matches!(arg, Expression::NamedArg { .. }));

        if has_named && has_positional {
            return Err(type_error(format!(
                "Constructor '{}' cannot mix named and positional arguments",
                class_name
            )));
        }

        if has_named {
            let mut seen = HashSet::new();
            for (index, arg) in args.iter().enumerate() {
                let Expression::NamedArg { name, .. } = arg else {
                    continue;
                };

                if !seen.insert(name.clone()) {
                    return Err(type_error(format!(
                        "Constructor '{}' received duplicate field '{}'",
                        class_name, name
                    )));
                }

                let field = class_sig
                    .fields
                    .iter()
                    .find(|field| field.name == *name)
                    .ok_or_else(|| {
                        type_error(format!(
                            "Constructor '{}' has no field '{}'",
                            class_name, name
                        ))
                    })?;

                let actual = arg_types.get(index).cloned().unwrap_or(DataType::Unknown);
                let expected = self.substitute_generics(&field.data_type, bindings);
                if !self.is_assignable(&expected, &actual) {
                    return Err(type_error(format!(
                        "Constructor '{}.{}' expects {:?}, got {:?}",
                        class_name, name, expected, actual
                    )));
                }
            }

            for field in &class_sig.fields {
                if !field.has_default && !seen.contains(&field.name) {
                    return Err(type_error(format!(
                        "Constructor '{}' is missing required field '{}'",
                        class_name, field.name
                    )));
                }
            }
        } else {
            if arg_types.len() > class_sig.fields.len() {
                return Err(type_error(format!(
                    "Constructor '{}' expects at most {} values, got {}",
                    class_name,
                    class_sig.fields.len(),
                    arg_types.len()
                )));
            }

            for (index, actual) in arg_types.iter().enumerate() {
                let Some(field) = class_sig.fields.get(index) else {
                    break;
                };
                let expected = self.substitute_generics(&field.data_type, bindings);
                if !self.is_assignable(&expected, actual) {
                    return Err(type_error(format!(
                        "Constructor '{}.{}' expects {:?}, got {:?}",
                        class_name, field.name, expected, actual
                    )));
                }
            }

            for field in class_sig.fields.iter().skip(arg_types.len()) {
                if !field.has_default {
                    return Err(type_error(format!(
                        "Constructor '{}' is missing required field '{}'",
                        class_name, field.name
                    )));
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
        let Some(struct_name) = self.lookup_struct_name(receiver_name) else {
            return Ok(None);
        };
        let full_name = format!("{}.{}", struct_name, method_name);
        let (sig, bindings) = if let Some(sig) = self.functions.get(&full_name) {
            (sig.clone(), HashMap::new())
        } else {
            let (receiver_base, receiver_type_args) = Self::split_nominal_type_args(&struct_name);
            let mut found: Option<(FunctionSig, HashMap<String, DataType>)> = None;
            for (candidate_name, candidate_sig) in &self.functions {
                let Some((owner, method)) = candidate_name.split_once('.') else {
                    continue;
                };
                if method != method_name {
                    continue;
                }
                let (owner_base, owner_type_args) = Self::split_nominal_type_args(owner);
                if owner_base != receiver_base {
                    continue;
                }
                if owner_type_args.len() != receiver_type_args.len() {
                    continue;
                }
                let mut bindings = HashMap::new();
                let mut compatible = true;
                for (ot, rt) in owner_type_args.iter().zip(receiver_type_args.iter()) {
                    if let DataType::Generic(param) = ot {
                        bindings.insert(param.clone(), rt.clone());
                    } else if !self.is_assignable(ot, rt) || !self.is_assignable(rt, ot) {
                        compatible = false;
                        break;
                    }
                }
                if compatible {
                    found = Some((candidate_sig.clone(), bindings));
                    break;
                }
            }
            let Some(pair) = found else {
                return Err(type_error(format!(
                    "Struct '{}' has no method '{}'",
                    struct_name, method_name
                )));
            };
            pair
        };

        if !sig.params.first().is_some_and(DataType::is_struct_like) {
            return Ok(None);
        }

        let expected_args: Vec<DataType> = sig
            .params
            .get(1..)
            .unwrap_or(&[])
            .iter()
            .map(|ty| self.substitute_generics(ty, &bindings))
            .collect();

        if expected_args.len() != arg_types.len() {
            return Err(type_error(format!(
                "Method '{}.{}' expects {} arguments, got {}",
                struct_name,
                method_name,
                expected_args.len(),
                arg_types.len()
            )));
        }

        for (idx, (expected, actual)) in expected_args.iter().zip(arg_types.iter()).enumerate() {
            if !self.is_assignable(expected, actual) {
                return Err(type_error(format!(
                    "Method '{}.{}' argument {} expects {:?}, got {:?}",
                    struct_name,
                    method_name,
                    idx + 1,
                    expected,
                    actual
                )));
            }
        }
        Ok(Some(self.substitute_generics(&sig.return_type, &bindings)))
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
                    return Err(type_error("lists.fold expects 3 arguments".to_string()));
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
                    return Err(type_error(format!(
                        "lists.fold closure must return {:?}, got {:?}",
                        acc_type, closure_return
                    )));
                }
                *data_type = acc_type.clone();
                Ok(acc_type)
            }
            "lists.map" => {
                if args.len() != 2 {
                    return Err(type_error("lists.map expects 2 arguments".to_string()));
                }
                let list_type = self.check_expression(&mut args[1])?;
                let elem_type = Self::infer_list_element_type(list_type)?;
                let mapped_type = self.check_closure_with_expected_params(
                    &mut args[0],
                    &[elem_type],
                    "lists.map",
                )?;
                if mapped_type == DataType::Unknown {
                    return Err(type_error(
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
                    return Err(type_error("lists.filter expects 2 arguments".to_string()));
                }
                let list_type = self.check_expression(&mut args[1])?;
                let elem_type = Self::infer_list_element_type(list_type)?;
                let predicate_type = self.check_closure_with_expected_params(
                    &mut args[0],
                    std::slice::from_ref(&elem_type),
                    "lists.filter",
                )?;
                if !Self::is_bool_like(&predicate_type) {
                    return Err(type_error(format!(
                        "lists.filter closure must return bool, got {:?}",
                        predicate_type
                    )));
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
