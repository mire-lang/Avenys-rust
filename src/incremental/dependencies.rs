use super::*;

pub(crate) fn collect_statement_dependencies(statement: &Statement, deps: &mut Vec<String>) {
    match statement {
        Statement::Let { value, .. } => {
            if let Some(value) = value {
                collect_expression_dependencies(value, deps);
            }
        }
        Statement::Assignment { target, value, .. } => {
            if let Some(name) = target.binding_name() {
                deps.push(name.to_string());
            }
            if let crate::parser::ast::AssignmentTarget::Index { target, index } = target {
                collect_expression_dependencies(target, deps);
                collect_expression_dependencies(index, deps);
            }
            collect_expression_dependencies(value, deps);
        }
        Statement::Function { body, params, .. } => {
            for (_, data_type) in params {
                collect_type_dependencies(data_type, deps);
            }
            for statement in body {
                collect_statement_dependencies(statement, deps);
            }
        }
        Statement::Return(expr) => {
            if let Some(expr) = expr {
                collect_expression_dependencies(expr, deps);
            }
        }
        Statement::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_expression_dependencies(condition, deps);
            for statement in then_branch {
                collect_statement_dependencies(statement, deps);
            }
            if let Some(branch) = else_branch {
                for statement in branch {
                    collect_statement_dependencies(statement, deps);
                }
            }
        }
        Statement::While { condition, body } => {
            collect_expression_dependencies(condition, deps);
            for statement in body {
                collect_statement_dependencies(statement, deps);
            }
        }
        Statement::For { iterable, body, .. } | Statement::Find { iterable, body, .. } => {
            collect_expression_dependencies(iterable, deps);
            for statement in body {
                collect_statement_dependencies(statement, deps);
            }
        }
        Statement::Expression(expr) | Statement::Drop { value: expr } => {
            collect_expression_dependencies(expr, deps);
        }
        Statement::New { value, .. } | Statement::Own { value, .. } => {
            if let Some(value) = value {
                collect_expression_dependencies(value, deps);
            }
        }
        Statement::Move { target, value } => {
            deps.push(target.clone());
            collect_expression_dependencies(value, deps);
        }
        Statement::Match {
            value,
            cases,
            default,
        } => {
            collect_expression_dependencies(value, deps);
            for (case, statements) in cases {
                collect_expression_dependencies(case, deps);
                for statement in statements {
                    collect_statement_dependencies(statement, deps);
                }
            }
            for statement in default {
                collect_statement_dependencies(statement, deps);
            }
        }
        Statement::Type { fields, .. }
        | Statement::Unsafe { body: fields }
        | Statement::Impl {
            methods: fields, ..
        } => {
            for statement in fields {
                collect_statement_dependencies(statement, deps);
            }
        }
        Statement::Skill { methods, .. } => {
            for method in methods {
                deps.push(method.name.clone());
                for (_, data_type) in &method.params {
                    collect_type_dependencies(data_type, deps);
                }
                collect_type_dependencies(&method.return_type, deps);
            }
        }
        Statement::ExternFunction {
            lib_name,
            params,
            return_type,
            ..
        } => {
            deps.push(lib_name.clone());
            for (_, data_type) in params {
                collect_type_dependencies(data_type, deps);
            }
            collect_type_dependencies(return_type, deps);
        }
        Statement::Enum { variants, .. } => {
            for variant in variants {
                for data_type in &variant.data_types {
                    collect_type_dependencies(data_type, deps);
                }
            }
        }
        Statement::Query {
            bindings,
            ops,
            joins,
            table,
            ..
        } => {
            deps.push(table.clone());
            for binding in bindings {
                deps.push(binding.column.clone());
            }
            for join in joins {
                deps.push(join.right_table.clone());
                deps.push(join.left_column.clone());
                deps.push(join.right_column.clone());
            }
            for op in ops {
                match op {
                    crate::parser::ast::QueryOp::Insert { assigns }
                    | crate::parser::ast::QueryOp::Update { assigns, .. } => {
                        for (_, expr) in assigns {
                            collect_expression_dependencies(expr, deps);
                        }
                    }
                    crate::parser::ast::QueryOp::Delete { condition } => {
                        collect_expression_dependencies(condition, deps);
                    }
                    crate::parser::ast::QueryOp::Get(get) => {
                        deps.push(get.target.clone());
                        collect_expression_dependencies(&get.condition, deps);
                        for statement in &get.body {
                            collect_statement_dependencies(statement, deps);
                        }
                    }
                    crate::parser::ast::QueryOp::Export { path }
                    | crate::parser::ast::QueryOp::Import { path } => deps.push(path.clone()),
                }
            }
        }
        Statement::Asm { instructions } => {
            for (_, expr) in instructions {
                collect_expression_dependencies(expr, deps);
            }
        }
        Statement::Load { path, items, .. } => {
            deps.extend(path.iter().cloned());
            if let Some(items) = items {
                deps.extend(items.iter().cloned());
            }
        }
        Statement::ExternLib { name, path } => {
            deps.push(name.clone());
            deps.push(path.clone());
        }
        Statement::Break | Statement::Continue | Statement::Module { .. } => {}
    }
}

