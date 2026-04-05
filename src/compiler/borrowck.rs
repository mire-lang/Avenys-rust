use std::collections::HashMap;

use crate::compiler::semantic::{BindingInfo, BindingKind, SemanticModel};
use crate::error::mss::MssError;
use crate::error::{MireError, Result};
use crate::parser::ast::{DataType, Expression, Program, QueryOp, Statement};

pub fn check_program(program: &Program, semantic_model: &SemanticModel) -> Result<()> {
    let mut checker = BorrowChecker::new(semantic_model);
    checker.check_statements(&program.statements)
}

#[derive(Debug, Clone, Default)]
struct BindingState {
    is_moved: bool,
    immutable_borrows: usize,
    mutable_borrow: bool,
    ref_target: Option<ReferenceBinding>,
}

#[derive(Debug, Clone)]
struct ReferenceBinding {
    target: String,
    is_mutable: bool,
}

struct BorrowChecker {
    semantic_model: SemanticModel,
    scopes: Vec<HashMap<String, BindingState>>,
    unsafe_depth: usize,
    function_stack: Vec<FunctionContext>,
}

#[derive(Debug, Clone)]
struct FunctionContext {
    scope_id: usize,
}

impl BorrowChecker {
    fn new(semantic_model: &SemanticModel) -> Self {
        Self {
            semantic_model: semantic_model.clone(),
            scopes: vec![HashMap::new()],
            unsafe_depth: 0,
            function_stack: Vec::new(),
        }
    }

    fn check_statements(&mut self, statements: &[Statement]) -> Result<()> {
        for statement in statements {
            self.check_statement(statement)?;
        }
        Ok(())
    }

    fn check_statement(&mut self, statement: &Statement) -> Result<()> {
        match statement {
            Statement::Let { name, value, .. } => {
                if let Some(value) = value {
                    self.check_expression(value)?;
                }
                let mut state = BindingState::default();
                if let Some((target, is_mutable)) = Self::reference_target(value.as_ref()) {
                    self.register_borrow(&target, is_mutable)?;
                    state.ref_target = Some(ReferenceBinding { target, is_mutable });
                }
                self.insert_binding(name.clone(), state);
            }
            Statement::Assignment { target, value, .. } => {
                self.ensure_binding_available(target)?;
                self.ensure_can_write(target)?;
                self.check_expression(value)?;
                self.rebind_reference_target(target, Self::reference_target(Some(value)))?;
                if let Some(state) = self.lookup_binding_mut(target) {
                    state.is_moved = false;
                }
            }
            Statement::Function { params, body, .. } => {
                let scope_id = self
                    .current_function_scope_id(statement)
                    .unwrap_or_else(|| self.current_scope_depth() + 1);
                self.push_scope();
                self.function_stack.push(FunctionContext { scope_id });
                for (name, _) in params {
                    self.insert_binding(name.clone(), BindingState::default());
                }
                let result = self.check_statements(body);
                self.function_stack.pop();
                self.pop_scope();
                result?;
            }
            Statement::Return(expr) => {
                if let Some(expr) = expr {
                    self.check_expression(expr)?;
                    self.ensure_return_is_safe(expr)?;
                }
            }
            Statement::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.check_expression(condition)?;
                self.push_scope();
                self.check_statements(then_branch)?;
                self.pop_scope();
                if let Some(else_branch) = else_branch {
                    self.push_scope();
                    self.check_statements(else_branch)?;
                    self.pop_scope();
                }
            }
            Statement::While { condition, body } => {
                self.check_expression(condition)?;
                self.push_scope();
                self.check_statements(body)?;
                self.pop_scope();
            }
            Statement::For {
                variable,
                iterable,
                body,
            }
            | Statement::Find {
                variable,
                iterable,
                body,
            } => {
                self.check_expression(iterable)?;
                self.push_scope();
                self.insert_binding(variable.clone(), BindingState::default());
                self.check_statements(body)?;
                self.pop_scope();
            }
            Statement::Expression(expr) => {
                self.check_expression(expr)?;
            }
            Statement::Match {
                value,
                cases,
                default,
            } => {
                self.check_expression(value)?;
                for (case_expr, case_body) in cases {
                    self.check_expression(case_expr)?;
                    self.push_scope();
                    self.check_statements(case_body)?;
                    self.pop_scope();
                }
                self.push_scope();
                self.check_statements(default)?;
                self.pop_scope();
            }
            Statement::Class { .. }
            | Statement::Impl { .. }
            | Statement::Type { .. }
            | Statement::Code { .. }
            | Statement::Skill { .. } => {}
            Statement::Unsafe { body } => {
                self.unsafe_depth += 1;
                self.push_scope();
                let result = self.check_statements(body);
                self.pop_scope();
                self.unsafe_depth = self.unsafe_depth.saturating_sub(1);
                result?;
            }
            Statement::Asm { instructions } => {
                for (_, expr) in instructions {
                    self.check_expression(expr)?;
                }
            }
            Statement::Module { body, .. }
            | Statement::DmireTable { body, .. }
            | Statement::DmireColumn { body, .. } => {
                self.push_scope();
                self.check_statements(body)?;
                self.pop_scope();
            }
            Statement::Drop { value } => {
                self.check_expression(value)?;
                if let Some(name) = Self::identifier_name(value) {
                    self.ensure_can_drop(&name)?;
                    if let Some(state) = self.lookup_binding_mut(&name) {
                        state.is_moved = true;
                    }
                }
            }
            Statement::Move { target, value } => {
                self.check_expression(value)?;
                if let Some(source) = Self::identifier_name(value) {
                    self.ensure_can_move(&source)?;
                    if let Some(state) = self.lookup_binding_mut(&source) {
                        state.is_moved = true;
                    }
                }
                self.insert_binding(target.clone(), BindingState::default());
            }
            Statement::Query { bindings, ops, .. } => {
                self.push_scope();
                for binding in bindings {
                    self.insert_binding(binding.target.clone(), BindingState::default());
                    self.insert_binding(binding.alias.clone(), BindingState::default());
                }
                for op in ops {
                    self.check_query_op(op)?;
                }
                self.pop_scope();
            }
            Statement::DmireDlist { data, .. } => {
                for expr in data {
                    self.check_expression(expr)?;
                }
            }
            Statement::Break
            | Statement::Continue
            | Statement::Trait { .. }
            | Statement::ExternLib { .. }
            | Statement::ExternFunction { .. }
            | Statement::AddLib { .. }
            | Statement::Use { .. }
            | Statement::Enum { .. } => {}
        }

