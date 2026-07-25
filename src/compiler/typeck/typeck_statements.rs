use crate::error::{Result, type_error_at_span};
use crate::parser::ast::{
    AssignmentTarget, DataType, Expression, Identifier, Literal, QueryBinding, QueryOp, Statement,
};

use crate::compiler::typeck::typeck_returns::{
    implicit_return_expression_mut, statements_contain_explicit_return,
};
use crate::compiler::typeck::{FunctionSig, TypeChecker};
impl TypeChecker {
    pub(super) fn check_let_statement(
        &mut self,
        name: &str,
        data_type: &mut DataType,
        value: &mut Option<Expression>,
        is_mutable: bool,
    ) -> Result<()> {
        if let Some(expr) = value
            && let Expression::Literal { lit: Literal::Int(int_val), .. } = expr
        {
            Self::validate_int_literal_range(data_type, *int_val, self.current_span.line, self.current_span.column)?;
        }
        let inferred = if let Some(expr) = value {
            self.check_expression(expr)?
        } else {
            DataType::Unknown
        };

        let final_type = if *data_type == DataType::Unknown {
            inferred
        } else {
            if inferred != DataType::Unknown && !self.is_assignable(data_type, &inferred) {
                if crate::types::unify::is_numeric(data_type)
                    && crate::types::unify::is_numeric(&inferred)
                {
                    return Err(crate::types::errors::precision_loss(
                        self.current_span.line,
                        self.current_span.column,
                        data_type,
                        &inferred,
                        Some(&format!("(value :{})", crate::types::errors::pretty(data_type))),
                    ));
                }
                return Err(crate::types::errors::type_mismatch(
                    self.current_span.line,
                    self.current_span.column,
                    data_type,
                    &inferred,
                    "let binding",
                ));
            }
            if let Some(expr) = value.as_ref() {
                Self::validate_explicit_nested_literal(data_type, expr)?;
            }
            data_type.clone()
        };

        *data_type = final_type.clone();
        self.insert_var(name.to_string(), final_type, is_mutable);
        self.refresh_binding_metadata(name, data_type, value.as_ref());
        Ok(())
    }

    pub(super) fn check_assignment_statement(
        &mut self,
        target: &mut AssignmentTarget,
        value: &mut Expression,
    ) -> Result<()> {
        let value_type = self.check_expression(value)?;
        let (mut target_type, is_target_mutable) =
            self.resolve_assignment_target(target)?.ok_or_else(|| {
                type_error_at_span(
                    self.current_span,
                    format!("Assignment to undefined variable '{}'", target),
                )
            })?;

        if !self.is_assignable(&target_type, &value_type) {
            return Err(type_error_at_span(
                self.current_span,
                format!(
                    "Type mismatch in assignment to '{}': expected {:?}, got {:?}",
                    target, target_type, value_type
                ),
            ));
        }

        if !is_target_mutable {
            return Err(type_error_at_span(
                self.current_span,
                format!(
                    "Variable '{}' is not mutable, maybe you meant to use 'mut'",
                    target
                ),
            ));
        }

        match target {
            AssignmentTarget::Field(path) => {
                self.check_field_assignment(path, value, &value_type)?
            }
            AssignmentTarget::Index { .. } => {}
            AssignmentTarget::Variable(name) => {
                Self::validate_explicit_nested_literal(&target_type, value)?;
                target_type = Self::unify_types(&target_type, &value_type)?;
                self.insert_var(name.clone(), target_type, is_target_mutable);
                self.refresh_binding_metadata(name, &value_type, Some(value));
            }
        }

        Ok(())
    }

