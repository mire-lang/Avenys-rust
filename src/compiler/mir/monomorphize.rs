use crate::parser::ast::{DataType, Expression, Statement};
use std::collections::HashMap;

pub fn monomorphize_program(program: &mut Vec<Statement>) {
    let mut generic_funcs: HashMap<String, (Vec<String>, Statement)> = HashMap::new();
    let mut generic_structs: HashMap<String, Statement> = HashMap::new();
    let mut generic_impls: HashMap<String, Statement> = HashMap::new();
    let mut generic_impl_type_params: HashMap<String, Vec<String>> = HashMap::new();

    for stmt in program.iter() {
        match stmt {
            Statement::Function { name, type_params, .. } if !type_params.is_empty() => {
                generic_funcs.insert(name.clone(), (type_params.clone(), stmt.clone()));
            }
            Statement::Type { name, type_params, .. } if !type_params.is_empty() => {
                let base = name
                    .split_once('[')
                    .map(|(b, _)| b.to_string())
                    .unwrap_or_else(|| name.clone());
                generic_structs.insert(base, stmt.clone());
            }
            Statement::Impl {
                type_name,
                type_params,
                ..
            } if !type_params.is_empty() => {
                let base = type_name
                    .split_once('[')
                    .map(|(b, _)| b.to_string())
                    .unwrap_or_else(|| type_name.clone());
                generic_impls.insert(base.clone(), stmt.clone());
                generic_impl_type_params.insert(base, type_params.clone());
            }
            _ => {}
        }
    }

    // Phase 1: Monomorphize free generic functions (existing logic)
    if !generic_funcs.is_empty() {
        let mut mono_statements: Vec<Statement> = Vec::new();
        let mut mono_names: HashMap<String, String> = HashMap::new();

        for i in 0..program.len() {
            match &program[i] {
                Statement::Function { body, .. } => {
                    collect_monos(body, &generic_funcs, &mut mono_names, &mut mono_statements);
                }
                Statement::Impl { methods, .. } => {
                    for m in methods {
                        if let Statement::Function { body, .. } = m {
                            collect_monos(body, &generic_funcs, &mut mono_names, &mut mono_statements);
                        }
                    }
                }
                _ => {}
            }
        }

        rewrite_call_sites(program, &mono_names);
        program.extend(mono_statements);
    }

    // Phase 2: Monomorphize generic impl blocks for concrete struct types
    if generic_impls.is_empty() && generic_structs.is_empty() {
        return;
    }

    let concrete_types = scan_concrete_struct_types(program);
    if concrete_types.is_empty() {
        return;
    }

    let mut impl_monos: Vec<Statement> = Vec::new();
    let mut typ_monos: Vec<Statement> = Vec::new();

    for (base_name, type_arg_sets) in &concrete_types {
        for type_args in type_arg_sets {
            if type_args.is_empty() {
                continue;
            }

            let bindings = make_type_bindings(
                generic_impl_type_params.get(base_name)
                    .or_else(|| generic_structs.get(base_name).and_then(|s| {
                        if let Statement::Type { type_params, .. } = s {
                            Some(type_params)
                        } else {
                            None
                        }
                    }))
                    .map(|v| v.as_slice())
                    .unwrap_or(&[]),
                type_args,
            );

            // Monomorphize struct definition
            if let Some(struct_stmt) = generic_structs.get(base_name) {
                let mut mono = struct_stmt.clone();
                substitute_generics_in_statement(&mut mono, &bindings);
                if let Statement::Type { name, type_params, .. } = &mut mono {
                    *name = make_concrete_name(base_name, type_args);
                    type_params.clear();
                }
                typ_monos.push(mono);
            }

            // Monomorphize impl block
            if let Some(impl_stmt) = generic_impls.get(base_name) {
                let mut mono = impl_stmt.clone();
                substitute_generics_in_statement(&mut mono, &bindings);
                let concrete_type_name = make_concrete_name(base_name, type_args);
                if let Statement::Impl { type_name, type_params, .. } = &mut mono {
                    *type_name = concrete_type_name.clone();
                    type_params.clear();
                }
                // Rename methods: Box[T].get -> Box[i64].get
                rename_impl_methods(&mut mono, &concrete_type_name);
                impl_monos.push(mono);
            }
        }
    }

    program.extend(typ_monos);
    program.extend(impl_monos);
}

