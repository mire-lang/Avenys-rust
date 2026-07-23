use crate::error::diagnostic::DiagnosticCode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MssError {
    MutationWhileShared,
    MultipleMutableRefs,
    MoveWhileBorrowed,
    UseAfterMove,
    DropWhileBorrowed,
    BorrowOutOfScope,
    InvalidMove,
    UnsafeViolation,
}

impl MssError {
    pub fn diagnostic_code(&self) -> DiagnosticCode {
        match self {
            MssError::UseAfterMove => DiagnosticCode::E0007,
            MssError::MultipleMutableRefs => DiagnosticCode::E0008,
            MssError::MutationWhileShared => DiagnosticCode::E0009,
            MssError::MoveWhileBorrowed | MssError::InvalidMove => DiagnosticCode::E0010,
            MssError::DropWhileBorrowed => DiagnosticCode::E0011,
            MssError::BorrowOutOfScope | MssError::UnsafeViolation => DiagnosticCode::E0013,
        }
    }
}

impl std::fmt::Display for MssError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MssError::MutationWhileShared => {
                write!(f, "MSS Error: Cannot mutate - value has shared references")
            }
            MssError::MultipleMutableRefs => {
                write!(f, "MSS Error: Multiple mutable references not allowed")
            }
            MssError::MoveWhileBorrowed => write!(f, "MSS Error: Cannot move - value is borrowed"),
            MssError::UseAfterMove => write!(f, "MSS Error: Use after move - value was moved"),
            MssError::DropWhileBorrowed => {
                write!(f, "MSS Error: Cannot drop - value has active references")
            }
            MssError::BorrowOutOfScope => write!(f, "MSS Error: Borrow outlives owner scope"),
            MssError::InvalidMove => write!(f, "MSS Error: Invalid move operation"),
            MssError::UnsafeViolation => write!(f, "MSS Error: Unsafe block violation"),
        }
    }
}
