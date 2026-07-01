use crate::compiler::mir::DataType;

pub(crate) fn llvm_type_str(dt: &DataType) -> String {
    match dt {
        DataType::I64 | DataType::Char | DataType::U64 => "i64".to_string(),
        DataType::I32 | DataType::U32 => "i32".to_string(),
        DataType::I16 | DataType::U16 => "i16".to_string(),
        DataType::I8 | DataType::U8 => "i8".to_string(),
        DataType::F32 => "float".to_string(),
        DataType::F64 => "double".to_string(),
        DataType::Bool => "i1".to_string(),
        DataType::None => "i64".to_string(),
        DataType::Array { element_type, size } => {
            format!("[{} x {}]", size, llvm_type_str(element_type))
        }
        DataType::Slice { element_type } => llvm_type_str(element_type),
        DataType::EnumNamed(_) => "i64".to_string(),
        DataType::Generic(_) => "i64".to_string(),
        _ => "ptr".to_string(),
    }
}

pub(crate) fn render_struct_llvm_type(fields: &[(String, DataType)]) -> String {
    let tys: Vec<String> = fields.iter().map(|(_, dt)| llvm_type_str(dt)).collect();
    format!("{{ {} }}", tys.join(", "))
}
