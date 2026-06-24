use crate::error::Result;
use crate::parser::ast::{DataType, Expression, Literal};

use crate::compiler::typeck::typeck_returns::{
    implicit_return_expression_mut, statements_contain_explicit_return,
};
use crate::compiler::typeck::typeck_type_parsing::data_type_name_for_diag;
use crate::compiler::typeck::{TypeChecker, type_error, type_error_at};
impl TypeChecker {
    pub(super) fn check_expression(&mut self, expression: &mut Expression) -> Result<DataType> {
        let (line, column) = super::location::expression_location(expression);
        self.current_line = line;
        self.current_column = column;
        match expression {
            Expression::Literal(lit) => Ok(Self::literal_type(lit)),
            Expression::Identifier(ident) => {
                if let Some((resolved, _)) = self.lookup_var(&ident.name) {
                    if ident.data_type == DataType::Unknown {
                        ident.data_type = resolved.clone();
                        return Ok(resolved);
                    }
                    return Ok(ident.data_type.clone());
                }
                if self.functions.contains_key(&ident.name) || {
                    let mut stripped = ident.name.clone();
                    let mut found = false;
                while let Some(next) = Self::strip_root_namespace(&stripped) {
                    if next == stripped {
                        break;
                    }
                    if self.functions.contains_key(&next) {
                        found = true;
                        break;
                    }
                    stripped = next;
                }
                    found
                } {
                    ident.data_type = DataType::Function;
                    return Ok(DataType::Function);
                }
                Err(type_error_at(
                    ident.line,
                    ident.column,
                    format!("Unknown identifier '{}'", ident.name),
                ))
            }
            Expression::BinaryOp {
                operator,
                left,
                right,
                data_type,
            } => {
                let left_type = if Self::is_logical_operator(operator) {
                    self.check_expression_allow_unknown_identifier(left)?
                } else {
                    self.check_expression(left)?
                };
                let right_type = if Self::is_logical_operator(operator) {
                    self.check_expression_allow_unknown_identifier(right)?
                } else {
                    self.check_expression(right)?
                };
                let resolved = self.resolve_binary_type(operator, &left_type, &right_type)?;
                *data_type = resolved.clone();
                Ok(resolved)
            }
            Expression::UnaryOp {
                operator,
                operand,
                data_type,
            } => {
                let operand_type = self.check_expression(operand)?;
                let resolved = match operator.as_str() {
                    "-" if Self::is_numeric(&operand_type) => operand_type,
                    "!" if Self::is_bool_like(&operand_type) => DataType::Bool,
                    "-" => {
                        return Err(type_error(format!(
                            "Unary '-' requires numeric operand, got {:?}",
                            operand_type
                        )));
                    }
                    _ => DataType::Unknown,
                };
                *data_type = resolved.clone();
                Ok(resolved)
            }
            Expression::NamedArg {
                value, data_type, ..
            } => {
                let resolved = self.check_expression(value)?;
                *data_type = resolved.clone();
                Ok(resolved)
            }
            Expression::Call {
                name,
                args,
                type_args,
                data_type,
            } => {
                if name == "ireru" && *data_type != DataType::Unknown {
                    *data_type = data_type.clone();
                    return Ok(data_type.clone());
                }

                if name == "lists.fold" || name == "lists.map" || name == "lists.filter" {
                    return self.check_list_hof(name, args, data_type);
                }
                if name == "call" {
                    if args.is_empty() {
                        return Err(type_error(
                            "call expects at least a callback argument".to_string(),
                        ));
                    }
                    let (callback_expr, callback_args) = args
                        .split_first_mut()
                        .expect("call arguments were checked as non-empty");
                    match callback_expr {
                        Expression::Identifier(ident) => {
                            let callback_name = self
                                .lookup_function_alias(&ident.name)
                                .unwrap_or_else(|| ident.name.clone());
                            let callback_sig = self
                                .functions
                                .get(&callback_name)
                                .cloned()
                                .or_else(|| {
                                    let mut stripped = callback_name.clone();
                                    loop {
                                        if let Some(next) = Self::strip_root_namespace(&stripped) {
                                            if next == stripped {
                                                break None;
                                            }
                                            if let Some(sig) = self.functions.get(&next).cloned() {
                                                break Some(sig);
                                            }
                                            stripped = next;
                                        } else {
                                            break None;
                                        }
                                    }
                                })
                                .or_else(|| self.lookup_function_value_signature(&ident.name));
                            if callback_sig.is_none() {
                                if self
                                    .lookup_var(&ident.name)
                                    .is_some_and(|(ty, _)| ty == DataType::Function)
                                {
                                    return Err(type_error(format!(
                                        "call callback '{}' is typed as :function but its signature cannot be inferred",
                                        ident.name
                                    )));
                                }
                                return Err(type_error(format!(
                                    "call callback '{}' must be a known function, extern fn, or function value",
                                    callback_name
                                )));
                            }
                            let callback_sig = callback_sig.expect("checked is_some");
                            if callback_sig.params.len() != callback_args.len() {
                                return Err(type_error(format!(
                                    "call callback '{}' expects {} argument(s), got {}",
                                    callback_name,
                                    callback_sig.params.len(),
                                    callback_args.len()
                                )));
                            }
                            for (actual_expr, expected_ty) in
                                callback_args.iter_mut().zip(callback_sig.params.iter())
                            {
                                let actual_ty = self.check_expression(actual_expr)?;
                                if !self.is_assignable(expected_ty, &actual_ty) {
                                    return Err(type_error(format!(
                                        "call callback '{}' expects {:?}, got {:?}",
                                        callback_name, expected_ty, actual_ty
                                    )));
                                }
                            }
                            ident.data_type = DataType::Function;
                            *data_type = callback_sig.return_type.clone();
                            return Ok(callback_sig.return_type);
                        }
                        Expression::Closure {
                            params,
                            body,
                            return_type,
                            capture,
                        } => {
                            if params.len() != callback_args.len() {
                                return Err(type_error(format!(
                                    "call closure expects {} argument(s), got {}",
                                    params.len(),
                                    callback_args.len()
                                )));
                            }
                            for ((_, param_ty), actual_expr) in
                                params.iter_mut().zip(callback_args.iter_mut())
                            {
                                let actual_ty = self.check_expression(actual_expr)?;
                                let resolved_param = Self::unify_types(param_ty, &actual_ty)?;
                                *param_ty = resolved_param.clone();
                                if !self.is_assignable(&resolved_param, &actual_ty) {
                                    return Err(type_error(format!(
                                        "call closure expects {:?}, got {:?}",
                                        resolved_param, actual_ty
                                    )));
                                }
                            }
                            self.push_scope();
                            let captures = self.collect_captures(body, params, capture);
                            *capture = captures;
                            for (name, data_type) in capture.iter() {
                                self.insert_var(name.clone(), data_type.clone(), true);
                            }
                            for (name, ptype) in params.iter() {
                                self.insert_var(name.clone(), ptype.clone(), true);
                            }
                            self.return_type_stack.push(return_type.clone());
                            self.check_statements(body)?;
                            let inferred_return =
                                self.return_type_stack.pop().unwrap_or(DataType::Unknown);
                            if *return_type == DataType::Unknown {
                                *return_type = inferred_return;
                            } else if inferred_return != DataType::Unknown
                                && !self.is_assignable(return_type, &inferred_return)
                            {
                                self.pop_scope();
                                return Err(type_error(format!(
                                    "call closure must return {:?}, got {:?}",
                                    return_type, inferred_return
                                )));
                            }
                            self.pop_scope();
                            *data_type = return_type.clone();
                            return Ok(return_type.clone());
                        }
                        callback_expr => {
                            let callback_ty = self.check_expression(callback_expr)?;
                            if callback_ty != DataType::Function && callback_ty != DataType::Unknown
                            {
                                return Err(type_error(format!(
                                    "call expects function callback, got {:?}",
                                    callback_ty
                                )));
                            }
                            if let Some(callback_sig) =
                                self.function_signature_for_expr(callback_expr)
                            {
                                if callback_sig.params.len() != callback_args.len() {
                                    return Err(type_error(format!(
                                        "call callback expression expects {} argument(s), got {}",
                                        callback_sig.params.len(),
                                        callback_args.len()
                                    )));
                                }
                                for (actual_expr, expected_ty) in
                                    callback_args.iter_mut().zip(callback_sig.params.iter())
                                {
                                    let actual_ty = self.check_expression(actual_expr)?;
                                    if !self.is_assignable(expected_ty, &actual_ty) {
                                        return Err(type_error(format!(
                                            "call callback expression expects {:?}, got {:?}",
                                            expected_ty, actual_ty
                                        )));
                                    }
                                }
                                *data_type = callback_sig.return_type.clone();
                                return Ok(callback_sig.return_type);
                            }
                            if callback_ty == DataType::Function {
                                return Err(type_error(
                                    "call callback expression is :function but its signature cannot be inferred"
                                        .to_string(),
                                ));
                            }
                            for arg in callback_args.iter_mut() {
                                let _ = self.check_expression(arg)?;
                            }
                            *data_type = DataType::Unknown;
                            return Ok(DataType::Unknown);
                        }
                    }
                }

                let arg_types: Vec<DataType> = args
                    .iter_mut()
                    .map(|arg| self.check_expression(arg))
                    .collect::<Result<_>>()?;

                if name == "__if_expr" {
                    if args.len() != 3 {
                        return Err(type_error(
                            "__if_expr expects condition, then branch, and else branch".to_string(),
                        ));
                    }

                    let cond_type = arg_types.first().cloned().unwrap_or(DataType::Unknown);
                    if !Self::is_bool_like(&cond_type) {
                        return Err(type_error(format!(
                            "If expression condition must be bool, got {:?}",
                            cond_type
                        )));
                    }

                    let then_type = Self::closure_return_type(&args[1], "__if_expr then")?;
                    let else_type = Self::closure_return_type(&args[2], "__if_expr else")?;
                    let resolved = Self::unify_types(&then_type, &else_type)?;
                    *data_type = resolved.clone();
                    return Ok(resolved);
                }

                if let Some(resolved) =
                    self.infer_lifecycle_call(name, args, &arg_types, data_type)?
                {
                    return Ok(resolved);
                }

                if let Some(resolved) = self.resolve_instance_method_call(name, &arg_types)? {
                    *data_type = resolved.clone();
                    return Ok(resolved);
                }

                if let Some(resolved) = self.infer_collection_call(name, &arg_types, data_type)? {
                    return Ok(resolved);
                }

                if let Some(resolved) =
                    self.infer_function_or_builtin_call(name, &arg_types, type_args, data_type)?
                {
                    return Ok(resolved);
                }

                let (base_name, nominal_type_args_from_name) = Self::split_nominal_type_args(name);
                let nominal_type_args = if !type_args.is_empty() {
                    type_args.clone()
                } else {
                    nominal_type_args_from_name
                };
                if let Some(class_sig) = self.classes.get(base_name).cloned() {
                    let bindings = self.bindings_for_nominal_type_args(
                        &class_sig.type_params,
                        &nominal_type_args,
                    )?;
                    self.validate_nominal_generic_bounds(
                        base_name,
                        &class_sig.type_param_bounds,
                        &bindings,
                    )?;
                    self.check_class_constructor_call_with_bindings(
                        name, &class_sig, &bindings, args, &arg_types,
                    )?;
                    let typed_name = if nominal_type_args.is_empty() {
                        name.clone()
                    } else {
                        format!(
                            "{}[{}]",
                            base_name,
                            nominal_type_args
                                .iter()
                                .map(data_type_name_for_diag)
                                .collect::<Vec<_>>()
                                .join(" ")
                        )
                    };
                    *data_type = DataType::StructNamed(typed_name.clone());
                    return Ok(DataType::StructNamed(typed_name));
                }

                let canonical_variant = Self::canonical_enum_variant_name(name);
                if let Some(variant_sig) = self.enum_variants.get(&canonical_variant).cloned() {
                    let call_type_args = name
                        .split_once('.')
                        .map(|(n, _)| Self::split_nominal_type_args(n).1)
                        .unwrap_or_default();
                    let bindings = self.bindings_for_nominal_type_args(
                        &variant_sig.type_params,
                        &call_type_args,
                    )?;
                    let enum_base = canonical_variant
                        .split_once('.')
                        .map(|(e, _)| e)
                        .unwrap_or(canonical_variant.as_str());
                    self.validate_nominal_generic_bounds(
                        enum_base,
                        &variant_sig.type_param_bounds,
                        &bindings,
                    )?;
                    self.check_enum_variant_call_with_bindings(
                        name,
                        &variant_sig,
                        &bindings,
                        &arg_types,
                    )?;
                    let enum_name = name
                        .split_once('.')
                        .map(|(enum_name, _)| enum_name.to_string())
                        .unwrap_or_else(|| name.clone());
                    *data_type = DataType::EnumNamed(enum_name.clone());
                    return Ok(DataType::EnumNamed(enum_name));
                }

                Err(type_error(format!("Unknown function '{}'", name)))
            }
            Expression::List {
                elements,
                element_type,
                data_type,
            } => {
                if let DataType::Vector { dynamic: true, .. } = data_type.clone() {
                    return Ok(data_type.clone());
                }
                if let DataType::Array { .. } = data_type.clone() {
                    return Ok(data_type.clone());
                }
                let mut current = DataType::Unknown;
                for element in elements.iter_mut() {
                    let elem_type = self.check_expression(element)?;
                    current = Self::unify_types(&current, &elem_type)?;
                }
                *element_type = current.clone();
                *data_type = DataType::Vector {
                    element_type: Box::new(current.clone()),
                    dynamic: false,
                };
                Ok(data_type.clone())
            }
            Expression::Dict {
                entries,
                key_type,
                value_type,
                data_type,
            } => {
                if let DataType::Map { .. } = data_type.clone() {
                    return Ok(data_type.clone());
                }
                let mut kt = DataType::Unknown;
                let mut vt = DataType::Unknown;
                for (key, value) in entries.iter_mut() {
                    let next_key = self.check_expression(key)?;
                    let next_value = self.check_expression(value)?;
                    kt = Self::unify_types(&kt, &next_key)?;
                    vt = Self::unify_types(&vt, &next_value)?;
                }
                *key_type = kt.clone();
                *value_type = vt.clone();
                *data_type = DataType::Map {
                    key_type: Box::new(kt),
                    value_type: Box::new(vt),
                };
                Ok(data_type.clone())
            }
            Expression::Tuple {
                elements,
                data_type,
            } => {
                for element in elements.iter_mut() {
                    self.check_expression(element)?;
                }
                *data_type = DataType::Tuple;
                Ok(DataType::Tuple)
            }
            Expression::Index {
                target,
                index,
                data_type,
            } => {
                let target_type = self.check_expression(target)?;
                let index_type = self.check_expression(index)?;

                if !Self::is_numeric(&index_type)
                    && !matches!(target_type, DataType::Dict)
                    && index_type != DataType::Unknown
                {
                    return Err(type_error(format!(
                        "Index must be numeric for {:?}, got {:?}",
                        target_type, index_type
                    )));
                }

                let resolved = match target_type {
                    DataType::Array { element_type, .. } | DataType::Slice { element_type } => {
                        *element_type
                    }
                    DataType::Str => DataType::Str,
                    DataType::Vector { element_type, .. } => *element_type,
                    DataType::List | DataType::Tuple | DataType::Dict => DataType::Anything,
                    DataType::Map { value_type, .. } => *value_type,
                    DataType::Unknown => DataType::Unknown,
                    other => {
                        return Err(type_error(format!("Type {:?} is not indexable", other)));
                    }
                };

                *data_type = resolved.clone();
                Ok(resolved)
            }
            Expression::MemberAccess {
                target,
                member,
                data_type,
            } => {
                let target_type = self.check_expression(target)?;
                if target_type.is_struct_like() {
                    if let Some(struct_name) = self
                        .struct_name_for_expr(target)
                        .or_else(|| target_type.struct_name().map(ToOwned::to_owned))
                    {
                        let (base_name, type_args) = Self::split_nominal_type_args(&struct_name);
                        if let Some(class_sig) = self.classes.get(base_name) {
                            let bindings = self.bindings_for_nominal_type_args(
                                &class_sig.type_params,
                                &type_args,
                            )?;
                            if let Some(field) = class_sig.fields.iter().find(|f| f.name == *member)
                            {
                                let resolved_field =
                                    self.substitute_generics(&field.data_type, &bindings);
                                if *data_type == DataType::Unknown {
                                    *data_type = resolved_field.clone();
                                    return Ok(resolved_field);
                                }
                                return Ok((*data_type).clone());
                            }
                        }
                        if let Some(fn_sig) =
                            self.functions.get(&format!("{}.{}", struct_name, member))
                        {
                            *data_type = fn_sig.return_type.clone();
                            return Ok(fn_sig.return_type.clone());
                        }
                        return Err(type_error(format!(
                            "Struct '{}' has no field or method '{}'",
                            struct_name, member
                        )));
                    }
                    return Err(type_error(format!(
                        "Cannot resolve concrete struct type for member access '.{}'",
                        member
                    )));
                }
                if matches!(target_type, DataType::Anything) {
                    *data_type = DataType::Anything;
                    return Ok(DataType::Anything);
                }
                if matches!(target_type, DataType::Unknown) {
                    return Err(type_error(format!(
                        "Cannot access member '{}' on unknown type - type not determined",
                        member
                    )));
                }
                Err(type_error(format!(
                    "Type {:?} has no member '{}'",
                    target_type, member
                )))
            }
            Expression::EnumVariantPath {
                enum_name,
                variant_name,
                data_type,
            } => {
                let full_name =
                    Self::canonical_enum_variant_name(&format!("{}.{}", enum_name, variant_name));
                if !self.enum_variants.contains_key(&full_name) {
                    return Err(type_error(format!("Unknown enum variant '{}'", full_name)));
                }
                *data_type = DataType::EnumNamed(enum_name.clone());
                Ok(DataType::EnumNamed(enum_name.clone()))
            }
            Expression::EnumVariant {
                enum_name,
                variant_name,
                payloads,
                data_type,
            } => {
                let typed_name = format!("{}.{}", enum_name, variant_name);
                let full_name = Self::canonical_enum_variant_name(&typed_name);
                let variant_sig =
                    self.enum_variants.get(&full_name).cloned().ok_or_else(|| {
                        type_error(format!("Unknown enum variant '{}'", typed_name))
                    })?;
                self.normalize_enum_variant_payloads(&typed_name, &variant_sig, payloads)?;
                *data_type = DataType::EnumNamed(enum_name.clone());
                Ok(DataType::EnumNamed(enum_name.clone()))
            }
            Expression::Closure {
                params,
                body,
                return_type,
                capture,
            } => {
                self.push_scope();

                let captures = self.collect_captures(body, params, capture);
                *capture = captures;

                for (name, data_type) in capture.iter() {
                    self.insert_var(name.clone(), data_type.clone(), true);
                }

                for (name, ptype) in params.iter() {
                    self.insert_var(name.clone(), ptype.clone(), true);
                }

                self.return_type_stack.push(return_type.clone());
                self.check_statements(body)?;
                let inferred_return = self.return_type_stack.pop().unwrap_or(DataType::Unknown);

                if *return_type == DataType::Unknown {
                    *return_type = inferred_return;
                }

                self.pop_scope();
                Ok(DataType::Function)
            }
            Expression::Reference {
                expr,
                is_mutable,
                data_type,
                referenced_type,
            } => {
                let target_type = self.check_expression(expr)?;
                let target_is_mutable = self.reference_target_is_mutable(expr);
                if *is_mutable && !target_is_mutable {
                    return Err(type_error(
                        "Cannot take mutable reference from immutable target".to_string(),
                    ));
                }
                *is_mutable = target_is_mutable;
                *referenced_type = target_type.clone();
                *data_type = if target_is_mutable {
                    DataType::RefMut {
                        inner: Box::new(target_type.clone()),
                    }
                } else {
                    DataType::Ref {
                        inner: Box::new(target_type.clone()),
                    }
                };
                Ok(data_type.clone())
            }
            Expression::Dereference { expr, data_type } => {
                let inner = self.check_expression(expr)?;
                let resolved = match inner {
                    DataType::Ref { .. } | DataType::RefMut { .. } => self
                        .referenced_type_for_expr(expr)
                        .unwrap_or(DataType::Unknown),
                    DataType::Unknown => DataType::Unknown,
                    other => {
                        return Err(type_error(format!(
                            "Cannot dereference non-reference type {:?}",
                            other
                        )));
                    }
                };
                *data_type = resolved.clone();
                Ok(resolved)
            }
            Expression::Box { value, data_type } => {
                self.check_expression(value)?;
                *data_type = DataType::Box;
                Ok(DataType::Box)
            }
            Expression::Pipeline {
                input,
                stage,
                safe,
                data_type,
            } => {
                let input_type = self.check_expression(input)?;
                let resolved = if let Expression::Closure {
                    params,
                    body,
                    return_type,
                    capture,
                } = stage.as_mut()
                {
                    let elem_type = self.pipeline_input_element_type(&input_type);
                    self.push_scope();
                    let captures = self.collect_captures(body, params, capture);
                    *capture = captures;
                    for (name, data_type) in capture.iter() {
                        self.insert_var(name.clone(), data_type.clone(), true);
                    }
                    if let Some((_, ptype)) = params.first_mut()
                        && *ptype == DataType::Unknown
                    {
                        *ptype = elem_type.clone();
                    }
                    for (name, ptype) in params.iter() {
                        self.insert_var(name.clone(), ptype.clone(), true);
                    }
                    self.return_type_stack.push(return_type.clone());
                    self.check_statements(body)?;
                    if !statements_contain_explicit_return(body)
                        && let Some(expr) = implicit_return_expression_mut(body)
                    {
                        let tail_type = self.check_expression(expr)?;
                        if let Some(current) = self.return_type_stack.last_mut() {
                            *current = Self::unify_types(current, &tail_type)?;
                        }
                    }
                    let inferred_return = self.return_type_stack.pop().unwrap_or(DataType::Unknown);
                    if *return_type == DataType::Unknown {
                        if inferred_return == DataType::Unknown {
                            return Err(type_error(
                                "Pipeline stage return type cannot be inferred - closure must return a value".to_string(),
                            ));
                        }
                        *return_type = inferred_return.clone();
                    }
                    self.pop_scope();
                    DataType::Vector {
                        element_type: Box::new(if *return_type == DataType::Unknown {
                            return Err(type_error(
                                    "Cannot determine pipeline output element type - specify return type in closure".to_string(),
                                ));
                        } else {
                            return_type.clone()
                        }),
                        dynamic: true,
                    }
                } else if let Some(stage_type) =
                    self.resolve_pipeline_stage_type(stage.as_mut(), &input_type)?
                {
                    stage_type
                } else {
                    let stage_check = self.check_expression(stage)?;
                    if stage_check == DataType::Unknown {
                        return Err(type_error(
                            "Pipeline stage has unknown type - cannot infer output type"
                                .to_string(),
                        ));
                    }
                    stage_check
                };
                let _ = safe;
                if *data_type == DataType::Unknown {
                    *data_type = resolved.clone();
                } else if resolved != DataType::Unknown && !self.is_assignable(data_type, &resolved)
                {
                    return Err(type_error(format!(
                        "Pipeline type mismatch: expected {:?}, got {:?}",
                        data_type, resolved
                    )));
                }
                Ok(data_type.clone())
            }
            Expression::Try { expr, data_type } => {
                let inner_type = self.check_expression(expr)?;
                let resolved = match inner_type {
                    DataType::Result { ok, .. } => *ok,
                    _ => {
                        return Err(type_error(
                            "'?' operator requires a result[T, E] type".to_string(),
                        ));
                    }
                };
                if let Some(current_return) = self.return_type_stack.last()
                    && !matches!(current_return, DataType::Result { .. })
                {
                    return Err(type_error(
                        "'?' operator can only be used in a function that returns result[T, E]"
                            .to_string(),
                    ));
                }
                if *data_type == DataType::Unknown {
                    *data_type = resolved.clone();
                }
                Ok(data_type.clone())
            }
            Expression::Match {
                value,
                cases,
                default,
                data_type,
            } => {
                let value_type = self.check_expression(value)?;
                self.validate_match_expr_coverage(&value_type, cases, default)?;
                let mut resolved_type = DataType::Unknown;
                for (case_expr, case_body) in cases.iter_mut() {
                    if !Self::is_match_identifier_pattern(case_expr) {
                        let _ = self.check_match_pattern(case_expr)?;
                    }

                    self.push_scope();

                    self.insert_match_pattern_bindings(case_expr);

                    let case_type = self.check_expression(case_body)?;
                    self.pop_scope();
                    resolved_type = Self::unify_types(&resolved_type, &case_type)?;
                }

                let is_implicit_default =
                    matches!(default.as_ref(), Expression::Literal(Literal::None));
                if !is_implicit_default {
                    let default_type = self.check_expression(default)?;
                    resolved_type = Self::unify_types(&resolved_type, &default_type)?;
                }

                if *data_type == DataType::Unknown {
                    *data_type = resolved_type.clone();
                } else if resolved_type != DataType::Unknown
                    && !self.is_assignable(data_type, &resolved_type)
                {
                    return Err(type_error(format!(
                        "Match expression type mismatch: expected {:?}, got {:?}",
                        data_type, resolved_type
                    )));
                }
                Ok(data_type.clone())
            }
            Expression::Ok { value, data_type } => {
                let val_type = self.check_expression(value)?;
                let resolved = DataType::Result {
                    ok: Box::new(val_type),
                    err: Box::new(DataType::Str),
                };
                if *data_type == DataType::Unknown {
                    *data_type = resolved.clone();
                }
                Ok(data_type.clone())
            }
            Expression::Err { value, data_type } => {
                let val_type = self.check_expression(value)?;
                let resolved = DataType::Result {
                    ok: Box::new(DataType::Unknown),
                    err: Box::new(val_type),
                };
                if *data_type == DataType::Unknown {
                    *data_type = resolved.clone();
                }
                Ok(data_type.clone())
            }
        }
    }
}
