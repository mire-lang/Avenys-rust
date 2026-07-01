use super::*;

pub(super) fn hash_statement(statement: &Statement, hasher: &mut FxHasher) {
    match statement {
        Statement::Let {
            name,
            data_type,
            value,
            is_constant,
            is_mutable,
            is_static,
            visibility,
            name_line: _,
            name_column: _,
        } => {
            hasher.write_u8(0);
            name.hash(hasher);
            hash_data_type(data_type, hasher);
            hash_option_expr(value, hasher);
            is_constant.hash(hasher);
            is_mutable.hash(hasher);
            is_static.hash(hasher);
            hash_visibility(*visibility, hasher);
        }
        Statement::Assignment {
            target,
            value,
            is_mutable,
        } => {
            hasher.write_u8(1);
            hash_assignment_target(target, hasher);
            hash_expression(value, hasher);
            is_mutable.hash(hasher);
        }
        Statement::Function {
            name,
            type_params,
            type_param_bounds,
            params,
            body,
            return_type,
            visibility,
            is_method,
        } => {
            hasher.write_u8(2);
            name.hash(hasher);
            type_params.hash(hasher);
            type_param_bounds.hash(hasher);
            hash_params(params, hasher);
            hash_statements(body, hasher);
            hash_data_type(return_type, hasher);
            hash_visibility(*visibility, hasher);
            is_method.hash(hasher);
        }
        Statement::Return(expr) => {
            hasher.write_u8(3);
            hash_option_expr(expr, hasher);
        }
        Statement::If {
            condition,
            then_branch,
            else_branch,
        } => {
            hasher.write_u8(4);
            hash_expression(condition, hasher);
            hash_statements(then_branch, hasher);
            hash_optional_statements(else_branch, hasher);
        }
        Statement::While { condition, body } => {
            hasher.write_u8(5);
            hash_expression(condition, hasher);
            hash_statements(body, hasher);
        }
        Statement::For {
            variable,
            index,
            iterable,
            body,
        } => {
            hasher.write_u8(6);
            variable.hash(hasher);
            index.hash(hasher);
            hash_expression(iterable, hasher);
            hash_statements(body, hasher);
        }
        Statement::Expression(expr) => {
            hasher.write_u8(7);
            hash_expression(expr, hasher);
        }
        Statement::Break => hasher.write_u8(8),
        Statement::Continue => hasher.write_u8(9),
        Statement::Find {
            variable,
            iterable,
            body,
        } => {
            hasher.write_u8(10);
            variable.hash(hasher);
            hash_expression(iterable, hasher);
            hash_statements(body, hasher);
        }
        Statement::Match {
            value,
            cases,
            default,
        } => {
            hasher.write_u8(11);
            hash_expression(value, hasher);
            cases.len().hash(hasher);
            for (case, body) in cases {
                hash_expression(case, hasher);
                hash_statements(body, hasher);
            }
            hash_statements(default, hasher);
        }
        Statement::Type {
            visibility: _,
            name,
            type_params,
            type_param_bounds,
            parent,
            fields,
        } => {
            hasher.write_u8(12);
            name.hash(hasher);
            type_params.hash(hasher);
            type_param_bounds.hash(hasher);
            parent.hash(hasher);
            hash_statements(fields, hasher);
        }
        Statement::Skill { name, methods, .. } => {
            hasher.write_u8(13);
            name.hash(hasher);
            hash_trait_methods(methods, hasher);
        }
        Statement::Impl {
            trait_name,
            type_name,
            methods,
            type_params,
            type_param_bounds,
        } => {
            hasher.write_u8(14);
            trait_name.hash(hasher);
            type_name.hash(hasher);
            type_params.hash(hasher);
            type_param_bounds.hash(hasher);
            hash_statements(methods, hasher);
        }
        Statement::ExternLib { name, path } => {
            hasher.write_u8(18);
            name.hash(hasher);
            path.hash(hasher);
        }
        Statement::ExternFunction {
            name,
            lib_name,
            params,
            return_type,
            visibility: _,
        } => {
            hasher.write_u8(19);
            name.hash(hasher);
            lib_name.hash(hasher);
            hash_params(params, hasher);
            hash_data_type(return_type, hasher);
        }
        Statement::Unsafe { body } => {
            hasher.write_u8(20);
            hash_statements(body, hasher);
        }
        Statement::Asm { instructions } => {
            hasher.write_u8(21);
            instructions.len().hash(hasher);
            for (name, expr) in instructions {
                name.hash(hasher);
                hash_expression(expr, hasher);
            }
        }
        Statement::Load { path, alias, items } => {
            hasher.write_u8(23);
            path.hash(hasher);
            alias.hash(hasher);
            items.hash(hasher);
        }
        Statement::Module { name } => {
            hasher.write_u8(24);
            name.hash(hasher);
        }
        Statement::Drop { value } => {
            hasher.write_u8(25);
            hash_expression(value, hasher);
        }
        Statement::Move { target, value } => {
            hasher.write_u8(26);
            target.hash(hasher);
            hash_expression(value, hasher);
        }
        Statement::New {
            value,
            declared_type,
        } => {
            hasher.write_u8(32);
            hash_option_expr(value, hasher);
            hash_data_type(declared_type, hasher);
        }
        Statement::Own { value, inner_type } => {
            hasher.write_u8(33);
            hash_option_expr(value, hasher);
            hash_data_type(inner_type, hasher);
        }
        Statement::Enum {
            visibility: _,
            name,
            type_params,
            type_param_bounds,
            variants,
        } => {
            hasher.write_u8(27);
            name.hash(hasher);
            type_params.hash(hasher);
            type_param_bounds.hash(hasher);
            hash_enum_variants(variants, hasher);
        }
        Statement::Query {
            table,
            bindings,
            ops,
            joins,
            group_by,
        } => {
            hasher.write_u8(31);
            table.hash(hasher);
            hash_query_bindings(bindings, hasher);
            hash_query_ops(ops, hasher);
            hash_query_joins(joins, hasher);
            hash_query_group(group_by.as_ref(), hasher);
        }
    }
}

pub(super) fn hash_statements(statements: &[Statement], hasher: &mut FxHasher) {
    statements.len().hash(hasher);
    for stmt in statements {
        hash_statement(stmt, hasher);
    }
}

pub(super) fn hash_optional_statements(statements: &Option<Vec<Statement>>, hasher: &mut FxHasher) {
    match statements {
        Some(stmts) => {
            hasher.write_u8(1);
            hash_statements(stmts, hasher);
        }
        None => hasher.write_u8(0),
    }
}

pub(super) fn hash_assignment_target(target: &AssignmentTarget, hasher: &mut FxHasher) {
    match target {
        AssignmentTarget::Variable(name) => {
            hasher.write_u8(0);
            name.hash(hasher);
        }
        AssignmentTarget::Field(name) => {
            hasher.write_u8(1);
            name.hash(hasher);
        }
        AssignmentTarget::Index { target, index } => {
            hasher.write_u8(2);
            hash_expression(target, hasher);
            hash_expression(index, hasher);
        }
    }
}
