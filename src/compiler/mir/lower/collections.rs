use super::MirLower;
use super::types::{data_type_to_kind, extract_data_type, is_pointer_valued_type};
use crate::compiler::mir::{MirCmp, MirConst, MirOp, MirType, MirValue};
use crate::parser::ast::{DataType, Expression};

fn is_i64_wide_type(dt: &DataType) -> bool {
    matches!(
        dt,
        DataType::I64 | DataType::U64 | DataType::Char | DataType::Bool
    )
}

fn coerce_index_to_i64(lower: &mut MirLower, value: MirValue, value_type: &DataType) -> MirValue {
    if is_i64_wide_type(value_type) || matches!(value_type, DataType::Unknown | DataType::Anything)
    {
        return value;
    }

    let widened = lower.new_temp();
    let last = lower.current_block;
    lower.func.blocks[last].push(
        Some(widened),
        MirOp::ZExt(
            value,
            MirType {
                data_type: DataType::I64,
            },
        ),
        (0, 0),
    );
    MirValue::temp(widened)
}

fn coerce_value_for_scalar_store(
    lower: &mut MirLower,
    value: MirValue,
    value_type: &DataType,
) -> MirValue {
    if is_i64_wide_type(value_type) || matches!(value_type, DataType::Unknown | DataType::Anything)
    {
        return value;
    }

    let widened = lower.new_temp();
    let last = lower.current_block;
    lower.func.blocks[last].push(
        Some(widened),
        MirOp::ZExt(
            value,
            MirType {
                data_type: DataType::I64,
            },
        ),
        (0, 0),
    );
    MirValue::temp(widened)
}

fn null_ptr(lower: &mut MirLower) -> MirValue {
    let tmp = lower.new_temp();
    let last = lower.current_block;
    lower.func.blocks[last].push(
        Some(tmp),
        MirOp::IntToPtr(
            MirValue::Const(MirConst::Int(0)),
            MirType {
                data_type: DataType::Str,
            },
        ),
        (0, 0),
    );
    MirValue::temp(tmp)
}

fn effective_data_type(lower: &MirLower, expr: &Expression) -> DataType {
    match expr {
        Expression::Identifier(id) => lower
            .var_types
            .get(&id.name)
            .cloned()
            .unwrap_or_else(|| id.data_type.clone()),
        _ => extract_data_type(expr),
    }
}

fn narrow_scalar_result(lower: &mut MirLower, value: MirValue, result_type: &DataType) -> MirValue {
    match result_type {
        DataType::Bool => {
            let bool_tmp = lower.new_temp();
            let last = lower.current_block;
            lower.func.blocks[last].push(
                Some(bool_tmp),
                MirOp::ICmp(MirCmp::Ne, value, MirValue::Const(MirConst::Int(0))),
                (0, 0),
            );
            MirValue::temp(bool_tmp)
        }
        DataType::I8
        | DataType::U8
        | DataType::I16
        | DataType::U16
        | DataType::I32
        | DataType::U32 => {
            let narrowed = lower.new_temp();
            let last = lower.current_block;
            lower.func.blocks[last].push(
                Some(narrowed),
                MirOp::Trunc(
                    value,
                    MirType {
                        data_type: result_type.clone(),
                    },
                ),
                (0, 0),
            );
            MirValue::temp(narrowed)
        }
        _ => value,
    }
}

pub(crate) fn lower_index_read(
    lower: &mut MirLower,
    target: &Expression,
    index: &Expression,
    result_type: &DataType,
) -> Option<MirValue> {
    let target_type = effective_data_type(lower, target);
    let index_type = effective_data_type(lower, index);
    let (key_type, value_type) = match &target_type {
        DataType::Map {
            key_type,
            value_type,
        } => (key_type.as_ref(), value_type.as_ref()),
        DataType::Dict => (&DataType::Str, result_type),
        DataType::Unknown if matches!(index_type, DataType::Str) => (&DataType::Str, result_type),
        _ => return None,
    };

    let target_val = lower.lower_expression(target);
    let index_val = lower.lower_expression(index);
    let key_kind = data_type_to_kind(key_type);
    let key_i64 = if key_kind >= 3 {
        MirValue::Const(MirConst::Int(0))
    } else {
        coerce_index_to_i64(lower, index_val.clone(), &index_type)
    };
    let key_ptr = if key_kind >= 3 {
        index_val.clone()
    } else {
        null_ptr(lower)
    };
    let last = lower.current_block;

    if is_pointer_valued_type(value_type) || is_pointer_valued_type(result_type) {
        let result = lower.new_temp();
        let default_ptr = null_ptr(lower);
        lower.func.blocks[last].push(
            Some(result),
            MirOp::Call(
                MirValue::Global("rt_dict_get_ptr".to_string()),
                vec![
                    target_val,
                    MirValue::Const(MirConst::Int(key_kind)),
                    key_i64,
                    key_ptr,
                    default_ptr,
                ],
                MirType {
                    data_type: result_type.clone(),
                },
            ),
            (0, 0),
        );
        Some(MirValue::temp(result))
    } else {
        let result = lower.new_temp();
        lower.func.blocks[last].push(
            Some(result),
            MirOp::Call(
                MirValue::Global("rt_dict_get_i64".to_string()),
                vec![
                    target_val,
                    MirValue::Const(MirConst::Int(key_kind)),
                    key_i64,
                    key_ptr,
                    MirValue::Const(MirConst::Int(0)),
                ],
                MirType {
                    data_type: DataType::I64,
                },
            ),
            (0, 0),
        );
        Some(narrow_scalar_result(
            lower,
            MirValue::temp(result),
            result_type,
        ))
    }
}

pub(crate) fn lower_index_write(
    lower: &mut MirLower,
    target: &Expression,
    index: &Expression,
    value: MirValue,
    value_type: &DataType,
) -> bool {
    let target_type = effective_data_type(lower, target);
    let index_type = effective_data_type(lower, index);
    let key_type = match &target_type {
        DataType::Map { key_type, .. } => key_type.as_ref(),
        DataType::Dict => &DataType::Str,
        DataType::Unknown if matches!(index_type, DataType::Str) => &DataType::Str,
        _ => return false,
    };

    let target_val = lower.lower_expression(target);
    let index_val = lower.lower_expression(index);
    let key_kind = data_type_to_kind(key_type);
    let key_i64 = if key_kind >= 3 {
        MirValue::Const(MirConst::Int(0))
    } else {
        coerce_index_to_i64(lower, index_val.clone(), &index_type)
    };
    let key_ptr = if key_kind >= 3 {
        index_val
    } else {
        null_ptr(lower)
    };
    let value_kind = data_type_to_kind(value_type);
    let call_name = if is_pointer_valued_type(value_type) {
        "rt_dict_set_ptr"
    } else {
        "rt_dict_set_i64"
    };
    let stored_value = if call_name == "rt_dict_set_ptr" {
        value
    } else {
        coerce_value_for_scalar_store(lower, value, value_type)
    };

    let args = vec![
        target_val,
        MirValue::Const(MirConst::Int(key_kind)),
        MirValue::Const(MirConst::Int(value_kind)),
        key_i64,
        key_ptr,
        stored_value,
    ];

    let result = lower.new_temp();
    let last = lower.current_block;
    lower.func.blocks[last].push(
        Some(result),
        MirOp::Call(
            MirValue::Global(call_name.to_string()),
            args,
            MirType {
                data_type: target_type.clone(),
            },
        ),
        (0, 0),
    );
    true
}