fn make_concrete_name(base: &str, type_args: &[DataType]) -> String {
    let args_str = type_args
        .iter()
        .map(|t| match t {
            DataType::StructNamed(s) => s.clone(),
            DataType::EnumNamed(s) => s.clone(),
            DataType::I64 => "i64".to_string(),
            DataType::I32 => "i32".to_string(),
            DataType::F64 => "f64".to_string(),
            DataType::F32 => "f32".to_string(),
            DataType::Bool => "bool".to_string(),
            DataType::Str => "str".to_string(),
            DataType::Char => "char".to_string(),
            DataType::None => "none".to_string(),
            _ => format!("{:?}", t),
        })
        .collect::<Vec<_>>()
        .join(" ");
    format!("{}[{}]", base, args_str)
}

fn make_type_bindings(type_params: &[String], type_args: &[DataType]) -> HashMap<String, DataType> {
    let mut bindings = HashMap::new();
    for (i, param) in type_params.iter().enumerate() {
        if let Some(arg) = type_args.get(i) {
            bindings.insert(param.clone(), arg.clone());
        }
    }
    bindings
}

fn is_concrete_type(dt: &DataType) -> bool {
    match dt {
        DataType::Generic(_) => false,
        DataType::StructNamed(name) => {
            // A struct name with "[" indicates nested generics (e.g. "Box[i64]").
            // A bare word like "T" is a type parameter, not a concrete struct.
            name.contains('[')
        }
        _ => true,
    }
}

fn scan_concrete_struct_types(program: &[Statement]) -> HashMap<String, Vec<Vec<DataType>>> {
    let mut result: HashMap<String, Vec<Vec<DataType>>> = HashMap::new();
    for stmt in program {
        walk_types_in_statement(stmt, &mut |dt: &DataType| {
            if let DataType::StructNamed(name) = dt {
                if let Some((base, type_args)) = parse_type_args_from_name(name) {
                    if !type_args.is_empty() && !has_generic(&type_args)
                        && type_args.iter().all(|a| is_concrete_type(a))
                    {
                        let entry = result.entry(base.to_string()).or_default();
                        if !entry.contains(&type_args) {
                            entry.push(type_args);
                        }
                    }
                }
            }
        });
    }
    result
}

fn parse_type_args_from_name(name: &str) -> Option<(&str, Vec<DataType>)> {
    let trimmed = name.trim();
    let start = trimmed.find('[')?;
    if !trimmed.ends_with(']') {
        return None;
    }
    let base = trimmed[..start].trim();
    let inner = &trimmed[start + 1..trimmed.len() - 1];
    let args: Vec<DataType> = if inner.is_empty() {
        Vec::new()
    } else {
        inner.split(' ').filter(|s| !s.is_empty()).map(|s| {
            let trimmed = s.trim();
            match trimmed.to_lowercase().as_str() {
                "i64" => DataType::I64,
                "i32" => DataType::I32,
                "i128" => DataType::I128,
                "f64" => DataType::F64,
                "f32" => DataType::F32,
                "bool" => DataType::Bool,
                "str" => DataType::Str,
                "char" => DataType::Char,
                "none" => DataType::None,
                _ if trimmed.starts_with("Generic(") || trimmed.starts_with("generic(") => {
                    let name = trimmed.trim_start_matches("Generic(").trim_end_matches(')').trim();
                    DataType::Generic(name.to_string())
                }
                _ if trimmed.contains('[') => DataType::StructNamed(trimmed.to_string()),
                _ => DataType::StructNamed(trimmed.to_string()),
            }
        }).collect()
    };
    Some((base, args))
}

fn has_generic(type_args: &[DataType]) -> bool {
    type_args.iter().any(|dt| matches!(dt, DataType::Generic(_)))
}

