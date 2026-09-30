// Derive expansion: reads @[derive(...)] from struct declarations and synthesizes
// impl blocks / functions into the program before typeck.
//
// This is compile-time sugar: the generated code is ordinary Mire source text
// parsed into statements that go through the normal typeck/MIR/codegen pipeline.
// A user could have written the generated code by hand; the derive attribute is
// consumed after expansion. If a generated call references a symbol that does not
// exist (e.g. `vec::clone`), typeck reports an ordinary error at that call site.
//
// Supported derives (structs only; enums are future work):
//   Default    -> `fn default: () :T`  no external helpers
//   Clone      -> `fn clone: (self) :T`  uses `str::copy` for str fields
//   PartialEq  -> `fn equals: (self other :&T) :bool`  uses `==`
//   Debug      -> `fn to_string: (self) :str`  uses `str::from::*` and `+`
//
// The generated Debug/Clone code references stdlib functions (`str::copy`,
// `str::from::i64`, ...) so the enclosing program must load the module that
// provides them (e.g. `load mire::str` or `load kioto`), exactly as a hand-written
// impl would.
//
// Text-level expansion: `expand_derives_source` parses source, finds structs with
// @[derive], generates impl source, and splices it after the struct's closing `}`.
// Runs in the loader before reachable-import selection, so generated impls participate
// in dependency candidate collection (e.g. `str::copy` is reachable-selected).

use crate::parser::{
    ast::{DataType, EnumVariantDef, Program, Statement},
    parse,
};
use std::collections::{HashMap, HashSet};

/// Expands @[derive(...)] attributes at the SOURCE TEXT level.
/// Parses the source, finds structs with derive attributes, generates impl
/// source for each, and splices it immediately after the struct declaration.
/// This preserves real spans so error messages point to actual source lines.
pub fn expand_derives_source(source: &str) -> String {
    // Parse to get positions
    let program = match parse(source) {
        Ok(p) => p,
        Err(_) => return source.to_string(), // On parse error, return original
    };

    let (_nominal, _enums, _owners) = collect_known_type_names(&program);

    // Helper: convert (line, column) 1-indexed to byte index in source
    let line_to_byte = |line: usize, column: usize| -> usize {
        let mut byte = 0;
        for (i, l) in source.lines().enumerate() {
            if i + 1 == line {
                return byte + column.saturating_sub(1).min(l.len());
            }
            byte += l.len() + 1; // +1 for newline
        }
        source.len()
    };

    // Collect (end_position, impl_source) for each struct with derives
    let mut splices: Vec<(usize, String)> = Vec::new();
    for stmt in &program.statements {
        if let Statement::Type {
            name,
            fields,
            attributes,
            end_line,
            end_column,
            ..
        } = stmt
        {
            let derive_names: Option<Vec<&str>> =
                attributes.iter().find(|a| a.name == "derive").map(|attr| {
                    attr.args
                        .iter()
                        .map(|arg| arg.value.as_str())
                        .map(|v| v.trim())
                        .filter(|v| !v.is_empty())
                        .collect()
                });

            if let Some(names) = derive_names {
                let mut gen_src = String::new();
                let mut any = false;
                for derive_name in &names {
                    match *derive_name {
                        "Default" => {
                            gen_src.push_str(&gens_default_impl(name, fields));
                            any = true;
                        }
                        "Clone" => {
                            gen_src.push_str(&gens_clone_impl(name, fields));
                            any = true;
                        }
                        "PartialEq" => {
                            gen_src.push_str(&gens_partialeq_impl(name, fields));
                            any = true;
                        }
                        "Debug" => {
                            gen_src.push_str(&gens_debug_impl(name, fields));
                            any = true;
                        }
                        _ => {}
                    }
                }
                if any {
                    // Compute byte position of the struct's closing '}' and insert AFTER it
                    let brace_pos = line_to_byte(*end_line, *end_column).min(source.len());
                    // Find the closing '}' and position after it
                    let after_brace = source[brace_pos..]
                        .find('}')
                        .map(|i| brace_pos + i + 1)
                        .unwrap_or(source.len());
                    splices.push((after_brace, gen_src));
                }
            }
        }
    }

    if splices.is_empty() {
        return source.to_string();
    }

    // Sort by position descending so we splice from end to start (offsets stay valid)
    splices.sort_by_key(|(pos, _)| *pos);
    splices.reverse();

    let mut out = source.to_string();
    for (pos, impl_src) in splices {
        // Insert after the struct: newline + impl + newline
        let insertion = format!("\n{}\n", impl_src);
        out.insert_str(pos, &insertion);
    }
    out
}

