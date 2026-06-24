use crate::error::{ErrorKind, MireError, Result};
use crate::parser::ast::{DataType, Expression, Literal, Statement};

use crate::compiler::typeck::{FunctionSig, TypeChecker, type_error};
impl TypeChecker {
    pub(super) fn infer_collection_call(
        &self,
        name: &str,
        arg_types: &[DataType],
        data_type: &mut DataType,
    ) -> Result<Option<DataType>> {
        if name == "dicts.get" {
            let resolved = match arg_types.first().cloned().unwrap_or(DataType::Unknown) {
                DataType::Map { value_type, .. } => *value_type,
                DataType::Dict => arg_types.get(2).cloned().unwrap_or(DataType::Anything),
                _ => arg_types.get(2).cloned().unwrap_or(DataType::Anything),
            };
            *data_type = resolved.clone();
            return Ok(Some(resolved));
        }

        if name == "dicts.set" {
            let key_type = arg_types.get(1).cloned().unwrap_or(DataType::Anything);
            let value_type = arg_types.get(2).cloned().unwrap_or(DataType::Anything);
            let resolved = match arg_types.first().cloned().unwrap_or(DataType::Unknown) {
                DataType::Map {
                    key_type,
                    value_type: existing_value,
                } => DataType::Map {
                    key_type,
                    value_type: Box::new(Self::unify_types(&existing_value, &value_type)?),
                },
                _ => DataType::Map {
                    key_type: Box::new(key_type),
                    value_type: Box::new(value_type),
                },
            };
            *data_type = resolved.clone();
            return Ok(Some(resolved));
        }

        if name == "lists.get" {
            let arg_type = arg_types.first().cloned().unwrap_or(DataType::Unknown);
            let resolved = match arg_type {
                DataType::Vector { element_type, .. } => *element_type,
                DataType::List | DataType::Unknown | DataType::Anything => DataType::Anything,
                other => {
                    return Err(type_error(format!(
                        "lists.get expects vec/vec! input, got {:?}",
                        other
                    )));
                }
            };
            *data_type = resolved.clone();
            return Ok(Some(resolved));
        }

        if name == "contains" || name == "strings.contains" {
            let haystack_type = arg_types.first().cloned().unwrap_or(DataType::Unknown);
            if !matches!(
                haystack_type,
                DataType::Str
                    | DataType::Vector { .. }
                    | DataType::List
                    | DataType::Dict
                    | DataType::Map { .. }
                    | DataType::Unknown
                    | DataType::Anything
            ) {
                return Err(MireError::new(ErrorKind::Backend {
                    message: format!(
                        "contains(...) is not implemented for type {:?}",
                        haystack_type
                    ),
                }));
            }
            *data_type = DataType::Bool;
            return Ok(Some(DataType::Bool));
        }

        if name == "lists.push" {
            let list_type = arg_types.first().cloned().unwrap_or(DataType::Unknown);
            let value_type = arg_types.get(1).cloned().unwrap_or(DataType::Unknown);
            let resolved = match list_type {
                DataType::Vector {
                    element_type,
                    dynamic: true,
                } => DataType::Vector {
                    element_type: Box::new(Self::unify_types(&element_type, &value_type)?),
                    dynamic: true,
                },
                DataType::Vector {
                    dynamic: false,
                    element_type,
                } => DataType::Vector {
                    element_type: Box::new(Self::unify_types(&element_type, &value_type)?),
                    dynamic: true,
                },
                DataType::List | DataType::Unknown => DataType::Vector {
                    element_type: Box::new(value_type),
                    dynamic: true,
                },
                other => {
                    return Err(type_error(format!(
                        "lists.push expects vec[T], got {:?}",
                        other
                    )));
                }
            };
            *data_type = resolved.clone();
            return Ok(Some(resolved));
        }

        if name == "lists.slice" {
            let list_type = arg_types.first().cloned().unwrap_or(DataType::Unknown);
            let resolved = match list_type {
                DataType::Vector { element_type, .. } => DataType::Vector {
                    element_type: element_type.clone(),
                    dynamic: true,
                },
                DataType::List => DataType::Vector {
                    element_type: Box::new(DataType::Unknown),
                    dynamic: true,
                },
                other => {
                    return Err(type_error(format!(
                        "lists.slice expects vector input, got {:?}",
                        other
                    )));
                }
            };
            *data_type = resolved.clone();
            return Ok(Some(resolved));
        }

        if name == "lists.pop" || name == "lists.first" || name == "lists.last" {
            let list_type = arg_types.first().cloned().unwrap_or(DataType::Unknown);
            let resolved = match list_type {
                DataType::Vector { element_type, .. } => *element_type,
                DataType::List | DataType::Unknown | DataType::Anything => DataType::Anything,
                other => {
                    return Err(type_error(format!(
                        "{} expects vec[T] input, got {:?}",
                        name, other
                    )));
                }
            };
            *data_type = resolved.clone();
            return Ok(Some(resolved));
        }

        if name == "lists.is_empty" {
            return Ok(Some(DataType::Bool));
        }

        if name == "lists.append" {
            return Ok(Some(DataType::None));
        }

        if name == "strings.join" {
            let parts_type = arg_types.first().cloned().unwrap_or(DataType::Unknown);
            let sep_type = arg_types.get(1).cloned().unwrap_or(DataType::Unknown);
            if !matches!(
                parts_type,
                DataType::Vector { .. } | DataType::List | DataType::Unknown | DataType::Anything
            ) {
                return Err(type_error(format!(
                    "strings.join expects vec input, got {:?}",
                    parts_type
                )));
            }
            if sep_type != DataType::Str && sep_type != DataType::Unknown {
                return Err(type_error(format!(
                    "strings.join separator expects Str, got {:?}",
                    sep_type
                )));
            }
            *data_type = DataType::Str;
            return Ok(Some(DataType::Str));
        }

        Ok(None)
    }
}

