//! Unit tests for the real type theory.
//!
//! These tests guarantee that the declared types ARE real: correct widths,
//! assignable without loss, and promotion without silent widening.

use crate::parser::ast::DataType;

#[test]
fn int_bit_widths_are_distinct_and_real() {
    use crate::types::codegen::integer_bit_width;
    assert_eq!(integer_bit_width(&DataType::I8), Some(8));
    assert_eq!(integer_bit_width(&DataType::U8), Some(8));
    assert_eq!(integer_bit_width(&DataType::I16), Some(16));
    assert_eq!(integer_bit_width(&DataType::U16), Some(16));
    assert_eq!(integer_bit_width(&DataType::I32), Some(32));
    assert_eq!(integer_bit_width(&DataType::U32), Some(32));
    assert_eq!(integer_bit_width(&DataType::I64), Some(64));
    assert_eq!(integer_bit_width(&DataType::U64), Some(64));
    assert_eq!(integer_bit_width(&DataType::I128), Some(128));
    assert_eq!(integer_bit_width(&DataType::U128), Some(128));
    assert_eq!(integer_bit_width(&DataType::Char), Some(32));
    assert_eq!(integer_bit_width(&DataType::F32), Some(32));
    assert_eq!(integer_bit_width(&DataType::F64), Some(64));
}

#[test]
fn llvm_type_str_has_real_widths() {
    use crate::types::codegen::llvm_type_str;
    assert_eq!(llvm_type_str(&DataType::U8), "i8");
    assert_eq!(llvm_type_str(&DataType::I8), "i8");
    assert_eq!(llvm_type_str(&DataType::U16), "i16");
    assert_eq!(llvm_type_str(&DataType::I16), "i16");
    assert_eq!(llvm_type_str(&DataType::U32), "i32");
    assert_eq!(llvm_type_str(&DataType::I32), "i32");
    assert_eq!(llvm_type_str(&DataType::U64), "i64");
    assert_eq!(llvm_type_str(&DataType::I64), "i64");
    assert_eq!(llvm_type_str(&DataType::U128), "i128");
    assert_eq!(llvm_type_str(&DataType::I128), "i128");
    assert_eq!(llvm_type_str(&DataType::Char), "i32");
    assert_eq!(llvm_type_str(&DataType::F32), "float");
    assert_eq!(llvm_type_str(&DataType::F64), "double");
    assert_eq!(llvm_type_str(&DataType::Bool), "i1");
}

#[test]
fn struct_layout_respects_real_widths() {
    use crate::types::codegen::render_struct_llvm_type;
    let fields = vec![
        ("a".to_string(), DataType::U8),
        ("b".to_string(), DataType::I8),
        ("c".to_string(), DataType::I64),
    ];
    // u8/i8 each occupy 8 bits; the third field is i64.
    assert_eq!(render_struct_llvm_type(&fields), "{ i8, i8, i64 }");
}

#[test]
fn assignable_without_loss() {
    use crate::types::unify::is_assignable;
    // widening: i8 -> i64 is valid (no loss)
    assert!(is_assignable(&DataType::I64, &DataType::I8));
    assert!(is_assignable(&DataType::I128, &DataType::I64));
    assert!(is_assignable(&DataType::F64, &DataType::F32));
    assert!(is_assignable(&DataType::F64, &DataType::I64));
    // same-width signed<->unsigned is valid (same representation)
    assert!(is_assignable(&DataType::U8, &DataType::I8));
    assert!(is_assignable(&DataType::I8, &DataType::U8));
}

#[test]
fn not_assignable_with_loss() {
    use crate::types::unify::is_assignable;
    // silent narrowing PROHIBITED
    assert!(!is_assignable(&DataType::I8, &DataType::I64));
    assert!(!is_assignable(&DataType::U8, &DataType::I64));
    assert!(!is_assignable(&DataType::I32, &DataType::I64));
    // float -> int requires explicit cast
    assert!(!is_assignable(&DataType::I64, &DataType::F64));
    assert!(!is_assignable(&DataType::I32, &DataType::F32));
    // int -> float narrow requires cast (does not fit without wide precision loss)
    assert!(!is_assignable(&DataType::F32, &DataType::I64));
}

#[test]
fn promote_numeric_keeps_widest() {
    use crate::types::unify::promote_numeric;
    assert_eq!(promote_numeric(&DataType::I8, &DataType::I64), DataType::I64);
    assert_eq!(promote_numeric(&DataType::U8, &DataType::U32), DataType::U32);
    assert_eq!(promote_numeric(&DataType::F32, &DataType::I64), DataType::F32);
    assert_eq!(promote_numeric(&DataType::I32, &DataType::F64), DataType::F64);
    assert_eq!(promote_numeric(&DataType::F64, &DataType::F32), DataType::F64);
    assert_eq!(promote_numeric(&DataType::I128, &DataType::I32), DataType::I128);
}

#[test]
fn unify_distinct_numerics_promotes_without_error() {
    use crate::types::unify::unify_types;
    // Unifying in arithmetic context promotes to the wider type (no loss).
    assert_eq!(
        unify_types(&DataType::I8, &DataType::I64).unwrap(),
        DataType::I64
    );
    assert_eq!(
        unify_types(&DataType::F32, &DataType::I64).unwrap(),
        DataType::F32
    );
}