fn walk_types_in_statement<F: FnMut(&DataType)>(stmt: &Statement, f: &mut F) {
    match stmt {
        Statement::Function { params, return_type, body, .. } => {
            for (_, pt) in params {
                f(pt);
            }
            f(return_type);
            for s in body {
                walk_types_in_statement(s, f);
            }
        }
        Statement::Let { data_type, value, .. } => {
            if *data_type != DataType::Unknown {
                f(data_type);
            }
            if let Some(expr) = value {
                walk_types_in_expression(expr, f);
            }
        }
        Statement::Assignment { value, .. } => {
            walk_types_in_expression(value, f);
        }
        Statement::Return(Some(expr)) => {
            walk_types_in_expression(expr, f);
        }
        Statement::Expression(expr) => {
            walk_types_in_expression(expr, f);
        }
        Statement::If { condition, then_branch, else_branch } => {
            walk_types_in_expression(condition, f);
            for s in then_branch {
                walk_types_in_statement(s, f);
            }
            if let Some(else_branch) = else_branch {
                for s in else_branch {
                    walk_types_in_statement(s, f);
                }
            }
        }
        Statement::While { condition, body } => {
            walk_types_in_expression(condition, f);
            for s in body {
                walk_types_in_statement(s, f);
            }
        }
        Statement::For { iterable, body, .. } => {
            walk_types_in_expression(iterable, f);
            for s in body {
                walk_types_in_statement(s, f);
            }
        }
        Statement::Find { iterable, body, .. } => {
            walk_types_in_expression(iterable, f);
            for s in body {
                walk_types_in_statement(s, f);
            }
        }
        Statement::Match { value, cases, default } => {
            walk_types_in_expression(value, f);
            for (_, case_body) in cases {
                for s in case_body {
                    walk_types_in_statement(s, f);
                }
            }
            for s in default {
                walk_types_in_statement(s, f);
            }
        }
        Statement::Impl { methods, .. } => {
            for m in methods {
                walk_types_in_statement(m, f);
            }
        }
        Statement::Type { fields, .. } => {
            for field in fields {
                walk_types_in_statement(field, f);
            }
        }
        _ => {}
    }
}

fn walk_types_in_expression<F: FnMut(&DataType)>(expr: &Expression, f: &mut F) {
    match expr {
        Expression::Call { args, data_type, type_args, .. } => {
            f(data_type);
            for ta in type_args {
                f(ta);
            }
            for arg in args {
                walk_types_in_expression(arg, f);
            }
        }
        Expression::BinaryOp { left, right, data_type, .. } => {
            f(data_type);
            walk_types_in_expression(left, f);
            walk_types_in_expression(right, f);
        }
        Expression::UnaryOp { operand, data_type, .. } => {
            f(data_type);
            walk_types_in_expression(operand, f);
        }
        Expression::NamedArg { value, data_type, .. } => {
            f(data_type);
            walk_types_in_expression(value, f);
        }
        Expression::Identifier(ident) => {
            f(&ident.data_type);
        }
        Expression::Literal { .. } => {}
        Expression::List { elements, element_type, data_type } => {
            f(data_type);
            f(element_type);
            for el in elements {
                walk_types_in_expression(el, f);
            }
        }
        Expression::Dict { entries, key_type, value_type, data_type } => {
            f(data_type);
            f(key_type);
            f(value_type);
            for (k, v) in entries {
                walk_types_in_expression(k, f);
                walk_types_in_expression(v, f);
            }
        }
        Expression::Tuple { elements, data_type } => {
            f(data_type);
            for el in elements {
                walk_types_in_expression(el, f);
            }
        }
        Expression::Index { target, index, data_type } => {
            f(data_type);
            walk_types_in_expression(target, f);
            walk_types_in_expression(index, f);
        }
        Expression::MemberAccess { target, data_type, .. } => {
            f(data_type);
            walk_types_in_expression(target, f);
        }
        Expression::Closure { params, return_type, body, .. } => {
            f(return_type);
            for (_, pt) in params {
                f(pt);
            }
            for s in body {
                walk_types_in_statement(s, f);
            }
        }
        Expression::Reference { expr: inner, data_type, .. }
        | Expression::Dereference { expr: inner, data_type } => {
            f(data_type);
            walk_types_in_expression(inner, f);
        }
        Expression::Match { value, cases, default, data_type, .. } => {
            f(data_type);
            walk_types_in_expression(value, f);
            for (_, body) in cases {
                walk_types_in_expression(body, f);
            }
            walk_types_in_expression(default, f);
        }
        Expression::EnumVariant { data_type, payloads, .. } => {
            f(data_type);
            for p in payloads {
                walk_types_in_expression(p, f);
            }
        }
        Expression::EnumVariantPath { data_type, .. } => {
            f(data_type);
        }
        Expression::Ok { value, data_type }
        | Expression::Err { value, data_type }
        | Expression::Some { value, data_type } => {
            f(data_type);
            walk_types_in_expression(value, f);
        }
        Expression::Box { value, data_type } => {
            f(data_type);
            walk_types_in_expression(value, f);
        }
        Expression::Pipeline { input, stage, data_type, .. } => {
            f(data_type);
            walk_types_in_expression(input, f);
            walk_types_in_expression(stage, f);
        }
        Expression::Try { expr: inner, data_type } => {
            f(data_type);
            walk_types_in_expression(inner, f);
        }
        Expression::Ascription { expr, target, data_type } => {
            f(data_type);
            f(target);
            walk_types_in_expression(expr, f);
        }
        Expression::UseMacro { inner } => {
            walk_types_in_expression(inner, f);
        }
        Expression::MacroCall { inner } => {
            walk_types_in_expression(inner, f);
        }
    }
}