impl TypeChecker {
    pub(super) fn infer_function_or_builtin_call(
        &self,
        name: &str,
        arg_types: &[DataType],
        type_args: &mut Vec<DataType>,
        data_type: &mut DataType,
    ) -> Result<Option<DataType>> {
        if let Some(sig) = self.functions.get(name).cloned() {
            match self.resolve_function_call(name, &sig, arg_types, type_args) {
                Ok(resolved) => {
                    *data_type = resolved.clone();
                    return Ok(Some(resolved));
                }
                Err(err) => {
                    if self.builtin_returns.contains_key(name) {
                        // User-defined function signature didn't match but a
                        // builtin exists (e.g. lists.len). Fall through to try
                        // the builtin return type, which may be more permissive.
                    } else {
                        return Err(err);
                    }
                }
            }
        }

        {
            let mut stripped = name.to_string();
            while let Some(next) = Self::strip_root_namespace(&stripped) {
                if next == stripped {
                    break;
                }
                if let Some(sig) = self.functions.get(&next).cloned() {
                    match self.resolve_function_call(&next, &sig, arg_types, type_args) {
                        Ok(resolved) => {
                            *data_type = resolved.clone();
                            return Ok(Some(resolved));
                        }
                        Err(err) => {
                            if !self.builtin_returns.contains_key(&next) {
                                return Err(err);
                            }
                        }
                    }
                }
                stripped = next;
            }
        }

        if let Some(ret) = self.builtin_returns.get(name).cloned() {
            *data_type = ret.clone();
            return Ok(Some(ret));
        }

        if let Some(rest) = name.strip_prefix("std.")
            && let Some(ret) = self.builtin_returns.get(rest).cloned()
        {
            *data_type = ret.clone();
            return Ok(Some(ret));
        }

        Ok(None)
    }

