mod typeck_check_expression;
mod typeck_closures;
mod typeck_enums;
mod typeck_expressions;
mod typeck_generics;
mod typeck_resolve;
mod typeck_returns;
mod typeck_scope;
mod typeck_signatures;
mod typeck_statements;
mod typeck_type_parsing;
mod typeck_types;
mod typeck_validate;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use self::typeck_returns::{implicit_return_expression_mut, statements_contain_explicit_return};
use crate::compiler::{AnalysisSelection, location};
use crate::error::{MireError, Result};
use crate::incremental::analysis_unit_key;
use crate::parser::ast::{
    AssignmentTarget, DataType, Expression, Identifier, Literal, Program, Statement, TraitMethodSig,
};
#[derive(Debug, Clone)]
struct FunctionSig {
    type_params: Vec<String>,
    type_param_bounds: Vec<(String, Vec<String>)>,
    params: Vec<DataType>,
    return_type: DataType,
}

#[derive(Debug, Clone)]
struct ClassFieldSig {
    name: String,
    data_type: DataType,
    has_default: bool,
}

#[derive(Debug, Clone)]
struct ClassSig {
    type_params: Vec<String>,
    type_param_bounds: Vec<(String, Vec<String>)>,
    fields: Vec<ClassFieldSig>,
}

#[derive(Debug, Clone)]
struct EnumVariantSig {
    type_params: Vec<String>,
    type_param_bounds: Vec<(String, Vec<String>)>,
    payload_names: Vec<String>,
    payload_types: Vec<DataType>,
}

#[derive(Debug, Clone)]
struct TraitSig {
    methods: Vec<TraitMethodSig>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MethodKind {
    Instance,
    Associated,
}

pub fn check_program_types(program: &mut Program, source: &str) -> Result<()> {
    let mut checker = TypeChecker::new(source);
    checker
        .collect_function_signatures(&program.statements)
        .map_err(|err| checker.attach_current_context(err))?;
    checker
        .collect_function_return_signatures(&program.statements)
        .map_err(|err| checker.attach_current_context(err))?;
    checker.check_top_level_statements(&mut program.statements)
}

pub fn check_program_types_with_origins(
    program: &mut Program,
    source: &str,
    statement_origins: &[PathBuf],
    sources: &HashMap<PathBuf, String>,
) -> Result<()> {
    check_program_types_partial_with_origins(
        program,
        source,
        statement_origins,
        sources,
        &AnalysisSelection::full(program),
    )
}

pub fn check_program_types_partial_with_origins(
    program: &mut Program,
    source: &str,
    statement_origins: &[PathBuf],
    sources: &HashMap<PathBuf, String>,
    selection: &AnalysisSelection,
) -> Result<()> {
    let mut checker = TypeChecker::new(source);
    checker.statement_origins = statement_origins
        .iter()
        .map(|path| path.display().to_string())
        .collect();
    checker.sources_by_filename = sources
        .iter()
        .map(|(path, source)| (path.display().to_string(), source.clone()))
        .collect();
    checker.nested_statement_masks = selection.nested_statement_masks.clone();
    checker
        .collect_function_signatures(&program.statements)
        .map_err(|err| checker.attach_current_context(err))?;
    checker
        .collect_function_return_signatures(&program.statements)
        .map_err(|err| checker.attach_current_context(err))?;
    checker.check_selected_top_level_statements(&mut program.statements, &selection.statement_mask)
}

struct TypeChecker {
    scopes: Vec<HashMap<String, (DataType, bool)>>,
    struct_scopes: Vec<HashMap<String, String>>,
    ref_scopes: Vec<HashMap<String, DataType>>,
    function_alias_scopes: Vec<HashMap<String, String>>,
    function_value_sig_scopes: Vec<HashMap<String, FunctionSig>>,
    functions: HashMap<String, FunctionSig>,
    function_return_signatures: HashMap<String, FunctionSig>,
    classes: HashMap<String, ClassSig>,
    enum_variants: HashMap<String, EnumVariantSig>,
    traits: HashMap<String, TraitSig>,
    impl_traits: HashMap<String, HashSet<String>>,
    builtin_returns: HashMap<String, DataType>,
    return_type_stack: Vec<DataType>,
    impl_self_type: Option<DataType>,
    impl_self_name: Option<String>,
    statement_origins: Vec<String>,
    sources_by_filename: HashMap<String, String>,
    base_source: Option<String>,
    current_filename: Option<String>,
    current_line: usize,
    current_column: usize,
    current_top_level_index: Option<usize>,
    current_top_level_key: Option<String>,
    nested_statement_masks: HashMap<String, Vec<bool>>,
}

impl TypeChecker {
    fn new(source: &str) -> Self {
        Self {
            scopes: vec![HashMap::new()],
            struct_scopes: vec![HashMap::new()],
            ref_scopes: vec![HashMap::new()],
            function_alias_scopes: vec![HashMap::new()],
            function_value_sig_scopes: vec![HashMap::new()],
            functions: HashMap::new(),
            function_return_signatures: HashMap::new(),
            classes: HashMap::new(),
            enum_variants: HashMap::new(),
            traits: HashMap::new(),
            impl_traits: HashMap::new(),
            builtin_returns: crate::builtins::default_builtin_returns(),
            return_type_stack: Vec::new(),
            impl_self_type: None,
            impl_self_name: None,
            statement_origins: Vec::new(),
            sources_by_filename: HashMap::new(),
            base_source: (!source.is_empty()).then(|| source.to_string()),
            current_filename: None,
            current_line: 1,
            current_column: 1,
            current_top_level_index: None,
            current_top_level_key: None,
            nested_statement_masks: HashMap::new(),
        }
    }

