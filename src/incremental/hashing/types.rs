use super::*;

pub(super) fn hash_data_type(data_type: &DataType, hasher: &mut FxHasher) {
    match data_type {
        DataType::I8 => hasher.write_u8(0),
        DataType::I16 => hasher.write_u8(1),
        DataType::I32 => hasher.write_u8(2),
        DataType::I64 => hasher.write_u8(3),
        DataType::I128 => hasher.write_u8(39),
        DataType::U128 => hasher.write_u8(40),
        DataType::U8 => hasher.write_u8(4),
        DataType::U16 => hasher.write_u8(5),
        DataType::U32 => hasher.write_u8(6),
        DataType::U64 => hasher.write_u8(7),
        DataType::F32 => hasher.write_u8(8),
        DataType::F64 => hasher.write_u8(9),
        DataType::Char => hasher.write_u8(10),
        DataType::Str => hasher.write_u8(11),
        DataType::Bool => hasher.write_u8(12),
        DataType::None => hasher.write_u8(13),
        DataType::List => hasher.write_u8(14),
        DataType::Vector {
            element_type,
            dynamic,
        } => {
            hasher.write_u8(15);
            hash_data_type(element_type, hasher);
            dynamic.hash(hasher);
        }
        DataType::Dict => hasher.write_u8(16),
        DataType::Map {
            key_type,
            value_type,
        } => {
            hasher.write_u8(17);
            hash_data_type(key_type, hasher);
            hash_data_type(value_type, hasher);
        }
        DataType::Anything => hasher.write_u8(18),
        DataType::Function => hasher.write_u8(19),
        DataType::Struct => hasher.write_u8(20),
        DataType::StructNamed(name) => {
            hasher.write_u8(21);
            name.hash(hasher);
        }
        DataType::Db => hasher.write_u8(22),
        DataType::Tuple => hasher.write_u8(23),
        DataType::Set => hasher.write_u8(24),
        DataType::Datetime => hasher.write_u8(25),
        DataType::Unknown => hasher.write_u8(26),
        DataType::Ref { inner } => {
            hasher.write_u8(27);
            hash_data_type(inner, hasher);
        }
        DataType::RefMut { inner } => {
            hasher.write_u8(28);
            hash_data_type(inner, hasher);
        }
        DataType::Box => hasher.write_u8(29),
        DataType::Enum => hasher.write_u8(30),
        DataType::EnumNamed(name) => {
            hasher.write_u8(31);
            name.hash(hasher);
        }
        DataType::DynTrait { trait_name } => {
            hasher.write_u8(32);
            trait_name.hash(hasher);
        }
        DataType::Array { element_type, size } => {
            hasher.write_u8(33);
            hash_data_type(element_type, hasher);
            size.hash(hasher);
        }
        DataType::Slice { element_type } => {
            hasher.write_u8(34);
            hash_data_type(element_type, hasher);
        }
        DataType::Pointer(inner) => {
            hasher.write_u8(35);
            hash_data_type(inner, hasher);
        }
        DataType::Result { ok, err } => {
            hasher.write_u8(36);
            hash_data_type(ok, hasher);
            hash_data_type(err, hasher);
        }
        DataType::Generic(name) => {
            hasher.write_u8(37);
            name.hash(hasher);
        }
        DataType::Closure { params, return_type } => {
            hasher.write_u8(38);
            hash_data_types(params, hasher);
            hash_data_type(return_type, hasher);
        }
        DataType::Maybe { inner } => {
            hasher.write_u8(41);
            hash_data_type(inner, hasher);
        }
    }
}

pub(super) fn hash_data_types(types: &[DataType], hasher: &mut FxHasher) {
    types.len().hash(hasher);
    for t in types {
        hash_data_type(t, hasher);
    }
}