    fn check_field_assignment(
        &mut self,
        path: &str,
        value: &Expression,
        value_type: &DataType,
    ) -> Result<()> {
        let Some((owner, field_name)) = path.split_once('.') else {
            return Ok(());
        };

        let (owner_type, owner_mutable) = self.lookup_var(owner).ok_or_else(|| {
            type_error_at_span(
                self.current_span,
                format!("Cannot find variable '{}' for field assignment", owner),
            )
        })?;

        if let DataType::StructNamed(ref struct_name) = owner_type
            && let Some(class_sig) = self.classes.get(struct_name)
        {
            let field = class_sig
                .fields
                .iter()
                .find(|f| f.name == field_name)
                .ok_or_else(|| {
                    type_error_at_span(
                        self.current_span,
                        format!("Struct '{}' has no field '{}'", struct_name, field_name),
                    )
                })?;

            if !self.is_assignable(&field.data_type, value_type) {
                return Err(type_error_at_span(
                    self.current_span,
                    format!(
                        "Type mismatch for field '{}': expected {:?}, got {:?}",
                        field_name, field.data_type, value_type
                    ),
                ));
            }

            let mut new_fields: Vec<Expression> = Vec::new();
            for f in &class_sig.fields {
                if f.name == field_name {
                    new_fields.push(value.clone());
                } else {
                    let field_access = Expression::MemberAccess {
                        target: Box::new(Expression::Identifier(Identifier {
                            name: owner.to_string(),
                            data_type: owner_type.clone(),
                            line: 0,
                            column: 0,
                        })),
                        member: f.name.clone(),
                        data_type: f.data_type.clone(),
                    };
                    new_fields.push(field_access);
                }
            }

            let struct_constructor = Expression::Call {
                name: struct_name.clone(),
                args: new_fields,
                type_args: Vec::new(),
                name_line: 0,
            name_column: 0,
            data_type: owner_type.clone(),
            };

            self.insert_var(owner.to_string(), owner_type.clone(), owner_mutable);
            self.refresh_binding_metadata(owner, &owner_type, Some(&struct_constructor));
        }

        Ok(())
    }
}

impl TypeChecker {
    pub(super) fn check_if_statement(
        &mut self,
        condition: &mut Expression,
        then_branch: &mut [Statement],
        else_branch: &mut Option<Vec<Statement>>,
    ) -> Result<()> {
        let cond_type = self.check_expression(condition)?;
        if !Self::is_bool_like(&cond_type) {
            return Err(type_error_at_span(
                self.current_span,
                format!("If condition must be bool, got {:?}", cond_type),
            ));
        }

        self.push_scope();
        self.check_statements(then_branch)?;
        self.pop_scope();

        if let Some(branch) = else_branch {
            self.push_scope();
            self.check_statements(branch)?;
            self.pop_scope();
        }

        Ok(())
    }

    pub(super) fn check_while_statement(
        &mut self,
        condition: &mut Expression,
        body: &mut [Statement],
    ) -> Result<()> {
        let cond_type = self.check_expression(condition)?;
        if !Self::is_bool_like(&cond_type) {
            return Err(type_error_at_span(
                self.current_span,
                format!("While condition must be bool, got {:?}", cond_type),
            ));
        }

        self.push_scope();
        self.check_statements(body)?;
        self.pop_scope();
        Ok(())
    }

    pub(super) fn check_for_statement(
        &mut self,
        variable: &str,
        index: &Option<String>,
        iterable: &mut Expression,
        body: &mut [Statement],
    ) -> Result<()> {
        let iter_type = self.check_expression(iterable)?;
        self.push_scope();

        let item_type = Self::loop_item_type(iterable, iter_type);
        self.insert_var(variable.to_string(), item_type, true);
        if let Some(index_name) = index {
            self.insert_var(index_name.clone(), DataType::I64, true);
        }

        self.check_statements(body)?;
        self.pop_scope();
        Ok(())
    }

    pub(super) fn check_find_statement(
        &mut self,
        variable: &str,
        iterable: &mut Expression,
        body: &mut [Statement],
    ) -> Result<()> {
        let iter_type = self.check_expression(iterable)?;
        self.push_scope();

        let item_type = Self::loop_item_type(iterable, iter_type);
        self.insert_var(variable.to_string(), item_type, true);

        self.check_statements(body)?;
        self.pop_scope();
        Ok(())
    }

    pub(super) fn check_match_statement(
        &mut self,
        value: &mut Expression,
        cases: &mut [(Expression, Vec<Statement>)],
        default: &mut [Statement],
    ) -> Result<()> {
        let value_type = self.check_expression(value)?;
        self.validate_match_coverage(&value_type, cases, !default.is_empty())?;
        for (case_expr, case_body) in cases.iter_mut() {
            if !Self::is_match_identifier_pattern(case_expr) {
                let case_type = self.check_match_pattern(case_expr)?;
                if value_type != DataType::Unknown
                    && case_type != DataType::Unknown
                    && !self.is_assignable(&value_type, &case_type)
                {
                    return Err(type_error_at_span(
                        self.current_span,
                        format!(
                            "Match case type mismatch: value is {:?}, case is {:?}",
                            value_type, case_type
                        ),
                    ));
                }
            }

            self.push_scope();
            self.insert_match_pattern_bindings(case_expr);
            self.check_statements(case_body)?;
            self.pop_scope();
        }

        self.push_scope();
        self.check_statements(default)?;
        self.pop_scope();
        Ok(())
    }