    fn check_top_level_statements(&mut self, statements: &mut [Statement]) -> Result<()> {
        for (index, statement) in statements.iter_mut().enumerate() {
            self.current_filename = self.statement_origins.get(index).cloned();
            self.current_top_level_index = Some(index);
            self.current_top_level_key = Some(analysis_unit_key(statement));
            self.check_statement(statement)
                .map_err(|err| self.attach_current_context(err))?;
        }
        self.current_top_level_index = None;
        self.current_top_level_key = None;
        Ok(())
    }

    fn check_selected_top_level_statements(
        &mut self,
        statements: &mut [Statement],
        statement_mask: &[bool],
    ) -> Result<()> {
        if statement_mask.len() != statements.len() {
            return Err(type_error(format!(
                "Typecheck mask length mismatch: expected {}, got {}",
                statements.len(),
                statement_mask.len()
            )));
        }

        for (index, (statement, should_check)) in statements
            .iter_mut()
            .zip(statement_mask.iter().copied())
            .enumerate()
        {
            if !should_check {
                continue;
            }

            self.current_filename = self.statement_origins.get(index).cloned();
            self.current_top_level_index = Some(index);
            self.current_top_level_key = Some(analysis_unit_key(statement));
            self.check_statement(statement)
                .map_err(|err| self.attach_current_context(err))?;
        }
        self.current_top_level_index = None;
        self.current_top_level_key = None;
        Ok(())
    }

    fn current_nested_statement_mask(&self) -> Option<&[bool]> {
        self.current_top_level_key
            .as_ref()
            .and_then(|key| self.nested_statement_masks.get(key).map(Vec::as_slice))
    }

    fn check_selected_statements(
        &mut self,
        statements: &mut [Statement],
        statement_mask: &[bool],
    ) -> Result<()> {
        if statement_mask.len() != statements.len() {
            return Err(type_error(format!(
                "Nested typecheck mask length mismatch: expected {}, got {}",
                statements.len(),
                statement_mask.len()
            )));
        }

        for (statement, should_check) in statements.iter_mut().zip(statement_mask.iter().copied()) {
            if !should_check {
                continue;
            }
            self.check_statement(statement)?;
        }

        Ok(())
    }

    fn check_container_statements(&mut self, statements: &mut [Statement]) -> Result<()> {
        if let Some(mask) = self.current_nested_statement_mask() {
            let mask = mask.to_vec();
            self.check_selected_statements(statements, &mask)
        } else {
            self.check_statements(statements)
        }
    }

    fn attach_current_context(&self, err: MireError) -> MireError {
        let err = if err.line == 1 && err.column == 1 {
            err.with_position(self.current_line, self.current_column)
        } else {
            err
        };

        let err = if err.filename().is_none() {
            if let Some(filename) = &self.current_filename {
                err.with_filename(filename.clone())
            } else {
                err
            }
        } else {
            err
        };

        if err.source().is_none() {
            if let Some(filename) = err.filename()
                && let Some(source) = self.sources_by_filename.get(filename)
            {
                return err.with_source(source.clone());
            }
            if let Some(source) = &self.base_source {
                return err.with_source(source.clone());
            }
        }

        err
    }
    fn check_statements(&mut self, statements: &mut [Statement]) -> Result<()> {
        for statement in statements {
            self.check_statement(statement)?;
        }
        Ok(())
    }