// ---------------------------------------------------------------------------
// Helper: collect known type names for parser seeding
// ---------------------------------------------------------------------------

fn collect_known_type_names(
    program: &Program,
) -> (HashSet<String>, HashSet<String>, HashMap<String, String>) {
    let mut nominal = HashSet::new();
    let mut enums = HashSet::new();
    let mut owners: HashMap<String, String> = HashMap::new();
    for stmt in &program.statements {
        match stmt {
            Statement::Type { name, .. } => {
                nominal.insert(name.clone());
            }
            Statement::Enum { name, variants, .. } => {
                enums.insert(name.clone());
                for EnumVariantDef { name: vname, .. } in variants {
                    owners.insert(vname.clone(), name.clone());
                }
            }
            _ => {}
        }
    }
    (nominal, enums, owners)
}

// ---------------------------------------------------------------------------
// Field metadata
// ---------------------------------------------------------------------------

struct FieldMeta {
    name: String,
    ty: DataType,
}

fn field_metas(fields: &[Statement]) -> Vec<FieldMeta> {
    fields
        .iter()
        .filter_map(|f| match f {
            Statement::Let {
                name, data_type, ..
            } => Some(FieldMeta {
                name: name.clone(),
                ty: data_type.clone(),
            }),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Generators: each returns a Mire source string for one `impl` block.
// ---------------------------------------------------------------------------

fn gens_default_impl(struct_name: &str, fields: &[Statement]) -> String {
    let metas = field_metas(fields);
    let inits: Vec<String> = metas
        .iter()
        .map(|f| format!("{}: {}", f.name, zero_expr(&f.ty)))
        .collect();
    format!(
        "impl {struct_name} {{\n  pub fn default: () :{struct_name} {{\n    return ({struct_name} {})\n  }}\n}}\n",
        inits.join(", ")
    )
}

fn gens_clone_impl(struct_name: &str, fields: &[Statement]) -> String {
    let metas = field_metas(fields);
    let inits: Vec<String> = metas
        .iter()
        .map(|f| format!("{}: {}", f.name, clone_expr(&f.name, &f.ty)))
        .collect();
    format!(
        "impl {struct_name} {{\n  pub fn clone: (self) :{struct_name} {{\n    return ({struct_name} {})\n  }}\n}}\n",
        inits.join(", ")
    )
}

fn gens_partialeq_impl(struct_name: &str, fields: &[Statement]) -> String {
    let metas = field_metas(fields);
    if metas.is_empty() {
        return format!(
            "impl {struct_name} {{\n  pub fn equals: (self other :&{struct_name}) :bool {{\n    return true\n  }}\n}}\n"
        );
    }
    let conds: Vec<String> = metas
        .iter()
        .map(|f| equality_expr(&f.name, &f.ty))
        .collect();
    format!(
        "impl {struct_name} {{\n  pub fn equals: (self other :&{struct_name}) :bool {{\n    return {}\n  }}\n}}\n",
        conds.join(" && ")
    )
}

fn gens_debug_impl(struct_name: &str, fields: &[Statement]) -> String {
    let metas = field_metas(fields);
    // Deliberately no `{`/`}` in the output: they are string-interpolation
    // markers in Mire. Use a parenthesised `Name(f: v, ...)` form instead.
    let mut parts: Vec<String> = vec![format!("\"{struct_name}(\"")];
    for (i, meta) in metas.iter().enumerate() {
        parts.push(debug_arg(&meta.name, &meta.ty));
        if i + 1 < metas.len() {
            parts.push("\", \"".to_string());
        }
    }
    parts.push("\")\"".to_string());

    format!(
        "impl {struct_name} {{\n  pub fn to_string: (self) :str {{\n    return {}\n  }}\n}}\n",
        parts.join(" + ")
    )
}

// ---------------------------------------------------------------------------
// Per-field expression helpers
// ---------------------------------------------------------------------------

fn zero_expr(ty: &DataType) -> String {
    match ty {
        DataType::I8
        | DataType::I16
        | DataType::I32
        | DataType::I64
        | DataType::I128
        | DataType::U8
        | DataType::U16
        | DataType::U32
        | DataType::U64
        | DataType::U128 => "0".to_string(),
        DataType::F32 | DataType::F64 => "0.0".to_string(),
        DataType::Bool => "false".to_string(),
        DataType::Str => "\"\"".to_string(),
        DataType::Vector { element_type, .. } => {
            format!("[] :vec[{}]", type_to_string(element_type))
        }
        DataType::Map {
            key_type,
            value_type,
        } => {
            format!(
                "{{}} :map[{} {}]",
                type_to_string(key_type),
                type_to_string(value_type)
            )
        }
        DataType::Maybe { inner } => format!("None :{}", type_to_string(inner)),
        DataType::StructNamed(n) => format!("{n}::default()"),
        _ => "\"\"".to_string(),
    }
}

fn clone_expr(field_name: &str, ty: &DataType) -> String {
    match ty {
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
        | DataType::F32
        | DataType::F64
        | DataType::Bool => format!("self.{field_name}"),
        DataType::Str => format!("str::copy(self.{field_name})"),
        DataType::Vector { .. } => format!("vec::clone(self.{field_name})"),
        DataType::Map { .. } => format!("map::clone(self.{field_name})"),
        DataType::StructNamed(_) => format!("self.{field_name}.clone()"),
        _ => format!("self.{field_name}"),
    }
}

fn equality_expr(field_name: &str, ty: &DataType) -> String {
    match ty {
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
        | DataType::F32
        | DataType::F64
        | DataType::Bool
        | DataType::Str => format!("self.{field_name} == other.{field_name}"),
        DataType::StructNamed(_) => format!("self.{field_name}.equals(other.{field_name})"),
        _ => "true".to_string(),
    }
}

fn debug_arg(field_name: &str, ty: &DataType) -> String {
    // Each arg is `"name: " + <value expression>`. The label is a quoted
    // literal so it parses as a string, and no `{}` interpolation is used.
    match ty {
        DataType::I8
        | DataType::I16
        | DataType::I32
        | DataType::I64
        | DataType::I128
        | DataType::U8
        | DataType::U16
        | DataType::U32
        | DataType::U64
        | DataType::U128 => {
            format!("\"{field_name}: \" + str::from::i64(self.{field_name})")
        }
        DataType::F32 | DataType::F64 => {
            format!("\"{field_name}: \" + str::from::f64(self.{field_name})")
        }
        DataType::Bool => {
            format!("\"{field_name}: \" + str::from::bool(self.{field_name})")
        }
        DataType::Str => format!("\"{field_name}: \" + self.{field_name}"),
        DataType::Vector { .. } => format!("\"{field_name}: [...]\""),
        DataType::Map { .. } => format!("\"{field_name}: [map]\""),
        DataType::StructNamed(_) => {
            format!("\"{field_name}: \" + self.{field_name}.to_string()")
        }
        _ => format!("\"{field_name}: \""),
    }
}

fn type_to_string(ty: &DataType) -> String {
    match ty {
        DataType::I8 => "i8".into(),
        DataType::I16 => "i16".into(),
        DataType::I32 => "i32".into(),
        DataType::I64 => "i64".into(),
        DataType::I128 => "i128".into(),
        DataType::U8 => "u8".into(),
        DataType::U16 => "u16".into(),
        DataType::U32 => "u32".into(),
        DataType::U64 => "u64".into(),
        DataType::U128 => "u128".into(),
        DataType::F32 => "f32".into(),
        DataType::F64 => "f64".into(),
        DataType::Bool => "bool".into(),
        DataType::Str => "str".into(),
        DataType::Vector { element_type, .. } => {
            format!("vec[{}]", type_to_string(element_type))
        }
        DataType::Map {
            key_type,
            value_type,
        } => {
            format!(
                "map[{} {}]",
                type_to_string(key_type),
                type_to_string(value_type)
            )
        }
        DataType::Maybe { inner } => format!("Maybe[{}]", type_to_string(inner)),
        DataType::StructNamed(n) => n.clone(),
        _ => "i64".into(),
    }
}