fn rename_impl_methods(impl_stmt: &mut Statement, concrete_type_name: &str) {
    if let Statement::Impl { methods, .. } = impl_stmt {
        for method in methods.iter_mut() {
            if let Statement::Function { name, .. } = method {
                if let Some((owner, method_name)) = name.split_once('.') {
                    if owner.contains('[') {
                        *name = format!("{}.{}", concrete_type_name, method_name);
                    }
                }
            }
        }
    }
}

// ====== Existing free-function monomorphization code (unchanged below) ======

fn collect_monos(
    body: &[Statement],
    generic_funcs: &HashMap<String, (Vec<String>, Statement)>,
    mono_names: &mut HashMap<String, String>,
    mono_statements: &mut Vec<Statement>,
) {
    for stmt in body {
        match stmt {
            Statement::Expression(expr) | Statement::Return(Some(expr)) => {
                collect_from_expr(expr, generic_funcs, mono_names, mono_statements);
            }
            Statement::If {
                then_branch,
                else_branch,
                ..
            } => {
                collect_monos(then_branch, generic_funcs, mono_names, mono_statements);
                if let Some(else_branch) = else_branch {
                    collect_monos(else_branch, generic_funcs, mono_names, mono_statements);
                }
            }
            Statement::While { body, .. }
            | Statement::For { body, .. }
            | Statement::Find { body, .. } => {
                collect_monos(body, generic_funcs, mono_names, mono_statements);
            }
            Statement::Match { cases, default, .. } => {
                for (_, case_body) in cases {
                    collect_monos(case_body, generic_funcs, mono_names, mono_statements);
                }
                collect_monos(default, generic_funcs, mono_names, mono_statements);
            }
            Statement::Function { body, .. } => {
                collect_monos(body, generic_funcs, mono_names, mono_statements);
            }
            _ => {}
        }
    }
}