    fn check_statement(&mut self, statement: &mut Statement) -> Result<()> {
        let (line, column) = location::statement_location(statement);
        self.current_line = line;
        self.current_column = column;
        match statement {
            Statement::Let {
                name,
                data_type,
                value,
                is_mutable,
                ..
            } => self.check_let_statement(name, data_type, value, *is_mutable)?,
            Statement::Assignment { target, value, .. } => {
                self.check_assignment_statement(target, value)?
            }
            Statement::Function {
                name,
                type_params,
                type_param_bounds,
                params,
                body,
                return_type,
                ..
            } => self.check_function_statement(
                name,
                type_params,
                type_param_bounds,
                params,
                body,
                return_type,
            )?,
            Statement::Return(expr) => self.check_return_statement(expr)?,
            Statement::If {
                condition,
                then_branch,
                else_branch,
            } => self.check_if_statement(condition, then_branch, else_branch)?,
            Statement::While { condition, body } => self.check_while_statement(condition, body)?,
            Statement::For {
                variable,
                index,
                iterable,
                body,
            } => self.check_for_statement(variable, index, iterable, body)?,
            Statement::Find {
                variable,
                iterable,
                body,
            } => self.check_find_statement(variable, iterable, body)?,
            Statement::Expression(expr) => {
                self.check_expression(expr)?;
            }
            Statement::Match {
                value,
                cases,
                default,
            } => self.check_match_statement(value, cases, default)?,
            Statement::Unsafe { body } => self.check_scoped_body(body)?,
            Statement::Asm { instructions } => self.check_asm_statement(instructions)?,
            Statement::Drop { value } => self.check_drop_statement(value)?,
            Statement::New {
                value,
                declared_type,
            } => self.check_new_statement(value, declared_type)?,
            Statement::Own { value, inner_type } => self.check_own_statement(value, inner_type)?,
            Statement::Move { target, value } => self.check_move_statement(target, value)?,
            Statement::Query {
                ops,
                bindings,
                group_by: _,
                joins: _,
                table: _,
            } => self.check_query_statement(ops, bindings)?,
            Statement::Impl {
                trait_name,
                type_name,
                methods,
                ..
            } => self.check_impl_statement(trait_name, type_name, methods)?,
            Statement::Type { fields, .. } => self.check_type_statement(fields)?,
            Statement::Skill { name, methods, .. } => self.check_skill_statement(name, methods)?,
            Statement::Break
            | Statement::Continue
            | Statement::ExternLib { .. }
            | Statement::ExternFunction { .. }
            | Statement::Enum { .. }
            | Statement::Module { .. } => {}
            Statement::Load { .. } => {}
        }

        Ok(())
    }
}

fn type_error(message: String) -> MireError {
    type_error_at(0, 0, message)
}

fn type_error_at(line: usize, column: usize, message: String) -> MireError {
    let (err_line, err_col) = if line == 0 { (1, 1) } else { (line, column) };
    MireError::type_error_at(err_line, err_col, message)
}

#[cfg(test)]
mod tests {
    use super::{
        check_program_types, check_program_types_partial_with_origins,
        check_program_types_with_origins,
    };
    use crate::compiler::AnalysisSelection;
    use crate::parse;
    use crate::parser::ast::{
        AssignmentTarget, DataType, Expression, Identifier, Literal, Program, Statement, Visibility,
    };
    use std::collections::HashMap;
    use std::path::PathBuf;

    #[test]
    fn infers_unknown_let_from_literal() {
        let mut program = Program {
            statements: vec![Statement::Let {
                name: "x".to_string(),
                data_type: DataType::Unknown,
                value: Some(Expression::Literal(Literal::Int(42))),
                is_constant: false,
                is_mutable: false,
                is_static: false,
                visibility: Visibility::Public,
                name_line: 1,
                name_column: 1,
            }],
        };

        check_program_types(&mut program, "").expect("type check must pass");

        match &program.statements[0] {
            Statement::Let { data_type, .. } => assert_eq!(*data_type, DataType::I64),
            _ => panic!("expected let"),
        }
    }

    #[test]
    fn resolves_identifier_type() {
        let mut program = Program {
            statements: vec![
                Statement::Let {
                    name: "x".to_string(),
                    data_type: DataType::I64,
                    value: Some(Expression::Literal(Literal::Int(1))),
                    is_constant: false,
                    is_mutable: false,
                    is_static: false,
                    visibility: Visibility::Public,
                    name_line: 1,
                    name_column: 1,
                },
                Statement::Expression(Expression::Identifier(Identifier {
                    name: "x".to_string(),
                    data_type: DataType::Unknown,
                    line: 0,
                    column: 0,
                })),
            ],
        };

        check_program_types(&mut program, "").expect("type check must pass");

        match &program.statements[1] {
            Statement::Expression(Expression::Identifier(ident)) => {
                assert_eq!(ident.data_type, DataType::I64)
            }
            _ => panic!("expected expression identifier"),
        }
    }