    fn resolve_function_call(
        &self,
        display_name: &str,
        sig: &FunctionSig,
        arg_types: &[DataType],
        type_args: &mut Vec<DataType>,
    ) -> Result<DataType> {
        if sig.params.len() != arg_types.len() {
            return Err(type_error(format!(
                "Function '{}' expects {} arguments, got {}",
                display_name,
                sig.params.len(),
                arg_types.len()
            )));
        }

        let resolved_type_args = if sig.type_params.is_empty() {
            if !type_args.is_empty() {
                return Err(type_error(format!(
                    "Function '{}' is not generic; remove explicit type arguments",
                    display_name
                )));
            }
            Vec::new()
        } else {
            self.resolve_generic_type_args(sig, type_args, arg_types)?
        };
        self.validate_generic_bounds(display_name, sig, &resolved_type_args)?;
        let generic_bindings = self.generic_bindings_from_args(sig, &resolved_type_args);

        for (idx, (expected, actual)) in sig
            .params
            .iter()
            .map(|param| self.substitute_generics(param, &generic_bindings))
            .zip(arg_types.iter())
            .enumerate()
        {
            if !self.is_assignable(&expected, actual) {
                return Err(type_error(format!(
                    "Function '{}' argument {} expects {:?}, got {:?}",
                    display_name,
                    idx + 1,
                    expected,
                    actual
                )));
            }
        }

        if !resolved_type_args.is_empty() {
            *type_args = resolved_type_args;
        }
        Ok(self.substitute_generics(&sig.return_type, &generic_bindings))
    }
}

impl TypeChecker {
    pub(super) fn infer_lifecycle_call(
        &self,
        name: &str,
        args: &[Expression],
        arg_types: &[DataType],
        data_type: &mut DataType,
    ) -> Result<Option<DataType>> {
        if name == "new::" {
            if args.is_empty() {
                if *data_type == DataType::Unknown {
                    return Err(type_error(
                        "new::() requires a type annotation (:T)".to_string(),
                    ));
                }
                return Ok(Some(data_type.clone()));
            }
            if args.len() == 1 {
                *data_type = arg_types[0].clone();
                return Ok(Some(arg_types[0].clone()));
            }
            return Ok(None);
        }

        if name == "own::" {
            if args.is_empty() {
                if *data_type == DataType::Unknown {
                    return Err(type_error(
                        "own::() requires a type annotation (:T)".to_string(),
                    ));
                }
                *data_type = DataType::Box;
                return Ok(Some(DataType::Box));
            }
            if args.len() == 1 {
                *data_type = DataType::Box;
                return Ok(Some(DataType::Box));
            }
            return Ok(None);
        }

        if name == "move::"
            && let Some(first) = arg_types.first()
        {
            *data_type = first.clone();
            return Ok(Some(first.clone()));
        }

        if name == "drop::" {
            *data_type = DataType::None;
            return Ok(Some(DataType::None));
        }

        Ok(None)
    }

    pub(super) fn validate_new_target_type(&self, declared_type: &DataType) -> Result<()> {
        if matches!(
            declared_type,
            DataType::Array { .. } | DataType::Vector { .. } | DataType::Map { .. }
        ) {
            return Ok(());
        }

        Err(type_error(format!(
            "new:: only supports arr/vec/map targets, got {:?}",
            declared_type
        )))
    }

    pub(super) fn validate_own_target_type(&self, inner_type: &DataType) -> Result<()> {
        if matches!(
            inner_type,
            DataType::I8
                | DataType::I16
                | DataType::I32
                | DataType::I64
                | DataType::U8
                | DataType::U16
                | DataType::U32
                | DataType::U64
                | DataType::F32
                | DataType::F64
                | DataType::Bool
                | DataType::Char
                | DataType::Str
                | DataType::Struct
                | DataType::StructNamed(_)
                | DataType::Enum
                | DataType::EnumNamed(_)
                | DataType::Array { .. }
                | DataType::Vector { .. }
                | DataType::Map { .. }
        ) {
            return Ok(());
        }

        Err(type_error(format!(
            "own:: target type {:?} is not heap-allocatable",
            inner_type
        )))
    }
}

