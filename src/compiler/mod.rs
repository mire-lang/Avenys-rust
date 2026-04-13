pub mod borrowck;
pub mod semantic;
pub mod typeck;

use crate::error::Result;
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