        Ok(())
    }

    fn check_query_op(&mut self, op: &QueryOp) -> Result<()> {
        match op {
            QueryOp::Insert { assigns } => {
                for (_, expr) in assigns {
                    self.check_expression(expr)?;
                }
            }
            QueryOp::Update { condition, assigns } => {
                self.check_expression(condition)?;
                for (_, expr) in assigns {
                    self.check_expression(expr)?;
                }
            }
            QueryOp::Delete { condition } => {
                self.check_expression(condition)?;
            }
            QueryOp::Get(get) => {
                self.check_expression(&get.condition)?;
                self.push_scope();
                self.insert_binding(get.target.clone(), BindingState::default());
                self.check_statements(&get.body)?;
                self.pop_scope();
            }
            QueryOp::Export { .. } | QueryOp::Import { .. } => {}
        }
        Ok(())
    }

    fn check_expression(&mut self, expression: &Expression) -> Result<()> {
        match expression {
            Expression::Literal(_) => {}
            Expression::Identifier(ident) => {
                self.ensure_binding_available(&ident.name)?;
            }
            Expression::BinaryOp { left, right, .. } => {
                self.check_expression(left)?;
                self.check_expression(right)?;
            }
            Expression::UnaryOp { operand, .. } => {
                self.check_expression(operand)?;
            }
            Expression::NamedArg { value, .. } => {
                self.check_expression(value)?;
            }
            Expression::Call { name, args, .. } => {
                for (index, arg) in args.iter().enumerate() {
                    self.check_expression(arg)?;
                    self.check_call_argument(name, index, arg)?;
                }
            }
            Expression::List { elements: args, .. } | Expression::Tuple { elements: args, .. } => {
                for arg in args {
                    self.check_expression(arg)?;
                }
            }
            Expression::Dict { entries, .. } => {
                for (key, value) in entries {
                    self.check_expression(key)?;
                    self.check_expression(value)?;
                }
            }
            Expression::Index { target, index, .. } => {
                self.check_expression(target)?;
                self.check_expression(index)?;
            }
            Expression::MemberAccess { target, .. } => {
                self.check_expression(target)?;
            }
            Expression::Closure { body, .. } => {
                self.push_scope();
                self.check_statements(body)?;
                self.pop_scope();
            }
            Expression::Reference {
                expr, is_mutable, ..
            } => {
                self.check_expression(expr)?;
                if let Some(name) = Self::identifier_name(expr) {
                    self.ensure_borrow_allowed(&name, *is_mutable)?;
                }
            }
            Expression::Dereference { expr, .. } | Expression::Box { value: expr, .. } => {
                self.check_expression(expr)?;
            }
        }
        Ok(())
    }

    fn ensure_binding_available(&self, name: &str) -> Result<()> {
        if let Some(state) = self.lookup_binding(name) {
            if state.is_moved {
                return Err(ownership_error(MssError::UseAfterMove));
            }
        }
        Ok(())
    }

    fn ensure_can_write(&self, name: &str) -> Result<()> {
        if self.unsafe_depth > 0 {
            return Ok(());
        }
        let state = self
            .lookup_binding(name)
            .ok_or_else(|| ownership_error(MssError::UseAfterMove))?;
        if state.mutable_borrow || state.immutable_borrows > 0 {
            return Err(ownership_error(MssError::MutationWhileShared));
        }
        Ok(())
    }

    fn ensure_can_move(&self, name: &str) -> Result<()> {
        let state = self
            .lookup_binding(name)
            .ok_or_else(|| ownership_error(MssError::UseAfterMove))?;
        if state.is_moved {
            return Err(ownership_error(MssError::UseAfterMove));
        }
        if self.unsafe_depth == 0 && (state.mutable_borrow || state.immutable_borrows > 0) {
            return Err(ownership_error(MssError::MoveWhileBorrowed));
        }
        Ok(())
    }

    fn ensure_can_drop(&self, name: &str) -> Result<()> {
        let state = self
            .lookup_binding(name)
            .ok_or_else(|| ownership_error(MssError::UseAfterMove))?;
        if state.is_moved {
            return Err(ownership_error(MssError::UseAfterMove));
        }
        if self.unsafe_depth == 0 && (state.mutable_borrow || state.immutable_borrows > 0) {
            return Err(ownership_error(MssError::DropWhileBorrowed));
        }
        Ok(())
    }

    fn ensure_borrow_allowed(&self, name: &str, is_mutable: bool) -> Result<()> {
        if self.unsafe_depth > 0 {
            return Ok(());
        }
        let state = self
            .lookup_binding(name)
            .ok_or_else(|| ownership_error(MssError::UseAfterMove))?;
        if state.is_moved {
            return Err(ownership_error(MssError::UseAfterMove));
        }
        if is_mutable {
            if state.mutable_borrow {
                return Err(ownership_error(MssError::MultipleMutableRefs));
            }
            if state.immutable_borrows > 0 {
                return Err(ownership_error(MssError::MutationWhileShared));
            }
        } else if state.mutable_borrow {
            return Err(ownership_error(MssError::MutationWhileShared));
        }
        Ok(())
    }

    fn register_borrow(&mut self, name: &str, is_mutable: bool) -> Result<()> {
        self.ensure_borrow_allowed(name, is_mutable)?;
        let state = self
            .lookup_binding_mut(name)
            .ok_or_else(|| ownership_error(MssError::UseAfterMove))?;
        if is_mutable {
            state.mutable_borrow = true;
        } else {
            state.immutable_borrows += 1;
        }
        Ok(())
    }

    fn release_borrow(&mut self, name: &str, is_mutable: bool) {
        if let Some(state) = self.lookup_binding_mut(name) {
            if is_mutable {
                state.mutable_borrow = false;
            } else if state.immutable_borrows > 0 {
                state.immutable_borrows -= 1;
            }
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        if self.scopes.len() <= 1 {
            return;
        }

        if let Some(scope) = self.scopes.pop() {
            for (_, binding) in scope {
                if let Some(reference) = binding.ref_target {
                    self.release_borrow(&reference.target, reference.is_mutable);
                }
            }
        }
    }

    fn insert_binding(&mut self, name: String, state: BindingState) {
        let previous = self
            .scopes
            .last_mut()
            .and_then(|scope| scope.insert(name, state));
        if let Some(previous) = previous.and_then(|binding| binding.ref_target) {
            self.release_borrow(&previous.target, previous.is_mutable);
        }
    }

    fn rebind_reference_target(
        &mut self,
        name: &str,
        new_reference: Option<(String, bool)>,
    ) -> Result<()> {
        let old_reference = self
            .lookup_binding(name)
            .and_then(|state| state.ref_target.clone());
        if let Some(old_reference) = old_reference {
            self.release_borrow(&old_reference.target, old_reference.is_mutable);
        }

        if let Some((target, is_mutable)) = new_reference {
            self.register_borrow(&target, is_mutable)?;
            if let Some(state) = self.lookup_binding_mut(name) {
                state.ref_target = Some(ReferenceBinding { target, is_mutable });
            }
        } else if let Some(state) = self.lookup_binding_mut(name) {
            state.ref_target = None;
        }

        Ok(())
    }

    fn lookup_binding(&self, name: &str) -> Option<&BindingState> {
        for scope in self.scopes.iter().rev() {
            if let Some(binding) = scope.get(name) {
                return Some(binding);
            }
        }
        None
    }

    fn lookup_binding_mut(&mut self, name: &str) -> Option<&mut BindingState> {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(binding) = scope.get_mut(name) {
                return Some(binding);
            }
        }
        None
    }

    fn current_scope_depth(&self) -> usize {
        self.scopes.len().saturating_sub(1)
    }

    fn current_function_scope_id(&self, statement: &Statement) -> Option<usize> {
        let Statement::Function { name, .. } = statement else {
            return None;
        };
        self.semantic_model
            .functions
            .get(name)
            .map(|info| info.scope_id)
    }

    fn ensure_return_is_safe(&self, expression: &Expression) -> Result<()> {
        let Some(function_context) = self.function_stack.last() else {
            return Ok(());
        };

        if let Some((target, is_mutable)) = Self::reference_target(Some(expression)) {
            let binding = self
                .semantic_binding(&target)
                .ok_or_else(|| ownership_error(MssError::BorrowOutOfScope))?;

            let is_reference_binding = matches!(
                binding.kind,
                BindingKind::SharedRef | BindingKind::MutableRef
            );
            let same_function_scope = binding.scope_id >= function_context.scope_id;

            if same_function_scope && !is_reference_binding {
                return Err(ownership_error(if is_mutable {
                    MssError::UnsafeViolation
                } else {
                    MssError::BorrowOutOfScope
                }));
            }
        }

        Ok(())
    }

    fn check_call_argument(&mut self, callee: &str, index: usize, arg: &Expression) -> Result<()> {
        let Some(function) = self.semantic_model.functions.get(callee) else {
            return Ok(());
        };
        let Some(expected) = function.params.get(index) else {
            return Ok(());
        };

        match expected {
            DataType::Ref => {
                if let Some((target, is_mutable)) = Self::reference_target(Some(arg)) {
                    if is_mutable {
                        return Err(ownership_error(MssError::MultipleMutableRefs));
                    }
                    self.ensure_borrow_allowed(&target, false)?;
                } else if let Some(binding) =
                    Self::identifier_name(arg).and_then(|name| self.semantic_binding(&name))
                {
                    if !matches!(
                        binding.kind,
                        BindingKind::SharedRef | BindingKind::MutableRef
                    ) {
                        return Err(MireError::type_error(format!(
                            "Function '{}' argument {} requires a shared reference",
                            callee,
                            index + 1
                        )));
                    }
                }
            }
            DataType::RefMut => {
                if let Some((target, is_mutable)) = Self::reference_target(Some(arg)) {
                    if !is_mutable {
                        return Err(MireError::type_error(format!(
                            "Function '{}' argument {} requires a mutable reference",
                            callee,
                            index + 1
                        )));
                    }
                    self.ensure_borrow_allowed(&target, true)?;
                } else if let Some(binding) =
                    Self::identifier_name(arg).and_then(|name| self.semantic_binding(&name))
                {
                    if !matches!(binding.kind, BindingKind::MutableRef) {
                        return Err(MireError::type_error(format!(
                            "Function '{}' argument {} requires a mutable reference",
                            callee,
                            index + 1
                        )));
                    }
                }
            }
            _ => {
                if let Some(name) = Self::identifier_name(arg) {
                    let should_consume = self
                        .semantic_binding(&name)
                        .map(|binding| Self::is_move_type(&binding.data_type))
                        .unwrap_or(false);

                    if let Some(state) = self.lookup_binding(&name) {
                        if self.unsafe_depth == 0
                            && (state.mutable_borrow || state.immutable_borrows > 0)
                        {
                            return Err(ownership_error(MssError::MoveWhileBorrowed));
                        }
                    }

                    if should_consume {
                        self.ensure_can_move(&name)?;
                        if let Some(state) = self.lookup_binding_mut(&name) {
                            state.is_moved = true;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn semantic_binding(&self, name: &str) -> Option<&BindingInfo> {
        let indexes = self.semantic_model.bindings_by_name.get(name)?;
        indexes
            .iter()
            .rev()
            .find_map(|index| self.semantic_model.bindings.get(*index))
    }

    fn is_move_type(data_type: &DataType) -> bool {
        matches!(
            data_type,
            DataType::Str
                | DataType::List
                | DataType::Vector { .. }
                | DataType::Dict
                | DataType::Map { .. }
                | DataType::Box
        )
    }

    fn is_copy_type(data_type: &DataType) -> bool {
        !Self::is_move_type(data_type)
    }

    fn reference_target(expression: Option<&Expression>) -> Option<(String, bool)> {
        match expression? {
            Expression::Reference {
                expr, is_mutable, ..
            } => Self::identifier_name(expr).map(|name| (name, *is_mutable)),
            _ => None,
        }
    }

    fn identifier_name(expression: &Expression) -> Option<String> {
        match expression {
            Expression::Identifier(ident) => Some(ident.name.clone()),
            _ => None,
        }
    }
}

fn ownership_error(kind: MssError) -> MireError {
    MireError::ownership_error(1, 1, kind)
}

#[cfg(test)]
mod tests {
    use super::check_program;
    use crate::compiler::semantic;
    use crate::parser::ast::{
        DataType, Expression, Identifier, Literal, Program, Statement, Visibility,
    };

    fn let_stmt(name: &str, value: Option<Expression>) -> Statement {
        Statement::Let {
            name: name.to_string(),
            data_type: DataType::Unknown,
            value,
            is_constant: false,
            is_static: false,
            visibility: Visibility::Public,
        }
    }

    fn ident(name: &str) -> Expression {
        Expression::Identifier(Identifier {
            name: name.to_string(),
            data_type: DataType::Unknown,
        })
    }

    #[test]
    fn rejects_assignment_while_shared_borrow_exists() {
        let program = Program {
            statements: vec![
                let_stmt("x", Some(Expression::Literal(Literal::Int(1)))),
                let_stmt(
                    "r",
                    Some(Expression::Reference {
                        expr: Box::new(ident("x")),
                        is_mutable: false,
                        data_type: DataType::Unknown,
                    }),
                ),
                Statement::Assignment {
                    target: "x".to_string(),
                    value: Expression::Literal(Literal::Int(2)),
                    is_mutable: true,
                },
            ],
        };

        let semantic_model = semantic::analyze_program(&program);
        let err = check_program(&program, &semantic_model).unwrap_err();
        assert!(format!("{}", err).contains("Cannot mutate"));
    }

    #[test]
    fn rejects_mutable_borrow_while_shared_borrow_exists() {
        let program = Program {
            statements: vec![
                let_stmt("x", Some(Expression::Literal(Literal::Int(1)))),
                let_stmt(
                    "r",
                    Some(Expression::Reference {
                        expr: Box::new(ident("x")),
                        is_mutable: false,
                        data_type: DataType::Unknown,
                    }),
                ),
                let_stmt(
                    "m",
                    Some(Expression::Reference {
                        expr: Box::new(ident("x")),
                        is_mutable: true,
                        data_type: DataType::Unknown,
                    }),
                ),
            ],
        };

        let semantic_model = semantic::analyze_program(&program);
        let err = check_program(&program, &semantic_model).unwrap_err();
        assert!(format!("{}", err).contains("Cannot mutate"));
    }

    #[test]
    fn rejects_use_after_move() {
        let program = Program {
            statements: vec![
                let_stmt("x", Some(Expression::Literal(Literal::Int(1)))),
                Statement::Move {
                    target: "y".to_string(),
                    value: ident("x"),
                },
                Statement::Expression(ident("x")),
            ],
        };

        let semantic_model = semantic::analyze_program(&program);
        let err = check_program(&program, &semantic_model).unwrap_err();
        assert!(format!("{}", err).contains("Use after move"));
    }

    #[test]
    fn releases_borrow_on_scope_exit() {
        let program = Program {
            statements: vec![
                let_stmt("x", Some(Expression::Literal(Literal::Int(1)))),
                Statement::If {
                    condition: Expression::Literal(Literal::Bool(true)),
                    then_branch: vec![let_stmt(
                        "r",
                        Some(Expression::Reference {
                            expr: Box::new(ident("x")),
                            is_mutable: false,
                            data_type: DataType::Unknown,
                        }),
                    )],
                    else_branch: None,
                },
                Statement::Assignment {
                    target: "x".to_string(),
                    value: Expression::Literal(Literal::Int(2)),
                    is_mutable: true,
                },
            ],
        };

        let semantic_model = semantic::analyze_program(&program);
        check_program(&program, &semantic_model)
            .expect("borrow should be released when scope ends");
    }

    #[test]
    fn unsafe_allows_write_while_borrowed() {
        let program = Program {
            statements: vec![
                let_stmt("x", Some(Expression::Literal(Literal::Int(1)))),
                let_stmt(
                    "r",
                    Some(Expression::Reference {
                        expr: Box::new(ident("x")),
                        is_mutable: false,
                        data_type: DataType::Unknown,
                    }),
                ),
                Statement::Unsafe {
                    body: vec![Statement::Assignment {
                        target: "x".to_string(),
                        value: Expression::Literal(Literal::Int(2)),
                        is_mutable: true,
                    }],
                },
            ],
        };

        let semantic_model = semantic::analyze_program(&program);
        check_program(&program, &semantic_model)
            .expect("unsafe block should bypass borrow conflict checks");
    }

    #[test]
    fn rejects_returning_reference_to_local_binding() {
        let program = Program {
            statements: vec![Statement::Function {
                name: "bad".to_string(),
                params: vec![],
                body: vec![
                    let_stmt("x", Some(Expression::Literal(Literal::Int(1)))),
                    Statement::Return(Some(Expression::Reference {
                        expr: Box::new(ident("x")),
                        is_mutable: false,
                        data_type: DataType::Unknown,
                    })),
                ],
                return_type: DataType::Ref,
                visibility: Visibility::Public,
                is_method: false,
            }],
        };

        let semantic_model = semantic::analyze_program(&program);
        let err = check_program(&program, &semantic_model).unwrap_err();
        assert!(format!("{}", err).contains("Borrow outlives owner scope"));
    }

    #[test]
    fn rejects_call_that_requires_mut_ref_but_receives_shared_ref() {
        let program = Program {
            statements: vec![
                Statement::Function {
                    name: "mutate".to_string(),
                    params: vec![("value".to_string(), DataType::RefMut)],
                    body: vec![],
                    return_type: DataType::None,
                    visibility: Visibility::Public,
                    is_method: false,
                },
                let_stmt("x", Some(Expression::Literal(Literal::Int(1)))),
                Statement::Expression(Expression::Call {
                    name: "mutate".to_string(),
                    args: vec![Expression::Reference {
                        expr: Box::new(ident("x")),
                        is_mutable: false,
                        data_type: DataType::Unknown,
                    }],
                    data_type: DataType::Unknown,
                }),
            ],
        };

        let semantic_model = semantic::analyze_program(&program);
        let err = check_program(&program, &semantic_model).unwrap_err();
        assert!(format!("{}", err).contains("mutable reference"));
    }

    #[test]
    fn passing_moved_type_by_value_consumes_binding() {
        let program = Program {
            statements: vec![
                Statement::Function {
                    name: "consume".to_string(),
                    params: vec![("name".to_string(), DataType::Str)],
                    body: vec![],
                    return_type: DataType::None,
                    visibility: Visibility::Public,
                    is_method: false,
                },
                Statement::Let {
                    name: "name".to_string(),
                    data_type: DataType::Str,
                    value: Some(Expression::Literal(Literal::Str("mire".to_string()))),
                    is_constant: false,
                    is_static: false,
                    visibility: Visibility::Public,
                },
                Statement::Expression(Expression::Call {
                    name: "consume".to_string(),
                    args: vec![ident("name")],
                    data_type: DataType::Unknown,
                }),
                Statement::Expression(ident("name")),
            ],
        };

        let semantic_model = semantic::analyze_program(&program);
        let err = check_program(&program, &semantic_model).unwrap_err();
        assert!(format!("{}", err).contains("Use after move"));
    }

    #[test]
    fn passing_copy_type_by_value_does_not_consume_binding() {
        let program = Program {
            statements: vec![
                Statement::Function {
                    name: "show".to_string(),
                    params: vec![("value".to_string(), DataType::I64)],
                    body: vec![],
                    return_type: DataType::None,
                    visibility: Visibility::Public,
                    is_method: false,
                },
                let_stmt("x", Some(Expression::Literal(Literal::Int(1)))),
                Statement::Expression(Expression::Call {
                    name: "show".to_string(),
                    args: vec![ident("x")],
                    data_type: DataType::Unknown,
                }),
                Statement::Expression(ident("x")),
            ],
        };

        let semantic_model = semantic::analyze_program(&program);
        check_program(&program, &semantic_model).expect("copy-like values should remain usable");
    }
}