    #[test]
    fn infers_function_call_return_type() {
        let mut program = Program {
            statements: vec![
                Statement::Function {
                    name: "sum".to_string(),
                    type_params: Vec::new(),
                    type_param_bounds: Vec::new(),
                    params: vec![
                        ("a".to_string(), DataType::I64),
                        ("b".to_string(), DataType::I64),
                    ],
                    body: vec![Statement::Return(Some(Expression::BinaryOp {
                        operator: "+".to_string(),
                        left: Box::new(Expression::Identifier(Identifier {
                            name: "a".to_string(),
                            data_type: DataType::Unknown,
                            line: 0,
                            column: 0,
                        })),
                        right: Box::new(Expression::Identifier(Identifier {
                            name: "b".to_string(),
                            data_type: DataType::Unknown,
                            line: 0,
                            column: 0,
                        })),
                        data_type: DataType::Unknown,
                    }))],
                    return_type: DataType::Unknown,
                    visibility: Visibility::Public,
                    is_method: false,
                },
                Statement::Expression(Expression::Call {
                    name: "sum".to_string(),
                    args: vec![
                        Expression::Literal(Literal::Int(1)),
                        Expression::Literal(Literal::Int(2)),
                    ],
                    type_args: Vec::new(),
                    data_type: DataType::Unknown,
                }),
            ],
        };

        check_program_types(&mut program, "").expect("type check must pass");

        match &program.statements[1] {
            Statement::Expression(Expression::Call { data_type, .. }) => {
                assert_eq!(*data_type, DataType::I64)
            }
            _ => panic!("expected call expression"),
        }
    }

    #[test]
    fn fails_on_undefined_identifier() {
        let mut program = Program {
            statements: vec![Statement::Expression(Expression::Identifier(Identifier {
                name: "missing".to_string(),
                data_type: DataType::Unknown,
                line: 0,
                column: 0,
            }))],
        };

        let err = check_program_types(&mut program, "").expect_err("must fail");
        assert!(err.to_string().contains("Unknown identifier 'missing'"));
    }

    #[test]
    fn fails_on_assignment_type_mismatch() {
        let mut program = Program {
            statements: vec![
                Statement::Let {
                    name: "x".to_string(),
                    data_type: DataType::I64,
                    value: Some(Expression::Literal(Literal::Int(1))),
                    is_constant: false,
                    is_mutable: false,
                    is_static: false,
                    visibility: Visibility::Public,
                    name_line: 1,
                    name_column: 1,
                },
                Statement::Assignment {
                    target: AssignmentTarget::Variable("x".to_string()),
                    value: Expression::Literal(Literal::Str("bad".to_string())),
                    is_mutable: true,
                },
            ],
        };

        let err = check_program_types(&mut program, "").expect_err("must fail");
        assert!(
            err.to_string()
                .contains("Type mismatch in assignment to 'x'")
        );
    }

    #[test]
    fn accepts_builtin_calls() {
        let mut program = Program {
            statements: vec![
                Statement::Expression(Expression::Call {
                    name: "dasu".to_string(),
                    args: vec![Expression::Literal(Literal::Str("hello".to_string()))],
                    type_args: Vec::new(),
                    data_type: DataType::Unknown,
                }),
                Statement::Expression(Expression::Call {
                    name: "len".to_string(),
                    args: vec![Expression::Literal(Literal::List(vec![
                        Expression::Literal(Literal::Int(1)),
                        Expression::Literal(Literal::Int(2)),
                    ]))],
                    type_args: Vec::new(),
                    data_type: DataType::Unknown,
                }),
            ],
        };

        check_program_types(&mut program, "").expect("type check must pass");

        match &program.statements[0] {
            Statement::Expression(Expression::Call { data_type, .. }) => {
                assert_eq!(*data_type, DataType::None)
            }
            _ => panic!("expected call expression"),
        }
        match &program.statements[1] {
            Statement::Expression(Expression::Call { data_type, .. }) => {
                assert_eq!(*data_type, DataType::I64)
            }
            _ => panic!("expected call expression"),
        }
    }

