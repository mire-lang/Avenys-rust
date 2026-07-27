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

use crate::load_project_manifest;

use self::typeck_returns::{implicit_return_expression_mut, statements_contain_explicit_return};
use crate::compiler::{AnalysisSelection, location};
use crate::error::{MireError, Result, type_error_at_span};
use crate::incremental::analysis_unit_key;
use crate::parser::ast::{
    AssignmentTarget, DataType, Expression, Identifier, Literal, Program, Statement, TraitMethodSig,
};

#[cfg(test)]
#[path = "typeck_tests.rs"]
mod tests;
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
    checker.collect_load_local_modules(&program.statements);
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
    checker.collect_load_local_modules(&program.statements);
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
    current_span: crate::error::Span,
    current_top_level_index: Option<usize>,
    current_top_level_key: Option<String>,
    nested_statement_masks: HashMap<String, Vec<bool>>,
    load_local_modules: HashSet<String>,
    in_use_macro: bool,
    allowed_builtins: Option<HashSet<String>>,
    constant_scopes: Vec<HashSet<String>>,
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
            current_span: crate::error::Span::new(1, 1),
            current_top_level_index: None,
            current_top_level_key: None,
            nested_statement_masks: HashMap::new(),
            load_local_modules: HashSet::new(),
            in_use_macro: false,
            allowed_builtins: Self::load_allowed_builtins(),
            constant_scopes: vec![HashSet::new()],
        }
    }

    /// Loads the `[builtins]` allowlist from the project's owl.toml.
    /// When `enabled = true` and `allow` is set, only those builtins are
    /// permitted; everything else is rejected at call sites. When the section
    /// is absent the behavior is unchanged (all builtins allowed).
    fn load_allowed_builtins() -> Option<HashSet<String>> {
        let cwd = std::env::current_dir().ok()?;
        let manifest = load_project_manifest(&cwd).ok()??;
        let builtins = manifest.builtins?;
        if !builtins.enabled {
            return None;
        }
        if builtins.allow.is_empty() {
            return None;
        }
        Some(builtins.allow.into_iter().collect())
    }

    fn collect_load_local_modules(&mut self, statements: &[Statement]) {
        for statement in statements {
            if let Statement::LoadLocal { rel_path, .. } = statement
                && let Some(prefix) = rel_path.last()
            {
                self.load_local_modules.insert(prefix.clone());
            }
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
            return Err(type_error_at_span(
                self.current_span,
                format!(
                    "Typecheck mask length mismatch: expected {}, got {}",
                    statements.len(),
                    statement_mask.len()
                ),
            ));
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
            return Err(type_error_at_span(
                self.current_span,
                format!(
                    "Nested typecheck mask length mismatch: expected {}, got {}",
                    statements.len(),
                    statement_mask.len()
                ),
            ));
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
        let err = if err.span.is_unknown() {
            err.with_span(self.current_span)
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
        let span = location::statement_location(statement);
        self.current_span = span;
        let result = match statement {
            Statement::Let {
                name,
                data_type,
                value,
                is_mutable,
                is_constant,
                ..
            } => self.check_let_statement(name, data_type, value, *is_mutable, *is_constant),
            Statement::Assignment { target, value, .. } => {
                self.check_assignment_statement(target, value)
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
            ),
            Statement::Return(expr) => self.check_return_statement(expr),
            Statement::If {
                condition,
                then_branch,
                else_branch,
            } => self.check_if_statement(condition, then_branch, else_branch),
            Statement::While { condition, body } => self.check_while_statement(condition, body),
            Statement::For {
                variable,
                index,
                iterable,
                body,
            } => self.check_for_statement(variable, index, iterable, body),
            Statement::Find {
                variable,
                iterable,
                body,
            } => self.check_find_statement(variable, iterable, body),
            Statement::Expression(expr) => self.check_expression(expr).map(|_| ()),
            Statement::Match {
                value,
                cases,
                default,
            } => self.check_match_statement(value, cases, default),
            Statement::Unsafe { body, .. } => self.check_scoped_body(body),
            Statement::Asm { instructions } => self.check_asm_statement(instructions),
            Statement::Drop { value } => self.check_drop_statement(value),
            Statement::New {
                value,
                declared_type,
            } => self.check_new_statement(value, declared_type),
            Statement::Own { value, inner_type } => self.check_own_statement(value, inner_type),
            Statement::Move { target, value } => self.check_move_statement(target, value),
            Statement::Query {
                ops,
                bindings,
                group_by: _,
                joins: _,
                table: _,
            } => self.check_query_statement(ops, bindings),
            Statement::Impl {
                trait_name,
                type_name,
                methods,
                ..
            } => self.check_impl_statement(trait_name, type_name, methods),
            Statement::Type { fields, .. } => self.check_type_statement(fields),
            Statement::Skill { name, methods, .. } => self.check_skill_statement(name, methods),
            Statement::Break
            | Statement::Continue
            | Statement::ExternLib { .. }
            | Statement::ExternFunction { .. }
            | Statement::Enum { .. }
            | Statement::Module { .. } => Ok(()),
            Statement::Load { .. } => Ok(()),
            Statement::LoadLocal { .. } => Ok(()),
        };
        result.map_err(|err| self.attach_current_context(err))
    }
}

fn type_error(line: usize, column: usize, message: String) -> MireError {
    crate::error::type_error(line, column, message)
}