fn collect_from_expr(
    expr: &Expression,
    generic_funcs: &HashMap<String, (Vec<String>, Statement)>,
    mono_names: &mut HashMap<String, String>,
    mono_statements: &mut Vec<Statement>,
) {
    match expr {
        Expression::Call {
            name,
            type_args,
            args,
            ..
        } => {
            if let Some((type_params, func_stmt)) = generic_funcs.get(name) {
                if !type_args.is_empty() && type_args.len() == type_params.len() {
                    let mono_name = make_mono_name(name, type_args);
                    if !mono_names.contains_key(&format!("{}/{:?}", name, type_args)) {
                        let key = format!("{}/{:?}", name, type_args);
                        mono_names.insert(key, mono_name.clone());

                        let mut cloned = func_stmt.clone();
                        let mut bindings = HashMap::new();
                        for (i, param) in type_params.iter().enumerate() {
                            if let Some(arg) = type_args.get(i) {
                                bindings.insert(param.clone(), arg.clone());
                            }
                        }
                        substitute_generics_in_statement(&mut cloned, &bindings);
                        if let Statement::Function { name: fn_name, type_params: tp, .. } = &mut cloned {
                            *fn_name = mono_name;
                            tp.clear();
                        }
                        mono_statements.push(cloned);
                    }
                }
            }
            for arg in args {
                collect_from_expr(arg, generic_funcs, mono_names, mono_statements);
            }
        }
        Expression::BinaryOp { left, right, .. } => {
            collect_from_expr(left, generic_funcs, mono_names, mono_statements);
            collect_from_expr(right, generic_funcs, mono_names, mono_statements);
        }
        Expression::UnaryOp { operand, .. } => {
            collect_from_expr(operand, generic_funcs, mono_names, mono_statements);
        }
        Expression::NamedArg { value, .. } => {
            collect_from_expr(value, generic_funcs, mono_names, mono_statements);
        }
        Expression::List { elements, .. } => {
            for el in elements {
                collect_from_expr(el, generic_funcs, mono_names, mono_statements);
            }
        }
        Expression::Dict { entries, .. } => {
            for (k, v) in entries {
                collect_from_expr(k, generic_funcs, mono_names, mono_statements);
                collect_from_expr(v, generic_funcs, mono_names, mono_statements);
            }
        }
        Expression::Index { target, index, .. } => {
            collect_from_expr(target, generic_funcs, mono_names, mono_statements);
            collect_from_expr(index, generic_funcs, mono_names, mono_statements);
        }
        Expression::MemberAccess { target, .. } => {
            collect_from_expr(target, generic_funcs, mono_names, mono_statements);
        }
        Expression::Closure { body, .. } => {
            collect_monos(body, generic_funcs, mono_names, mono_statements);
        }
        Expression::Reference { expr: inner, .. }
        | Expression::Dereference { expr: inner, .. } => {
            collect_from_expr(inner, generic_funcs, mono_names, mono_statements);
        }
        _ => {}
    }
}

fn make_mono_name(name: &str, type_args: &[DataType]) -> String {
    let args_str = type_args
        .iter()
        .map(|t| match t {
            DataType::StructNamed(s) => s.replace(|c: char| !c.is_alphanumeric(), "_"),
            DataType::EnumNamed(s) => s.replace(|c: char| !c.is_alphanumeric(), "_"),
            DataType::I64 => "i64".to_string(),
            DataType::I32 => "i32".to_string(),
            DataType::F64 => "f64".to_string(),
            DataType::F32 => "f32".to_string(),
            DataType::Bool => "bool".to_string(),
            DataType::Str => "str".to_string(),
            DataType::Char => "char".to_string(),
            DataType::None => "none".to_string(),
            _ => format!("{:?}", t).replace(|c: char| !c.is_alphanumeric(), "_"),
        })
        .collect::<Vec<_>>()
        .join("_");
    format!("{}__{}", name, args_str)
}

fn substitute_generics_in_statement(stmt: &mut Statement, bindings: &HashMap<String, DataType>) {
    match stmt {
        Statement::Function {
            params,
            return_type,
            body,
            ..
        } => {
                for (_, param_type) in params.iter_mut() {
                    *param_type = subst(param_type.clone(), bindings);
                }
                *return_type = subst(return_type.clone(), bindings);
                for s in body.iter_mut() {
                    substitute_generics_in_statement(s, bindings);
                }
        }
        Statement::Let { data_type, value, .. } => {
            *data_type = subst(data_type.clone(), bindings);
            if let Some(expr) = value {
                substitute_generics_in_expression(expr, bindings);
            }
        }
        Statement::Assignment { target: _, value, .. } => {
            substitute_generics_in_expression(value, bindings);
        }
        Statement::Return(expr) => {
            if let Some(expr) = expr {
                substitute_generics_in_expression(expr, bindings);
            }
        }
        Statement::Expression(expr) => {
            substitute_generics_in_expression(expr, bindings);
        }
        Statement::If {
            condition,
            then_branch,
            else_branch,
        } => {
            substitute_generics_in_expression(condition, bindings);
            for s in then_branch.iter_mut() {
                substitute_generics_in_statement(s, bindings);
            }
            if let Some(else_branch) = else_branch {
                for s in else_branch.iter_mut() {
                    substitute_generics_in_statement(s, bindings);
                }
            }
        }
        Statement::While { condition, body } => {
            substitute_generics_in_expression(condition, bindings);
            for s in body.iter_mut() {
                substitute_generics_in_statement(s, bindings);
            }
        }
        Statement::For { iterable, body, .. } => {
            substitute_generics_in_expression(iterable, bindings);
            for s in body.iter_mut() {
                substitute_generics_in_statement(s, bindings);
            }
        }
        Statement::Find { iterable, body, .. } => {
            substitute_generics_in_expression(iterable, bindings);
            for s in body.iter_mut() {
                substitute_generics_in_statement(s, bindings);
            }
        }
        Statement::Match { value, cases, default } => {
            substitute_generics_in_expression(value, bindings);
            for (_, case_body) in cases.iter_mut() {
                for s in case_body.iter_mut() {
                    substitute_generics_in_statement(s, bindings);
                }
            }
            for s in default.iter_mut() {
                substitute_generics_in_statement(s, bindings);
            }
        }
        Statement::Impl { methods, .. } => {
            for m in methods.iter_mut() {
                substitute_generics_in_statement(m, bindings);
            }
        }
        Statement::Type { fields, .. } => {
            for f in fields.iter_mut() {
                substitute_generics_in_statement(f, bindings);
            }
        }
        _ => {}
    }
}