    #[test]
    fn allows_unknown_in_logical_binary_ops() {
        let mut program = Program {
            statements: vec![
                Statement::Let {
                    name: "x".to_string(),
                    data_type: DataType::I64,
                    value: Some(Expression::Literal(Literal::Int(1))),
                    is_constant: false,
                    is_mutable: false,
                    is_static: false,
                    visibility: Visibility::Public,
                    name_line: 1,
                    name_column: 1,
                },
                Statement::Let {
                    name: "b".to_string(),
                    data_type: DataType::Unknown,
                    value: None,
                    is_constant: false,
                    is_mutable: false,
                    is_static: false,
                    visibility: Visibility::Public,
                    name_line: 1,
                    name_column: 1,
                },
                Statement::Expression(Expression::BinaryOp {
                    operator: "&&".to_string(),
                    left: Box::new(Expression::Identifier(Identifier {
                        name: "a".to_string(),
                        data_type: DataType::Unknown,
                        line: 0,
                        column: 0,
                    })),
                    right: Box::new(Expression::Identifier(Identifier {
                        name: "b".to_string(),
                        data_type: DataType::Unknown,
                        line: 0,
                        column: 0,
                    })),
                    data_type: DataType::Unknown,
                }),
            ],
        };

        check_program_types(&mut program, "").expect("type check must pass");
    }

    #[test]
    fn partial_typecheck_rechecks_only_selected_top_level_statements() {
        let mut previous = Program {
            statements: vec![
                Statement::Let {
                    name: "x".to_string(),
                    data_type: DataType::Unknown,
                    value: Some(Expression::Literal(Literal::Int(1))),
                    is_constant: false,
                    is_mutable: false,
                    is_static: false,
                    visibility: Visibility::Public,
                    name_line: 1,
                    name_column: 1,
                },
                Statement::Let {
                    name: "y".to_string(),
                    data_type: DataType::Unknown,
                    value: Some(Expression::Identifier(Identifier {
                        name: "x".to_string(),
                        data_type: DataType::Unknown,
                        line: 0,
                        column: 0,
                    })),
                    is_constant: false,
                    is_mutable: false,
                    is_static: false,
                    visibility: Visibility::Public,
                    name_line: 1,
                    name_column: 1,
                },
            ],
        };
        check_program_types(&mut previous, "").expect("baseline type check must pass");

        let mut current = Program {
            statements: vec![
                Statement::Let {
                    name: "x".to_string(),
                    data_type: DataType::Unknown,
                    value: Some(Expression::Literal(Literal::Int(2))),
                    is_constant: false,
                    is_mutable: false,
                    is_static: false,
                    visibility: Visibility::Public,
                    name_line: 1,
                    name_column: 1,
                },
                previous.statements[1].clone(),
            ],
        };

        let origins = vec![PathBuf::from("test.mire"), PathBuf::from("test.mire")];
        check_program_types_partial_with_origins(
            &mut current,
            "",
            &origins,
            &HashMap::new(),
            &AnalysisSelection {
                statement_mask: vec![true, false],
                ..AnalysisSelection::default()
            },
        )
        .expect("partial type check must pass");

        match &current.statements[0] {
            Statement::Let { data_type, .. } => assert_eq!(*data_type, DataType::I64),
            _ => panic!("expected let"),
        }

        match &current.statements[1] {
            Statement::Let {
                data_type,
                value: Some(Expression::Identifier(ident)),
                ..
            } => {
                assert_eq!(*data_type, DataType::I64);
                assert_eq!(ident.data_type, DataType::I64);
            }
            _ => panic!("expected reused typed let"),
        }
    }

    #[test]
    fn partial_typecheck_can_skip_unchanged_impl_methods() {
        let mut program = Program {
            statements: vec![Statement::Impl {
                trait_name: None,
                type_name: "Point".to_string(),
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
                methods: vec![
                    Statement::Function {
                        name: "good".to_string(),
                        type_params: Vec::new(),
                        type_param_bounds: Vec::new(),
                        params: vec![],
                        body: vec![Statement::Return(Some(Expression::Literal(Literal::Int(
                            1,
                        ))))],
                        return_type: DataType::I64,
                        visibility: Visibility::Public,
                        is_method: true,
                    },
                    Statement::Function {
                        name: "bad".to_string(),
                        type_params: Vec::new(),
                        type_param_bounds: Vec::new(),
                        params: vec![],
                        body: vec![Statement::Return(Some(Expression::Identifier(
                            Identifier {
                                name: "missing".to_string(),
                                data_type: DataType::Unknown,
                                line: 0,
                                column: 0,
                            },
                        )))],
                        return_type: DataType::I64,
                        visibility: Visibility::Public,
                        is_method: true,
                    },
                ],
            }],
        };

        check_program_types_partial_with_origins(
            &mut program,
            "",
            &[PathBuf::from("test.mire")],
            &HashMap::new(),
            &AnalysisSelection {
                statement_mask: vec![true],
                nested_statement_masks: HashMap::from([(
                    "impl::Point".to_string(),
                    vec![true, false],
                )]),
            },
        )
        .expect("partial type check should skip unchanged impl method");
    }