pub(crate) fn collect_statement_bindings(statement: &Statement, bindings: &mut Vec<String>) {
    match statement {
        Statement::Let { name, .. } => bindings.push(name.clone()),
        Statement::Move { target, .. } => bindings.push(target.clone()),
        Statement::Function { params, body, .. } => {
            for (param_name, _) in params {
                bindings.push(param_name.clone());
            }
            for stmt in body {
                collect_statement_bindings(stmt, bindings);
            }
        }
        Statement::For {
            variable,
            index,
            body,
            ..
        } => {
            bindings.push(variable.clone());
            if let Some(index) = index {
                bindings.push(index.clone());
            }
            for stmt in body {
                collect_statement_bindings(stmt, bindings);
            }
        }
        Statement::Find { variable, body, .. } => {
            bindings.push(variable.clone());
            for stmt in body {
                collect_statement_bindings(stmt, bindings);
            }
        }
        Statement::If {
            then_branch,
            else_branch,
            ..
        } => {
            for stmt in then_branch {
                collect_statement_bindings(stmt, bindings);
            }
            if let Some(else_branch) = else_branch {
                for stmt in else_branch {
                    collect_statement_bindings(stmt, bindings);
                }
            }
        }
        Statement::While { body, .. } => {
            for stmt in body {
                collect_statement_bindings(stmt, bindings);
            }
        }
        Statement::Match { cases, default, .. } => {
            for (_, body) in cases {
                for stmt in body {
                    collect_statement_bindings(stmt, bindings);
                }
            }
            for stmt in default {
                collect_statement_bindings(stmt, bindings);
            }
        }
        Statement::Unsafe { body }
        | Statement::Type { fields: body, .. }
        | Statement::Impl { methods: body, .. } => {
            for stmt in body {
                collect_statement_bindings(stmt, bindings);
            }
        }
        Statement::Return(Some(expr))
        | Statement::Expression(expr)
        | Statement::Drop { value: expr } => {
            collect_expression_bindings(expr, bindings);
        }
        _ => {}
    }
}

fn collect_expression_bindings(expression: &Expression, bindings: &mut Vec<String>) {
    match expression {
        Expression::Match { cases, default, .. } => {
            for (_, expr) in cases {
                collect_expression_bindings(expr, bindings);
            }
            collect_expression_bindings(default, bindings);
        }
        Expression::Closure { params, body, .. } => {
            for (param_name, _) in params {
                bindings.push(param_name.clone());
            }
            for stmt in body {
                collect_statement_bindings(stmt, bindings);
            }
        }
        Expression::BinaryOp { left, right, .. } => {
            collect_expression_bindings(left, bindings);
            collect_expression_bindings(right, bindings);
        }
        Expression::UnaryOp { operand, .. }
        | Expression::Reference { expr: operand, .. }
        | Expression::Dereference { expr: operand, .. }
        | Expression::Box { value: operand, .. } => collect_expression_bindings(operand, bindings),
        Expression::Call { args, .. }
        | Expression::List { elements: args, .. }
        | Expression::Tuple { elements: args, .. } => {
            for arg in args {
                collect_expression_bindings(arg, bindings);
            }
        }
        Expression::Dict { entries, .. } => {
            for (key, value) in entries {
                collect_expression_bindings(key, bindings);
                collect_expression_bindings(value, bindings);
            }
        }
        Expression::Index { target, index, .. } => {
            collect_expression_bindings(target, bindings);
            collect_expression_bindings(index, bindings);
        }
        Expression::MemberAccess { target, .. } | Expression::NamedArg { value: target, .. } => {
            collect_expression_bindings(target, bindings);
        }
        Expression::Pipeline { input, stage, .. } => {
            collect_expression_bindings(input, bindings);
            collect_expression_bindings(stage, bindings);
        }
        Expression::Try { expr, .. }
        | Expression::Ok { value: expr, .. }
        | Expression::Err { value: expr, .. } => {
            collect_expression_bindings(expr, bindings);
        }
        Expression::EnumVariant { payloads, .. } => {
            for payload in payloads {
                collect_expression_bindings(payload, bindings);
            }
        }
        Expression::EnumVariantPath { .. }
        | Expression::Identifier { .. }
        | Expression::Literal { .. } => {}
    }
}

