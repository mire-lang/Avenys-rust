use super::*;
use crate::error::type_error_at_span;

impl TypeChecker {
    pub(super) fn split_nominal_type_args(name: &str) -> (&str, Vec<DataType>) {
        let trimmed = name.trim();
        if let Some(start) = trimmed.find('[')
            && trimmed.ends_with(']')
        {
            let base = trimmed[..start].trim();
            let inner = &trimmed[start + 1..trimmed.len() - 1];
            return (base, Self::parse_nominal_type_args(inner));
        }
        (trimmed, Vec::new())
    }

    pub(super) fn strip_root_namespace(name: &str) -> Option<String> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return None;
        }
        let normalized = trimmed.trim_start_matches("::");
        if let Some((_, rest)) = normalized.split_once('.') {
            return Some(rest.to_string());
        }
        if let Some((_, rest)) = normalized.split_once("::") {
            return Some(rest.to_string());
        }
        Some(normalized.to_string())
    }

    pub(super) fn parse_nominal_type_args(inner: &str) -> Vec<DataType> {
        let mut args = Vec::new();
        let mut depth: i32 = 0;
        let mut start = 0usize;
        for (idx, ch) in inner.char_indices() {
            match ch {
                '[' => depth += 1,
                ']' => depth -= 1,
                ',' if depth == 0 => {
                    let part = inner[start..idx].trim();
                    if !part.is_empty() {
                        args.push(Self::parse_nominal_type_arg(part));
                    }
                    start = idx + ch.len_utf8();
                }
                _ => {}
            }
        }
        let tail = inner[start..].trim();
        if !tail.is_empty() {
            args.push(Self::parse_nominal_type_arg(tail));
        }
        args
    }

    fn parse_nominal_type_arg(part: &str) -> DataType {
        let parsed = DataType::parse_type(part);
        if parsed == DataType::Unknown {
            DataType::Generic(part.to_string())
        } else {
            parsed
        }
    }

    pub(super) fn bindings_for_nominal_type_args(
        &self,
        type_params: &[String],
        type_args: &[DataType],
    ) -> Result<HashMap<String, DataType>> {
        if type_args.is_empty() {
            return Ok(HashMap::new());
        }
        if type_params.len() != type_args.len() {
            return Err(type_error_at_span(
                self.current_span,
                format!(
                    "Generic arity mismatch: expected {}, got {}",
                    type_params.len(),
                    type_args.len()
                ),
            ));
        }
        Ok(type_params
            .iter()
            .cloned()
            .zip(type_args.iter().cloned())
            .collect())
    }

    pub(super) fn canonical_enum_variant_name(name: &str) -> String {
        if let Some((enum_name, variant)) = name.split_once('.') {
            let (base, _) = Self::split_nominal_type_args(enum_name);
            format!("{}.{}", base, variant)
        } else if let Some(stripped) = Self::strip_root_namespace(name) {
            stripped
        } else {
            name.to_string()
        }
    }
}

pub(super) fn data_type_name_for_diag(data_type: &DataType) -> String {
    match data_type {
        DataType::EnumNamed(name) => {
            TypeChecker::strip_root_namespace(name).unwrap_or_else(|| name.clone())
        }
        DataType::StructNamed(name) => {
            TypeChecker::strip_root_namespace(name).unwrap_or_else(|| name.clone())
        }
        _ => format!("{:?}", data_type),
    }
}
