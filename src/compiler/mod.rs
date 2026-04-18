pub mod borrowck;
pub mod semantic;
pub mod typeck;

use crate::error::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use crate::parser::Program;

pub use semantic::{
    BindingInfo, BindingKind, BorrowFact, BorrowKind, MoveFact, ScopeInfo, SemanticModel,
};
pub use typeck::check_program_types;

pub fn analyze_program(program: &mut Program, source: &str) -> Result<SemanticModel> {
    typeck::check_program_types(program, source)?;
    let semantic_model = semantic::analyze_program(program);
    borrowck::check_program(program, &semantic_model)?;
    Ok(semantic_model)
}

pub fn analyze_program_with_origins(
    program: &mut Program,
    source: &str,
    statement_origins: &[PathBuf],
    sources: &HashMap<PathBuf, String>,
) -> Result<SemanticModel> {
    typeck::check_program_types_with_origins(program, source, statement_origins, sources)?;
    let semantic_model = semantic::analyze_program(program);
    borrowck::check_program_with_origins(program, &semantic_model, statement_origins, sources)?;
    Ok(semantic_model)
}