    #[test]
    fn partial_typecheck_can_skip_nested_members_in_type_and_impl_members() {
        let mut program = Program {
            statements: vec![
                Statement::Type {
                    visibility: Visibility::Public,
                    name: "PointType".to_string(),
                    type_params: Vec::new(),
                    type_param_bounds: Vec::new(),
                    parent: None,
                    fields: vec![
                        Statement::Let {
                            name: "x".to_string(),
                            data_type: DataType::Unknown,
                            value: Some(Expression::Literal(Literal::Int(1))),
                            is_constant: false,
                            is_mutable: false,
                            is_static: false,
                            visibility: Visibility::Public,
                            name_line: 1,
                            name_column: 1,
                        },
                        Statement::Let {
                            name: "broken".to_string(),
                            data_type: DataType::Unknown,
                            value: Some(Expression::Identifier(Identifier {
                                name: "missing".to_string(),
                                data_type: DataType::Unknown,
                                line: 0,
                                column: 0,
                            })),
                            is_constant: false,
                            is_mutable: false,
                            is_static: false,
                            visibility: Visibility::Public,
                            name_line: 1,
                            name_column: 1,
                        },
                    ],
                },
                Statement::Impl {
                    trait_name: None,
                    type_name: "PointType".to_string(),
                    type_params: Vec::new(),
                    type_param_bounds: Vec::new(),
                    methods: vec![
                        Statement::Function {
                            name: "good".to_string(),
                            type_params: Vec::new(),
                            type_param_bounds: Vec::new(),
                            params: vec![],
                            body: vec![Statement::Return(Some(Expression::Literal(Literal::Int(
                                1,
                            ))))],
                            return_type: DataType::I64,
                            visibility: Visibility::Public,
                            is_method: true,
                        },
                        Statement::Function {
                            name: "bad".to_string(),
                            type_params: Vec::new(),
                            type_param_bounds: Vec::new(),
                            params: vec![],
                            body: vec![Statement::Return(Some(Expression::Identifier(
                                Identifier {
                                    name: "missing".to_string(),
                                    data_type: DataType::Unknown,
                                    line: 0,
                                    column: 0,
                                },
                            )))],
                            return_type: DataType::I64,
                            visibility: Visibility::Public,
                            is_method: true,
                        },
                    ],
                },
            ],
        };

        check_program_types_partial_with_origins(
            &mut program,
            "",
            &[PathBuf::from("test.mire"), PathBuf::from("test.mire")],
            &HashMap::new(),
            &AnalysisSelection {
                statement_mask: vec![true, true],
                nested_statement_masks: HashMap::from([
                    ("PointType".to_string(), vec![true, false]),
                    ("impl::PointType".to_string(), vec![true, false]),
                ]),
            },
        )
        .expect("partial type check should skip unchanged nested members");

        let Statement::Type { fields, .. } = &program.statements[0] else {
            panic!("expected type");
        };
        let Statement::Let { data_type, .. } = &fields[0] else {
            panic!("expected typed field");
        };
        assert_eq!(*data_type, DataType::I64);
    }

    #[test]
    fn dereference_of_reference_binding_recovers_pointed_type() {
        let source = "pub fn main: () {\n    set x = 1 :i64\n    set r = &x\n    set y = *r\n}\n";
        let mut program = parse(source).expect("source should parse");

        check_program_types(&mut program, source).expect("type check must pass");

        let Statement::Function { body, .. } = &program.statements[0] else {
            panic!("expected function");
        };
        let Statement::Let {
            data_type,
            value:
                Some(Expression::Dereference {
                    data_type: deref_type,
                    ..
                }),
            ..
        } = &body[2]
        else {
            panic!("expected dereference binding");
        };
        assert_eq!(*deref_type, DataType::I64);
        assert_eq!(*data_type, DataType::I64);
    }