impl TypeChecker {
    fn variant_names_from_match_pattern(
        &self,
        pattern: &Expression,
        expected_enum: &str,
    ) -> Result<(Vec<String>, bool)> {
        match pattern {
            Expression::EnumVariantPath {
                enum_name,
                variant_name,
                ..
            }
            | Expression::EnumVariant {
                enum_name,
                variant_name,
                ..
            } => {
                if enum_name != expected_enum {
                    return Err(type_error(format!(
                        "Match pattern enum mismatch: expected '{}', got '{}'",
                        expected_enum, enum_name
                    )));
                }
                Ok((vec![variant_name.clone()], true))
            }
            Expression::Call { name, args, .. } if name == "__match_guard" => {
                if let Some(inner) = args.first() {
                    // Guarded variants do not count as exhaustive coverage.
                    let (names, _) = self.variant_names_from_match_pattern(inner, expected_enum)?;
                    Ok((names, false))
                } else {
                    Ok((Vec::new(), false))
                }
            }
            Expression::Call { name, args, .. } if name == "__match_or" => {
                if args.len() != 2 {
                    return Ok((Vec::new(), false));
                }
                let (mut left, left_exhaustive) =
                    self.variant_names_from_match_pattern(&args[0], expected_enum)?;
                let (right, right_exhaustive) =
                    self.variant_names_from_match_pattern(&args[1], expected_enum)?;
                left.extend(right);
                Ok((left, left_exhaustive && right_exhaustive))
            }
            _ => Ok((Vec::new(), false)),
        }
    }

    fn enum_variant_names_for(&self, enum_name: &str) -> Vec<String> {
        let prefix = format!("{enum_name}.");
        self.enum_variants
            .keys()
            .filter_map(|full| full.strip_prefix(&prefix).map(ToOwned::to_owned))
            .collect()
    }

    pub(super) fn validate_match_coverage(
        &self,
        value_type: &DataType,
        cases: &[(Expression, Vec<Statement>)],
        has_default: bool,
    ) -> Result<()> {
        let DataType::EnumNamed(enum_name) = value_type else {
            return Ok(());
        };

        let mut covered = std::collections::HashSet::new();
        for (pattern, _) in cases {
            let (variant_names, counts_for_exhaustiveness) =
                self.variant_names_from_match_pattern(pattern, enum_name)?;
            if !counts_for_exhaustiveness {
                continue;
            }
            for variant_name in variant_names {
                if !covered.insert(variant_name.clone()) {
                    return Err(type_error(format!(
                        "Duplicate match arm for enum variant '{}.{}'",
                        enum_name, variant_name
                    )));
                }
            }
        }

        if has_default {
            return Ok(());
        }

        let all = self.enum_variant_names_for(enum_name);
        let missing: Vec<String> = all
            .into_iter()
            .filter(|name| !covered.contains(name))
            .collect();
        if missing.is_empty() {
            return Ok(());
        }

        Err(type_error(format!(
            "Non-exhaustive match for enum '{}'; missing variants: {}",
            enum_name,
            missing.join(", ")
        )))
    }

    pub(super) fn validate_match_expr_coverage(
        &self,
        value_type: &DataType,
        cases: &[(Expression, Expression)],
        default: &Expression,
    ) -> Result<()> {
        let DataType::EnumNamed(enum_name) = value_type else {
            return Ok(());
        };

        let mut covered = std::collections::HashSet::new();
        for (pattern, _) in cases {
            let (variant_names, counts_for_exhaustiveness) =
                self.variant_names_from_match_pattern(pattern, enum_name)?;
            if !counts_for_exhaustiveness {
                continue;
            }
            for variant_name in variant_names {
                if !covered.insert(variant_name.clone()) {
                    return Err(type_error(format!(
                        "Duplicate match arm for enum variant '{}.{}'",
                        enum_name, variant_name
                    )));
                }
            }
        }

        let has_default = !matches!(default, Expression::Literal(Literal::None));
        if has_default {
            return Ok(());
        }

        let all = self.enum_variant_names_for(enum_name);
        let missing: Vec<String> = all
            .into_iter()
            .filter(|name| !covered.contains(name))
            .collect();
        if missing.is_empty() {
            return Ok(());
        }

        Err(type_error(format!(
            "Non-exhaustive match expression for enum '{}'; missing variants: {}",
            enum_name,
            missing.join(", ")
        )))
    }
}