    fn loop_item_type(iterable: &Expression, iter_type: DataType) -> DataType {
        match iterable {
            Expression::Call { name, .. } if name == "range" => DataType::I64,
            _ => match iter_type {
                DataType::Array { element_type, .. } | DataType::Slice { element_type } => {
                    *element_type
                }
                DataType::Tuple => DataType::Anything,
                DataType::List => DataType::Anything,
                DataType::Vector { element_type, .. } => *element_type,
                DataType::Str => DataType::Str,
                _ => DataType::Anything,
            },
        }
    }
}

impl TypeChecker {
    pub(super) fn check_function_statement(
        &mut self,
        name: &str,
        type_params: &[String],
        type_param_bounds: &[(String, Vec<String>)],
        params: &[(String, DataType)],
        body: &mut [Statement],
        return_type: &mut DataType,
    ) -> Result<()> {
        self.functions.insert(
            name.to_string(),
            FunctionSig {
                type_params: type_params.to_vec(),
                type_param_bounds: type_param_bounds.to_vec(),
                params: params.iter().map(|(_, t)| t.clone()).collect(),
                return_type: return_type.clone(),
            },
        );

        self.push_scope();
        for (param_name, param_type) in params.iter() {
            self.insert_var(param_name.clone(), param_type.clone(), true);
            self.refresh_binding_metadata(param_name, param_type, None);
        }

        self.return_type_stack.push(return_type.clone());
        self.check_statements(body)?;
        if *return_type != DataType::None
            && !statements_contain_explicit_return(body)
            && let Some(expr) = implicit_return_expression_mut(body)
        {
            let tail_type = self.check_expression(expr)?;
            if let Some(current) = self.return_type_stack.last_mut() {
                let unified = Self::unify_types(current, &tail_type)?;
                *current = unified;
            }
        }
        let inferred_return = self.return_type_stack.pop().unwrap_or(DataType::Unknown);

        if *return_type == DataType::Unknown || *return_type == DataType::None {
            *return_type = inferred_return.clone();
        } else if inferred_return != DataType::Unknown
            && !self.is_assignable(return_type, &inferred_return)
        {
            return Err(type_error_at_span(
                self.current_span,
                format!(
                    "Function '{}' return type mismatch: declared {:?}, inferred {:?}",
                    name, return_type, inferred_return
                ),
            ));
        }

        self.pop_scope();

        if let Some(sig) = self.functions.get_mut(name) {
            sig.return_type = return_type.clone();
        }

        Ok(())
    }

    pub(super) fn check_return_statement(&mut self, expr: &mut Option<Expression>) -> Result<()> {
        let return_type = if let Some(expression) = expr {
            self.check_expression(expression)?
        } else {
            DataType::None
        };

        if let Some(current) = self.return_type_stack.last_mut() {
            let unified = Self::unify_types(current, &return_type)?;
            *current = unified;
        }

        Ok(())
    }
}

impl TypeChecker {
    pub(super) fn check_scoped_body(&mut self, body: &mut [Statement]) -> Result<()> {
        self.push_scope();
        self.check_statements(body)?;
        self.pop_scope();
        Ok(())
    }

    pub(super) fn check_asm_statement(
        &mut self,
        instructions: &mut [(String, Expression)],
    ) -> Result<()> {
        for (_, expr) in instructions.iter_mut() {
            self.check_expression(expr)?;
        }
        Ok(())
    }

    pub(super) fn check_drop_statement(&mut self, value: &mut Expression) -> Result<()> {
        self.check_expression(value)?;
        Ok(())
    }

    pub(super) fn check_new_statement(
        &mut self,
        value: &mut Option<Expression>,
        declared_type: &DataType,
    ) -> Result<()> {
        self.validate_new_target_type(declared_type)?;
        if let Some(initial) = value {
            let initial_ty = self.check_expression(initial)?;
            if !self.is_assignable(declared_type, &initial_ty) {
                return Err(type_error_at_span(
                    self.current_span,
                    format!(
                        "new:: value type mismatch: declared {:?}, got {:?}",
                        declared_type, initial_ty
                    ),
                ));
            }
        }
        Ok(())
    }