fn substitute_generics_in_expression(expr: &mut Expression, bindings: &HashMap<String, DataType>) {
    match expr {
        Expression::Call {
            name: _,
            type_args,
            args,
            data_type,
            ..
        } => {
            *data_type = subst(data_type.clone(), bindings);
            for ta in type_args.iter_mut() {
                *ta = subst(ta.clone(), bindings);
            }
            for arg in args.iter_mut() {
                substitute_generics_in_expression(arg, bindings);
            }
        }
        Expression::BinaryOp { left, right, data_type, .. } => {
            *data_type = subst(data_type.clone(), bindings);
            substitute_generics_in_expression(left, bindings);
            substitute_generics_in_expression(right, bindings);
        }
        Expression::UnaryOp { operand, data_type, .. } => {
            *data_type = subst(data_type.clone(), bindings);
            substitute_generics_in_expression(operand, bindings);
        }
        Expression::NamedArg { value, data_type, .. } => {
            *data_type = subst(data_type.clone(), bindings);
            substitute_generics_in_expression(value, bindings);
        }
        Expression::Identifier(ident) => {
            ident.data_type = subst(ident.data_type.clone(), bindings);
        }
        Expression::Literal { .. } => {}
        Expression::List {
            elements,
            element_type,
            data_type,
        } => {
            *element_type = subst(element_type.clone(), bindings);
            *data_type = subst(data_type.clone(), bindings);
            for el in elements.iter_mut() {
                substitute_generics_in_expression(el, bindings);
            }
        }
        Expression::Dict {
            entries,
            key_type,
            value_type,
            data_type,
        } => {
            *key_type = subst(key_type.clone(), bindings);
            *value_type = subst(value_type.clone(), bindings);
            *data_type = subst(data_type.clone(), bindings);
            for (k, v) in entries.iter_mut() {
                substitute_generics_in_expression(k, bindings);
                substitute_generics_in_expression(v, bindings);
            }
        }
        Expression::Tuple { elements, data_type } => {
            *data_type = subst(data_type.clone(), bindings);
            for el in elements.iter_mut() {
                substitute_generics_in_expression(el, bindings);
            }
        }
        Expression::Index {
            target,
            index,
            data_type,
        } => {
            *data_type = subst(data_type.clone(), bindings);
            substitute_generics_in_expression(target, bindings);
            substitute_generics_in_expression(index, bindings);
        }
        Expression::MemberAccess { target, data_type, .. } => {
            *data_type = subst(data_type.clone(), bindings);
            substitute_generics_in_expression(target, bindings);
        }
        Expression::Closure {
            params,
            return_type,
            body,
            ..
        } => {
            for (_, pt) in params.iter_mut() {
                *pt = subst(pt.clone(), bindings);
            }
            *return_type = subst(return_type.clone(), bindings);
            for s in body.iter_mut() {
                substitute_generics_in_statement(s, bindings);
            }
        }
        Expression::Reference { expr: inner, data_type, .. } | Expression::Dereference { expr: inner, data_type } => {
            *data_type = subst(data_type.clone(), bindings);
            substitute_generics_in_expression(inner, bindings);
        }
        Expression::Match {
            value,
            cases,
            default,
            data_type,
            ..
        } => {
            *data_type = subst(data_type.clone(), bindings);
            substitute_generics_in_expression(value, bindings);
            for (_, body) in cases.iter_mut() {
                substitute_generics_in_expression(body, bindings);
            }
            substitute_generics_in_expression(default, bindings);
        }
        Expression::EnumVariant { data_type, payloads, .. } => {
            *data_type = subst(data_type.clone(), bindings);
            for p in payloads.iter_mut() {
                substitute_generics_in_expression(p, bindings);
            }
        }
        Expression::EnumVariantPath { data_type, .. } => {
            *data_type = subst(data_type.clone(), bindings);
        }
        Expression::Ok { value, data_type }
        | Expression::Err { value, data_type }
        | Expression::Some { value, data_type } => {
            *data_type = subst(data_type.clone(), bindings);
            substitute_generics_in_expression(value, bindings);
        }
        Expression::Box { value, data_type } => {
            *data_type = subst(data_type.clone(), bindings);
            substitute_generics_in_expression(value, bindings);
        }
        Expression::Pipeline { input, stage, data_type, .. } => {
            *data_type = subst(data_type.clone(), bindings);
            substitute_generics_in_expression(input, bindings);
            substitute_generics_in_expression(stage, bindings);
        }
        Expression::Try { expr: inner, data_type } => {
            *data_type = subst(data_type.clone(), bindings);
            substitute_generics_in_expression(inner, bindings);
        }
        Expression::Ascription { expr, target, data_type } => {
            *target = subst(target.clone(), bindings);
            *data_type = subst(data_type.clone(), bindings);
            substitute_generics_in_expression(expr, bindings);
        }
        Expression::UseMacro { inner } => {
            substitute_generics_in_expression(inner, bindings);
        }
        Expression::MacroCall { inner } => {
            substitute_generics_in_expression(inner, bindings);
        }
    }
}