fn collect_expression_dependencies(expression: &Expression, deps: &mut Vec<String>) {
    match expression {
        Expression::Identifier(ident) => deps.push(ident.name.clone()),
        Expression::Call { name, args, .. } => {
            deps.push(name.clone());
            if let Some((_, member)) = name.rsplit_once('.') {
                deps.push(member.to_string());
            }
            for arg in args {
                collect_expression_dependencies(arg, deps);
            }
        }
        Expression::MemberAccess { target, member, .. } => {
            deps.push(member.clone());
            collect_expression_dependencies(target, deps);
        }
        Expression::BinaryOp { left, right, .. } => {
            collect_expression_dependencies(left, deps);
            collect_expression_dependencies(right, deps);
        }
        Expression::UnaryOp { operand, .. }
        | Expression::Reference { expr: operand, .. }
        | Expression::Dereference { expr: operand, .. }
        | Expression::Box { value: operand, .. } => collect_expression_dependencies(operand, deps),
        Expression::NamedArg { name, value, .. } => {
            deps.push(name.clone());
            collect_expression_dependencies(value, deps);
        }
        Expression::List { elements, .. } | Expression::Tuple { elements, .. } => {
            for element in elements {
                collect_expression_dependencies(element, deps);
            }
        }
        Expression::Dict { entries, .. } => {
            for (key, value) in entries {
                collect_expression_dependencies(key, deps);
                collect_expression_dependencies(value, deps);
            }
        }
        Expression::Index { target, index, .. } => {
            collect_expression_dependencies(target, deps);
            collect_expression_dependencies(index, deps);
        }
        Expression::Closure {
            params,
            body,
            return_type,
            ..
        } => {
            for (_, data_type) in params {
                collect_type_dependencies(data_type, deps);
            }
            collect_type_dependencies(return_type, deps);
            for statement in body {
                collect_statement_dependencies(statement, deps);
            }
        }
        Expression::Pipeline { input, stage, .. } => {
            collect_expression_dependencies(input, deps);
            collect_expression_dependencies(stage, deps);
        }
        Expression::Match {
            value,
            cases,
            default,
            ..
        } => {
            collect_expression_dependencies(value, deps);
            for (case, expr) in cases {
                collect_expression_dependencies(case, deps);
                collect_expression_dependencies(expr, deps);
            }
            collect_expression_dependencies(default, deps);
        }
        Expression::EnumVariantPath {
            enum_name,
            variant_name,
            ..
        } => {
            deps.push(enum_name.clone());
            deps.push(variant_name.clone());
        }
        Expression::EnumVariant {
            enum_name,
            variant_name,
            payloads,
            ..
        } => {
            deps.push(enum_name.clone());
            deps.push(variant_name.clone());
            for payload in payloads {
                collect_expression_dependencies(payload, deps);
            }
        }
        Expression::Try { expr, .. } => {
            collect_expression_dependencies(expr, deps);
        }
        Expression::Ok { value, .. } | Expression::Err { value, .. } => {
            collect_expression_dependencies(value, deps);
        }
        Expression::Literal(_) => {}
    }
}

fn collect_type_dependencies(data_type: &crate::parser::ast::DataType, deps: &mut Vec<String>) {
    match data_type {
        crate::parser::ast::DataType::StructNamed(name)
        | crate::parser::ast::DataType::EnumNamed(name) => deps.push(name.clone()),
        crate::parser::ast::DataType::DynTrait { trait_name } => deps.push(trait_name.clone()),
        crate::parser::ast::DataType::Vector { element_type, .. }
        | crate::parser::ast::DataType::Slice { element_type }
        | crate::parser::ast::DataType::Result {
            ok: element_type, ..
        } => {
            collect_type_dependencies(element_type, deps);
        }
        crate::parser::ast::DataType::Map {
            key_type,
            value_type,
        } => {
            collect_type_dependencies(key_type, deps);
            collect_type_dependencies(value_type, deps);
        }
        crate::parser::ast::DataType::Array { element_type, .. } => {
            collect_type_dependencies(element_type, deps);
        }
        _ => {}
    }
}