    pub(super) fn check_own_statement(
        &mut self,
        value: &mut Option<Expression>,
        inner_type: &DataType,
    ) -> Result<()> {
        self.validate_own_target_type(inner_type)?;
        if let Some(initial) = value {
            let initial_ty = self.check_expression(initial)?;
            if !self.is_assignable(inner_type, &initial_ty) {
                return Err(type_error_at_span(
                    self.current_span,
                    format!(
                        "own:: value type mismatch: declared {:?}, got {:?}",
                        inner_type, initial_ty
                    ),
                ));
            }
        }
        Ok(())
    }

    pub(super) fn check_move_statement(
        &mut self,
        target: &str,
        value: &mut Expression,
    ) -> Result<()> {
        let moved_type = self.check_expression(value)?;
        self.insert_var(target.to_string(), moved_type.clone(), true);
        self.refresh_binding_metadata(target, &moved_type, Some(value));
        Ok(())
    }

    pub(super) fn check_query_statement(
        &mut self,
        ops: &mut [QueryOp],
        bindings: &[QueryBinding],
    ) -> Result<()> {
        for bind in bindings {
            self.insert_var(bind.target.clone(), DataType::Anything, true);
            self.insert_var(bind.alias.clone(), DataType::Anything, true);
        }

        for op in ops.iter_mut() {
            self.check_query_op(op)?;
        }

        Ok(())
    }

    fn check_query_op(&mut self, op: &mut QueryOp) -> Result<()> {
        match op {
            QueryOp::Insert { assigns } => {
                for (_, expr) in assigns.iter_mut() {
                    self.check_expression(expr)?;
                }
            }
            QueryOp::Update { condition, assigns } => {
                let cond_type = self.check_expression(condition)?;
                if !Self::is_bool_like(&cond_type) {
                    return Err(type_error_at_span(
                        self.current_span,
                        format!("Query update condition must be bool, got {:?}", cond_type),
                    ));
                }
                for (_, expr) in assigns.iter_mut() {
                    self.check_expression(expr)?;
                }
            }
            QueryOp::Delete { condition } => {
                let cond_type = self.check_expression(condition)?;
                if !Self::is_bool_like(&cond_type) {
                    return Err(type_error_at_span(
                        self.current_span,
                        format!("Query delete condition must be bool, got {:?}", cond_type),
                    ));
                }
            }
            QueryOp::Get(get) => {
                let cond_type = self.check_expression(&mut get.condition)?;
                if !Self::is_bool_like(&cond_type) {
                    return Err(type_error_at_span(
                        self.current_span,
                        format!("Query get condition must be bool, got {:?}", cond_type),
                    ));
                }

                self.push_scope();
                self.insert_var(get.target.clone(), DataType::Anything, true);
                self.check_statements(&mut get.body)?;
                self.pop_scope();
            }
            QueryOp::Export { .. } | QueryOp::Import { .. } => {}
        }

        Ok(())
    }
}

impl TypeChecker {
    pub(super) fn check_impl_statement(
        &mut self,
        trait_name: &Option<String>,
        type_name: &str,
        methods: &mut [Statement],
    ) -> Result<()> {
        self.validate_impl_method_declarations(type_name, methods)?;
        if let Some(trait_name) = trait_name {
            self.validate_trait_impl(trait_name, type_name, methods)?;
        }

        let old_self = self.impl_self_type.take();
        let old_self_name = self.impl_self_name.take();
        let method_mask = self
            .current_nested_statement_mask()
            .map(|mask| mask.to_vec());

        for (method_index, method) in methods.iter_mut().enumerate() {
            if method_mask
                .as_ref()
                .and_then(|mask| mask.get(method_index))
                .is_some_and(|should_check| !should_check)
            {
                continue;
            }
            let has_self = matches!(
                method,
                Statement::Function { params, .. }
                    if params.iter().any(|(param_name, _)| param_name == "self")
            );
            self.impl_self_type = has_self.then(|| DataType::StructNamed(type_name.to_string()));
            self.impl_self_name = has_self.then(|| type_name.to_string());
            self.check_statement(method)?;
        }

        self.impl_self_type = old_self;
        self.impl_self_name = old_self_name;
        Ok(())
    }

    pub(super) fn check_type_statement(&mut self, fields: &mut [Statement]) -> Result<()> {
        self.check_container_statements(fields)
    }

    pub(super) fn check_skill_statement(
        &mut self,
        name: &str,
        methods: &[crate::parser::ast::TraitMethodSig],
    ) -> Result<()> {
        if methods.is_empty() {
            return Err(type_error_at_span(
                self.current_span,
                format!("Skill '{}' must declare at least one method", name),
            ));
        }
        self.validate_trait_method_declarations(name, methods, "Skill")?;
        Ok(())
    }
}