fn subst(dt: DataType, bindings: &HashMap<String, DataType>) -> DataType {
    match dt {
        DataType::Generic(name) => bindings.get(&name).cloned().unwrap_or(DataType::Unknown),
        DataType::Vector {
            element_type,
            dynamic,
        } => DataType::Vector {
            element_type: Box::new(subst(*element_type, bindings)),
            dynamic,
        },
        DataType::Map {
            key_type,
            value_type,
        } => DataType::Map {
            key_type: Box::new(subst(*key_type, bindings)),
            value_type: Box::new(subst(*value_type, bindings)),
        },
        DataType::Array { element_type, size } => DataType::Array {
            element_type: Box::new(subst(*element_type, bindings)),
            size,
        },
        DataType::Slice { element_type } => DataType::Slice {
            element_type: Box::new(subst(*element_type, bindings)),
        },
        DataType::Ref { inner } => DataType::Ref {
            inner: Box::new(subst(*inner, bindings)),
        },
        DataType::RefMut { inner } => DataType::RefMut {
            inner: Box::new(subst(*inner, bindings)),
        },
        DataType::Result { ok, err } => DataType::Result {
            ok: Box::new(subst(*ok, bindings)),
            err: Box::new(subst(*err, bindings)),
        },
        DataType::Maybe { inner } => DataType::Maybe {
            inner: Box::new(subst(*inner, bindings)),
        },
        other => other,
    }
}

fn rewrite_call_sites(
    statements: &mut [Statement],
    mono_names: &HashMap<String, String>,
) {
    for stmt in statements.iter_mut() {
        match stmt {
            Statement::Function { body, .. } => {
                rewrite_in_body(body, mono_names);
            }
            Statement::Impl { methods, .. } => {
                for m in methods.iter_mut() {
                    if let Statement::Function { body, .. } = m {
                        rewrite_in_body(body, mono_names);
                    }
                }
            }
            Statement::If {
                then_branch,
                else_branch,
                ..
            } => {
                rewrite_call_sites(then_branch, mono_names);
                if let Some(else_branch) = else_branch {
                    rewrite_call_sites(else_branch, mono_names);
                }
            }
            Statement::While { body, .. }
            | Statement::For { body, .. }
            | Statement::Find { body, .. } => {
                rewrite_call_sites(body, mono_names);
            }
            Statement::Match { cases, default, .. } => {
                for (_, case_body) in cases {
                    rewrite_call_sites(case_body, mono_names);
                }
                rewrite_call_sites(default, mono_names);
            }
            _ => {}
        }
    }
}