    #[test]
    fn pipeline_closure_infers_vector_of_return_type() {
        let source = "pub fn main: () {\n    set nums = [1 2 3] :vec[i64]\n    set doubled = nums => (x => x * 2)\n}\n";
        let mut program = parse(source).expect("source should parse");

        check_program_types(&mut program, source).expect("pipeline should type check");

        let Statement::Function { body, .. } = &program.statements[0] else {
            panic!("expected function");
        };
        let Statement::Let { data_type, .. } = &body[1] else {
            panic!("expected pipeline let");
        };
        assert_eq!(
            *data_type,
            DataType::Vector {
                element_type: Box::new(DataType::I64),
                dynamic: true,
            }
        );
    }

    #[test]
    fn integer_literal_range_validation_does_not_scan_unrelated_scope_bindings() {
        let source = "pub fn main: () {\n    set tiny = 1 :i8\n    set big = 300 :i64\n}\n";
        let mut program = parse(source).expect("source should parse");

        check_program_types(&mut program, source)
            .expect("unrelated i8 binding must not reject i64 literal");
    }

    #[test]
    fn map_assignment_rejects_vector_values() {
        let mut program = Program {
            statements: vec![
                Statement::Let {
                    name: "values".to_string(),
                    data_type: DataType::Vector {
                        element_type: Box::new(DataType::I64),
                        dynamic: true,
                    },
                    value: Some(Expression::List {
                        elements: vec![
                            Expression::Literal(Literal::Int(1)),
                            Expression::Literal(Literal::Int(2)),
                            Expression::Literal(Literal::Int(3)),
                        ],
                        element_type: DataType::I64,
                        data_type: DataType::Vector {
                            element_type: Box::new(DataType::I64),
                            dynamic: true,
                        },
                    }),
                    is_constant: false,
                    is_mutable: false,
                    is_static: false,
                    visibility: Visibility::Public,
                    name_line: 1,
                    name_column: 1,
                },
                Statement::Let {
                    name: "m".to_string(),
                    data_type: DataType::Map {
                        key_type: Box::new(DataType::Str),
                        value_type: Box::new(DataType::I64),
                    },
                    value: Some(Expression::Identifier(Identifier {
                        name: "values".to_string(),
                        data_type: DataType::Unknown,
                        line: 0,
                        column: 0,
                    })),
                    is_constant: false,
                    is_mutable: false,
                    is_static: false,
                    visibility: Visibility::Public,
                    name_line: 1,
                    name_column: 1,
                },
            ],
        };

        let err = check_program_types(&mut program, "").expect_err("must reject vec -> map");
        assert!(err.to_string().contains("Type mismatch in let 'm'"));
    }

    #[test]
    fn typed_struct_parameters_can_dispatch_instance_methods() {
        let source = "struct Counter {\n    value :i64\n}\n\nimpl Counter {\n    fn get: (self) :i64 {\n        return self.value\n    }\n}\n\nfn read_counter: (counter :Counter) :i64 {\n    return counter.get()\n}\n";
        let mut program = parse(source).expect("source should parse");

        check_program_types(&mut program, source)
            .expect("typed struct parameter should preserve concrete method dispatch");
    }

    #[test]
    fn unify_types_is_order_independent_for_reference_and_value_pairs() {
        assert_eq!(
            super::TypeChecker::unify_types(
                &DataType::Ref {
                    inner: Box::new(DataType::I64),
                },
                &DataType::I64,
            )
            .expect("ref + value should unify"),
            DataType::I64
        );
        assert_eq!(
            super::TypeChecker::unify_types(
                &DataType::I64,
                &DataType::Ref {
                    inner: Box::new(DataType::I64),
                },
            )
            .expect("value + ref should unify"),
            DataType::I64
        );
    }

