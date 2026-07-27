use super::MirLower;
use super::types::llvm_elem_type_str;
use crate::compiler::mir::*;
use crate::parser::ast::{DataType, Expression, Literal};

impl MirLower {
    pub(crate) fn get_target_elem_type(&self, target: &Expression) -> String {
        if let Expression::Identifier(id) = target
            && let Some(ty) = self.var_types.get(&id.name)
        {
            match ty {
                DataType::Array { element_type, .. }
                | DataType::Vector { element_type, .. }
                | DataType::Slice { element_type, .. } => {
                    return llvm_elem_type_str(element_type);
                }
                _ => {}
            }
        }
        "i64".to_string()
    }

    pub(crate) fn get_struct_name(&self, expr: &Expression) -> Option<String> {
        match expr {
            Expression::Identifier(id) => self.var_types.get(&id.name).and_then(|t| match t {
                DataType::StructNamed(name) => Some(name.clone()),
                _ => None,
            }),
            _ => None,
        }
    }

    pub(crate) fn lower_literal(&self, lit: &Literal) -> MirValue {
        match lit {
            Literal::Int(v) => MirValue::Const(MirConst::Int(*v)),
            Literal::Float(v) => MirValue::Const(MirConst::Float(*v)),
            Literal::Bool(v) => MirValue::Const(MirConst::Bool(*v)),
            Literal::Char(v) => MirValue::Const(MirConst::Char(char::from_u32(*v).unwrap_or('\0'))),
            Literal::Str(v) => MirValue::Const(MirConst::Str(v.clone())),
            Literal::None => MirValue::Const(MirConst::None),
            _ => MirValue::Const(MirConst::None),
        }
    }

    pub(crate) fn emit_convert(
        &mut self,
        src_val: MirValue,
        src_type: &DataType,
        target_type: &DataType,
        loc: (usize, usize),
    ) -> MirValue {
        if src_type == target_type || *target_type == DataType::Unknown {
            return src_val;
        }
        use DataType::*;
        let op = match (src_type, target_type) {
            // Integer -> Float (signed)
            (
                I64 | I128 | I32 | I16 | I8 | Char,
                F64 | F32,
            ) => MirOp::Sitofp(src_val, MirType { data_type: target_type.clone() }),
            // Float -> Integer (fractional truncation)
            (
                F64 | F32,
                I64 | I128 | I32 | I16 | I8 | U64 | U128 | U32 | U16 | U8 | Char,
            ) => MirOp::Fptosi(src_val, MirType { data_type: target_type.clone() }),
            // Float -> Float (width change)
            (F64, F32) => MirOp::Fptrunc(src_val, MirType { data_type: target_type.clone() }),
            (F32, F64) => MirOp::Fpext(src_val, MirType { data_type: target_type.clone() }),
            // Integer -> Integer of different width / sign
            (s, t) if is_int_or_char(s) && is_int_or_char(t) => {
                let s_w = int_width(s);
                let t_w = int_width(t);
                if t_w >= s_w {
                    // Extension: sign-extended for signed types, zero-extended for unsigned.
                    if is_signed_int(s) {
                        MirOp::SExt(src_val, MirType { data_type: target_type.clone() })
                    } else {
                        MirOp::ZExt(src_val, MirType { data_type: target_type.clone() })
                    }
                } else {
                    MirOp::Trunc(src_val, MirType { data_type: target_type.clone() })
                }
            }
            _ => MirOp::Sitofp(src_val, MirType { data_type: target_type.clone() }),
        };
        let result = self.new_temp();
        let last = self.current_block;
        self.func.blocks[last].push(Some(result), op, loc);
        MirValue::temp(result)
    }
}

fn is_int_or_char(t: &DataType) -> bool {
    matches!(
        t,
        DataType::I8
            | DataType::I16
            | DataType::I32
            | DataType::I64
            | DataType::I128
            | DataType::U8
            | DataType::U16
            | DataType::U32
            | DataType::U64
            | DataType::U128
            | DataType::Char
    )
}

fn is_signed_int(t: &DataType) -> bool {
    matches!(
        t,
        DataType::I8 | DataType::I16 | DataType::I32 | DataType::I64 | DataType::I128 | DataType::Char
    )
}

fn int_width(t: &DataType) -> u32 {
    match t {
        DataType::I8 | DataType::U8 => 8,
        DataType::I16 | DataType::U16 => 16,
        DataType::I32 | DataType::U32 => 32,
        DataType::I64 | DataType::U64 => 64,
        DataType::I128 | DataType::U128 => 128,
        DataType::Char => 32,
        _ => 64,
    }
}