fn rewrite_in_body(body: &mut [Statement], mono_names: &HashMap<String, String>) {
    for stmt in body.iter_mut() {
        match stmt {
            Statement::Expression(expr) | Statement::Return(Some(expr)) => {
                rewrite_expr(expr, mono_names);
            }
            Statement::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                rewrite_expr(condition, mono_names);
                rewrite_call_sites(then_branch, mono_names);
                if let Some(else_branch) = else_branch {
                    rewrite_call_sites(else_branch, mono_names);
                }
            }
            Statement::While { condition, body, .. } => {
                rewrite_expr(condition, mono_names);
                rewrite_call_sites(body, mono_names);
            }
            Statement::For { iterable, body, .. } => {
                rewrite_expr(iterable, mono_names);
                rewrite_call_sites(body, mono_names);
            }
            Statement::Find { iterable, body, .. } => {
                rewrite_expr(iterable, mono_names);
                rewrite_call_sites(body, mono_names);
            }
            Statement::Match { value, cases, default, .. } => {
                rewrite_expr(value, mono_names);
                for (_, case_body) in cases.iter_mut() {
                    rewrite_call_sites(case_body, mono_names);
                }
                rewrite_call_sites(default, mono_names);
            }
            _ => {}
        }
    }
}

fn rewrite_expr(expr: &mut Expression, mono_names: &HashMap<String, String>) {
    match expr {
        Expression::Call {
            name,
            type_args,
            args,
            ..
        } => {
            let key = format!("{}/{:?}", name, type_args);
            if let Some(mono_name) = mono_names.get(&key) {
                *name = mono_name.clone();
            }
            for arg in args.iter_mut() {
                rewrite_expr(arg, mono_names);
            }
        }
        Expression::BinaryOp { left, right, .. } => {
            rewrite_expr(left, mono_names);
            rewrite_expr(right, mono_names);
        }
        Expression::UnaryOp { operand, .. } => {
            rewrite_expr(operand, mono_names);
        }
        Expression::NamedArg { value, .. } => {
            rewrite_expr(value, mono_names);
        }
        Expression::List { elements, .. } => {
            for el in elements.iter_mut() {
                rewrite_expr(el, mono_names);
            }
        }
        Expression::Dict { entries, .. } => {
            for (k, v) in entries.iter_mut() {
                rewrite_expr(k, mono_names);
                rewrite_expr(v, mono_names);
            }
        }
        Expression::Index { target, index, .. } => {
            rewrite_expr(target, mono_names);
            rewrite_expr(index, mono_names);
        }
        Expression::MemberAccess { target, .. } => {
            rewrite_expr(target, mono_names);
        }
        Expression::Closure { body, .. } => {
            rewrite_call_sites(body, mono_names);
        }
        Expression::Reference { expr: inner, .. }
        | Expression::Dereference { expr: inner, .. } => {
            rewrite_expr(inner, mono_names);
        }
        Expression::Match {
            value,
            cases,
            default,
            ..
        } => {
            rewrite_expr(value, mono_names);
            for (_, body) in cases.iter_mut() {
                rewrite_expr(body, mono_names);
            }
            rewrite_expr(default, mono_names);
        }
        Expression::Ok { value, .. }
        | Expression::Err { value, .. }
        | Expression::Some { value, .. } => {
            rewrite_expr(value, mono_names);
        }
        Expression::Box { value, .. } => {
            rewrite_expr(value, mono_names);
        }
        Expression::Pipeline { input, stage, .. } => {
            rewrite_expr(input, mono_names);
            rewrite_expr(stage, mono_names);
        }
        Expression::Try { expr: inner, .. } => {
            rewrite_expr(inner, mono_names);
        }
        Expression::Ascription { expr, .. } => {
            rewrite_expr(expr, mono_names);
        }
        Expression::UseMacro { inner } => {
            rewrite_expr(inner, mono_names);
        }
        Expression::MacroCall { inner } => {
            rewrite_expr(inner, mono_names);
        }
        Expression::Tuple { elements, .. } => {
            for el in elements.iter_mut() {
                rewrite_expr(el, mono_names);
            }
        }
        Expression::EnumVariant { payloads, .. } => {
            for p in payloads.iter_mut() {
                rewrite_expr(p, mono_names);
            }
        }
        _ => {}
    }
}