    #[test]
    fn mutable_reference_expectation_rejects_shared_reference_argument() {
        let mut program = Program {
            statements: vec![
                Statement::Function {
                    name: "bump".to_string(),
                    type_params: Vec::new(),
                    type_param_bounds: Vec::new(),
                    params: vec![(
                        "value".to_string(),
                        DataType::RefMut {
                            inner: Box::new(DataType::I64),
                        },
                    )],
                    body: vec![],
                    return_type: DataType::None,
                    visibility: Visibility::Public,
                    is_method: false,
                },
                Statement::Function {
                    name: "main".to_string(),
                    type_params: Vec::new(),
                    type_param_bounds: Vec::new(),
                    params: vec![],
                    body: vec![
                        Statement::Let {
                            name: "x".to_string(),
                            data_type: DataType::I64,
                            value: Some(Expression::Literal(Literal::Int(1))),
                            is_constant: false,
                            is_mutable: false,
                            is_static: false,
                            visibility: Visibility::Public,
                            name_line: 1,
                            name_column: 1,
                        },
                        Statement::Let {
                            name: "shared".to_string(),
                            data_type: DataType::Unknown,
                            value: Some(Expression::Reference {
                                expr: Box::new(Expression::Identifier(Identifier {
                                    name: "x".to_string(),
                                    data_type: DataType::Unknown,
                                    line: 0,
                                    column: 0,
                                })),
                                is_mutable: false,
                                data_type: DataType::Unknown,
                                referenced_type: DataType::Unknown,
                            }),
                            is_constant: false,
                            is_mutable: false,
                            is_static: false,
                            visibility: Visibility::Public,
                            name_line: 1,
                            name_column: 1,
                        },
                        Statement::Expression(Expression::Call {
                            name: "bump".to_string(),
                            args: vec![Expression::Identifier(Identifier {
                                name: "shared".to_string(),
                                data_type: DataType::Unknown,
                                line: 0,
                                column: 0,
                            })],
                            type_args: Vec::new(),
                            data_type: DataType::Unknown,
                        }),
                    ],
                    return_type: DataType::None,
                    visibility: Visibility::Public,
                    is_method: false,
                },
            ],
        };

        let err = check_program_types(&mut program, "")
            .expect_err("shared ref should not satisfy &mut parameter");
        assert!(
            err.to_string()
                .contains("Function 'bump' argument 1 expects")
        );
        assert!(err.to_string().contains("RefMut"));
    }

    #[test]
    fn mutable_binding_reference_is_inferred_as_refmut() {
        let source = "pub fn main: () {\n    set x = 1 :i64 mut\n    set r = &x\n}\n";
        let mut program = parse(source).expect("source should parse");

        check_program_types(&mut program, source).expect("type check should pass");

        let Statement::Function { body, .. } = &program.statements[0] else {
            panic!("expected function");
        };
        let Statement::Let { data_type, .. } = &body[1] else {
            panic!("expected second let");
        };
        assert!(matches!(data_type, DataType::RefMut { .. }));
    }

    #[test]
    fn immutable_binding_reference_is_inferred_as_shared_ref() {
        let source = "pub fn main: () {\n    set x = 1 :i64\n    set r = &x\n}\n";
        let mut program = parse(source).expect("source should parse");

        check_program_types(&mut program, source).expect("type check should pass");

        let Statement::Function { body, .. } = &program.statements[0] else {
            panic!("expected function");
        };
        let Statement::Let { data_type, .. } = &body[1] else {
            panic!("expected second let");
        };
        assert!(matches!(data_type, DataType::Ref { .. }));
    }

    #[test]
    fn explicit_mut_reference_rejected_for_immutable_binding() {
        let source = "pub fn main: () {\n    set x = 1 :i64\n    set r = &mut x\n}\n";
        let mut program = parse(source).expect("source should parse");
        let err = check_program_types(&mut program, source)
            .expect_err("immutable binding cannot produce mutable reference");
        assert!(
            err.to_string()
                .contains("Cannot take mutable reference from immutable target")
        );
    }

    #[test]
    fn type_checker_source_context_does_not_leak_between_runs() {
        let source_a = "pub fn main: () {\n    use dasu(missing_a)\n}\n";
        let mut program_a = parse(source_a).expect("source A should parse");
        let err_a = check_program_types(&mut program_a, source_a).expect_err("A must fail");
        assert_eq!(err_a.source(), Some(&source_a.to_string()));

        let source_b = "pub fn main: () {\n    use dasu(missing_b)\n}\n";
        let mut program_b = parse(source_b).expect("source B should parse");
        let err_b = check_program_types(&mut program_b, source_b).expect_err("B must fail");
        assert_eq!(err_b.source(), Some(&source_b.to_string()));
        assert_ne!(err_a.source(), err_b.source());
    }

    #[test]
    fn type_checker_uses_file_source_from_origins_without_global_state() {
        let source = "pub fn main: () {\n    use dasu(missing_file)\n}\n";
        let mut program = parse(source).expect("source should parse");
        let file = PathBuf::from("prototype_typeck_context.mire");
        let origins = vec![file.clone()];
        let mut sources = HashMap::new();
        sources.insert(file.clone(), source.to_string());

        let err = check_program_types_with_origins(&mut program, "", &origins, &sources)
            .expect_err("must fail and attach origin source");
        assert_eq!(
            err.filename().map(String::as_str),
            Some("prototype_typeck_context.mire")
        );
        assert_eq!(err.source(), Some(&source.to_string()));
    }
}
