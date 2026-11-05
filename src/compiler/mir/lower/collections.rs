use super::MirLower;
use super::types::{data_type_to_kind, extract_data_type, is_pointer_valued_type};
use crate::compiler::location::expression_location;
use crate::compiler::mir::{MirCmp, MirConst, MirOp, MirType, MirValue};
use crate::parser::ast::{DataType, Expression};

fn is_i64_wide_type(dt: &DataType) -> bool {
    matches!(
        dt,
        DataType::I64
            | DataType::I128
            | DataType::U64
            | DataType::U128
            | DataType::Char
            | DataType::Bool
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
    let ty = match expr {
        Expression::Identifier(id) => lower
            .var_types
            .get(&id.name)
            .cloned()
            .unwrap_or_else(|| id.data_type.clone()),
        _ => extract_data_type(expr),
    };
    // Indexing auto-derefs a reference to a collection, so `m[k]` on a
    // `&map`/`&dict` reaches the map instead of falling through to the scalar
    // index path.
    match ty {
        DataType::Ref { inner } | DataType::RefMut { inner } => *inner,
        other => other,
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
        } => (key_type.as_ref().clone(), value_type.as_ref().clone()),
        DataType::Dict => (DataType::Str, result_type.clone()),
        DataType::Unknown if matches!(index_type, DataType::Str) => {
            (DataType::Str, result_type.clone())
        }
        _ => return None,
    };
    // A map whose keys were still unknown at declaration reveals its key kind
    // from the index expression, exactly as a write would.
    let key_type = if matches!(key_type, DataType::Unknown | DataType::Anything)
        && !matches!(index_type, DataType::Unknown)
    {
        index_type.clone()
    } else {
        key_type
    };
    // The AST node type is Unknown while the map was untyped; the map's own
    // value kind (filled in by refine_indexed_target_type) is more precise.
    let result_type = if matches!(result_type, DataType::Unknown)
        && !matches!(value_type, DataType::Unknown | DataType::Anything)
    {
        value_type.clone()
    } else {
        result_type.clone()
    };

    let loc = expression_location(target).to_tuple();
    let target_val = lower.lower_expression(target);
    let index_val = lower.lower_expression(index);
    let key_kind = data_type_to_kind(&key_type);
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

    if is_pointer_valued_type(&value_type) || is_pointer_valued_type(&result_type) {
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
            loc,
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
            loc,
        );
        Some(narrow_scalar_result(
            lower,
            MirValue::temp(result),
            &result_type,
        ))
    }
}

/// Fill in the key/value kinds of an untyped `{}` once an indexed write
/// reveals them. `set m = {}` has no annotation, so lowering starts from
/// `Unknown`; without this, a later `m["key"]` read has no key/value kinds and
/// falls back to the scalar path, reading an i64 slot where the runtime stored
/// a pointer (or vice versa).
fn refine_indexed_target_type(
    lower: &mut MirLower,
    target: &Expression,
    key_type: &DataType,
    value_type: &DataType,
) {
    let Expression::Identifier(id) = target else {
        return;
    };
    let Some(current) = lower.var_types.get(&id.name).cloned() else {
        return;
    };
    let refine = |declared: &DataType, inferred: &DataType| -> DataType {
        if matches!(declared, DataType::Unknown | DataType::Anything) {
            inferred.clone()
        } else {
            declared.clone()
        }
    };
    let refined = match current {
        DataType::Unknown => {
            if matches!(key_type, DataType::Unknown | DataType::Anything) {
                return;
            }
            DataType::Map {
                key_type: Box::new(key_type.clone()),
                value_type: Box::new(value_type.clone()),
            }
        }
        DataType::Map {
            key_type: declared_key,
            value_type: declared_value,
        } => DataType::Map {
            key_type: Box::new(refine(&declared_key, key_type)),
            value_type: Box::new(refine(&declared_value, value_type)),
        },
        _ => return,
    };
    lower.var_types.insert(id.name.clone(), refined);
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
    // An untyped `{}` has no key kind of its own. When the index is a string,
    // the runtime stores a string-keyed dict; use that kind here instead of
    // defaulting Unknown to the integer kind.
    let key_type: DataType = match &target_type {
        DataType::Map { key_type, .. }
            if !matches!(**key_type, DataType::Unknown | DataType::Anything) =>
        {
            (**key_type).clone()
        }
        DataType::Map { .. } => index_type.clone(),
        DataType::Dict => DataType::Str,
        DataType::Unknown if matches!(index_type, DataType::Str) => DataType::Str,
        _ => return false,
    };

    let loc = expression_location(target).to_tuple();
    let target_val = lower.lower_expression(target);
    let index_val = lower.lower_expression(index);
    let key_kind = data_type_to_kind(&key_type);
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
        loc,
    );
    // The runtime may return a different map when the target was still an
    // untyped `{}` (the first insertion fixes the key/value kinds). Indexed
    // assignment has no rebind expression, so write the returned pointer back
    // into the identifier's slot; otherwise the insertion is lost.
    if let Expression::Identifier(id) = target
        && let Some(&slot) = lower.vars.get(&id.name)
    {
        let last = lower.current_block;
        lower.func.blocks[last].push(
            None,
            MirOp::Store(MirValue::temp(slot), MirValue::temp(result)),
            loc,
        );
    }
    // Remember the kinds this write established, so subsequent `m[k]` reads
    // use the same runtime entry points as the write.
    refine_indexed_target_type(lower, target, &index_type, value_type);
    true
}
