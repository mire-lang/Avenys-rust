#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MssError {
    MutationWhileShared,
    MultipleMutableRefs,
    MoveWhileBorrowed,
    UseAfterMove,
    DropWhileBorrowed,
    DoubleDrop,
    BorrowOutOfScope,
    InvalidMove,
    UnsafeViolation,
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
            MssError::DoubleDrop => write!(f, "MSS Error: Double drop detected"),
            MssError::BorrowOutOfScope => write!(f, "MSS Error: Borrow outlives owner scope"),
            MssError::InvalidMove => write!(f, "MSS Error: Invalid move operation"),
            MssError::UnsafeViolation => write!(f, "MSS Error: Unsafe block violation"),
        }
    }
}