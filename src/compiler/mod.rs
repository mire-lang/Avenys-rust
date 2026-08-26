pub mod borrowck;
pub mod location;
pub mod mir;
pub mod semantic;
pub mod typeck;
pub mod warnings;
mod warning_diagnostics;

use crate::error::Result;
use crate::error::diagnostic::{Diagnostic, DiagnosticCode, WarningFilter};
use crate::parser::Program;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub use semantic::{
    BindingInfo, BindingKind, BorrowFact, BorrowKind, MoveFact, ScopeInfo, SemanticModel,
};
pub use typeck::check_program_types;
pub use warnings::check_warnings;
pub use warnings::check_warnings_with_origins;

#[derive(Debug, Clone, Default)]
pub struct AnalysisSelection {
    pub statement_mask: Vec<bool>,
    pub nested_statement_masks: HashMap<String, Vec<bool>>,
}

#[derive(Debug, Clone)]
pub struct WarningConfig {
    pub filter: WarningFilter,
    pub deny: HashSet<DiagnosticCode>,
}

#[derive(Debug, Clone)]
pub struct AnalysisReport {
    pub semantic: SemanticModel,
    pub diagnostics: Vec<Diagnostic>,
}

impl AnalysisSelection {
    pub fn full(program: &Program) -> Self {
        Self {
            statement_mask: vec![true; program.statements.len()],
            nested_statement_masks: HashMap::new(),
        }
    }
}

pub fn analyze_program(program: &mut Program, source: &str) -> Result<SemanticModel> {
    typeck::check_program_types(program, source)?;
    let semantic_model = semantic::analyze_program(program);
    borrowck::check_program(program, &semantic_model)?;
    Ok(semantic_model)
}

pub fn analyze_program_with_warnings(
    program: &mut Program,
    source: &str,
    filename: Option<&str>,
    warning_config: WarningConfig,
) -> Result<AnalysisReport> {
    let semantic_model = semantic::analyze_program(program);
    let warnings = check_warnings(
        program,
        source,
        filename,
        warning_config.filter,
        warning_config.deny,
    );
    Ok(AnalysisReport {
        semantic: semantic_model,
        diagnostics: warnings,
    })
}

pub fn analyze_program_with_warnings_and_origins(
    program: &mut Program,
    source: &str,
    filename: Option<&str>,
    warning_config: WarningConfig,
    statement_origins: &[PathBuf],
    entry_path: &Path,
) -> Result<AnalysisReport> {
    let semantic_model = semantic::analyze_program(program);
    let warnings = check_warnings_with_origins(
        program,
        source,
        filename,
        warning_config.filter,
        warning_config.deny,
        statement_origins,
        entry_path,
    );
    Ok(AnalysisReport {
        semantic: semantic_model,
        diagnostics: warnings,
    })
}

pub fn analyze_program_with_origins(
    program: &mut Program,
    source: &str,
    statement_origins: &[PathBuf],
    sources: &HashMap<PathBuf, String>,
) -> Result<SemanticModel> {
    analyze_program_with_origins_partial(
        program,
        source,
        statement_origins,
        sources,
        &AnalysisSelection::full(program),
    )
}

pub fn analyze_program_with_origins_partial(
    program: &mut Program,
    source: &str,
    statement_origins: &[PathBuf],
    sources: &HashMap<PathBuf, String>,
    selection: &AnalysisSelection,
) -> Result<SemanticModel> {
    typeck::check_program_types_partial_with_origins(
        program,
        source,
        statement_origins,
        sources,
        selection,
    )?;
    let semantic_model = semantic::analyze_program(program);
    borrowck::check_program_partial_with_origins(
        program,
        &semantic_model,
        statement_origins,
        sources,
        selection,
    )?;
    Ok(semantic_model)
}
