use crate::parser::ast::{DataType, Expression, Literal};

pub(crate) fn extract_data_type(expr: &Expression) -> DataType {
    match expr {
        Expression::Literal(lit) => match lit {
            Literal::Int(_) => DataType::I64,
            Literal::Float(_) => DataType::F64,
            Literal::Bool(_) => DataType::Bool,
            Literal::Str(_) => DataType::Str,
            Literal::Char(_) => DataType::Char,
            Literal::None => DataType::None,
            Literal::List(_) => DataType::List,
            Literal::Dict(..) => DataType::Dict,
            Literal::Tuple(_) => DataType::Tuple,
        },
        Expression::Identifier(id) => id.data_type.clone(),
        Expression::BinaryOp { data_type, .. } => data_type.clone(),
        Expression::UnaryOp { data_type, .. } => data_type.clone(),
        Expression::NamedArg { data_type, .. } => data_type.clone(),
        Expression::Call { data_type, .. } => data_type.clone(),
        Expression::List { data_type, .. } => data_type.clone(),
        Expression::Dict { data_type, .. } => data_type.clone(),
        Expression::Tuple { data_type, .. } => data_type.clone(),
        Expression::Index { data_type, .. } => data_type.clone(),
        Expression::MemberAccess { data_type, .. } => data_type.clone(),
        Expression::Closure { return_type, .. } => return_type.clone(),
        Expression::Reference { data_type, .. } => data_type.clone(),
        Expression::Dereference { data_type, .. } => data_type.clone(),
        Expression::Box { data_type, .. } => data_type.clone(),
        Expression::Pipeline { data_type, .. } => data_type.clone(),
        Expression::Try { data_type, .. } => data_type.clone(),
        Expression::Ok { data_type, .. } => data_type.clone(),
        Expression::Err { data_type, .. } => data_type.clone(),
        Expression::Match { data_type, .. } => data_type.clone(),
        Expression::EnumVariantPath { data_type, .. } => data_type.clone(),
        Expression::EnumVariant { data_type, .. } => data_type.clone(),
    }
}

pub(crate) fn is_map_or_dict_type(dt: &DataType) -> bool {
    matches!(dt, DataType::Map { .. } | DataType::Dict)
}

pub(crate) fn is_trivial_deref(source: &DataType, target: &DataType) -> bool {
    if !matches!(source, DataType::Ref { .. } | DataType::RefMut { .. }) {
        return false;
    }
    matches!(target,
        DataType::Str
        | DataType::List
        | DataType::Vector { .. }
        | DataType::Dict
        | DataType::Map { .. }
        | DataType::Box
        | DataType::Struct
        | DataType::StructNamed(_)
        | DataType::Function
        | DataType::Tuple
        | DataType::Set
        | DataType::Datetime
        | DataType::Slice { .. }
        | DataType::DynTrait { .. }
        | DataType::Result { .. }
    )
}

pub(crate) fn data_type_to_kind(dt: &DataType) -> i64 {
    match dt {
        DataType::Bool => 2,
        DataType::Str => 3,
        DataType::Map { .. } | DataType::Dict => 4,
        DataType::Vector { .. }
        | DataType::List
        | DataType::Array { .. }
        | DataType::Struct
        | DataType::StructNamed(_)
        | DataType::Ref { .. }
        | DataType::RefMut { .. }
        | DataType::Box => 5,
        _ => 1,
    }
}

pub(crate) fn llvm_elem_type_str(dt: &DataType) -> String {
    match dt {
        DataType::I64 | DataType::Char | DataType::U64 => "i64".to_string(),
        DataType::I32 | DataType::U32 => "i32".to_string(),
        DataType::I16 | DataType::U16 => "i16".to_string(),
        DataType::I8 | DataType::U8 => "i8".to_string(),
        DataType::F32 => "float".to_string(),
        DataType::F64 => "double".to_string(),
        DataType::Bool => "i1".to_string(),
        DataType::None => "i64".to_string(),
        DataType::StructNamed(name) => format!("struct:{}", name),
        _ => "i64".to_string(),
    }
}
