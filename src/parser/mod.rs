pub mod ast;
mod expressions;
pub(crate) mod helpers;
mod imports;
mod statements;
mod syntax;
mod types;

use crate::error::{MireError, Result};
use crate::lexer::{Token, TokenType, tokenize};
use std::collections::{HashMap, HashSet};

pub use ast::{EnumDef, EnumVariantDef, MireValue, Program, Statement};
pub use helpers::{apply_map_type_to_dict, apply_vector_type_to_list};

pub fn parse(source: &str) -> Result<Program> {
    let tokens = tokenize(source)?;
    Parser::new(tokens).parse()
}

/// Parses source with error recovery: on parse error, skips to the next
/// statement boundary and continues. Returns the partial program and all
/// collected errors.
pub fn parse_with_recovery(source: &str) -> (Program, Vec<MireError>) {
    match tokenize(source) {
        Ok(tokens) => Parser::new(tokens).parse_with_recovery(),
        Err(e) => (
            Program {
                statements: Vec::new(),
            },
            vec![e],
        ),
    }
}

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    scopes: Vec<HashSet<String>>,
    enum_names: HashSet<String>,
    enum_variant_owners: HashMap<String, String>,
    nominal_type_names: HashSet<String>,
    method_context: usize,
    type_param_scopes: Vec<HashSet<String>>,
    function_body_depth: usize,
    errors: Vec<MireError>,
}

#[derive(Clone, Copy)]
enum BlockBoundary {
    Open,
    Close,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        let (enum_names, enum_variant_owners, nominal_type_names) =
            Self::collect_top_level_metadata(&tokens);
        Self {
            tokens,
            pos: 0,
            scopes: vec![HashSet::new()],
            enum_names,
            enum_variant_owners,
            nominal_type_names,
            method_context: 0,
            type_param_scopes: vec![HashSet::new()],
            function_body_depth: 0,
            errors: Vec::new(),
        }
    }

    fn collect_top_level_metadata(
        tokens: &[Token],
    ) -> (HashSet<String>, HashMap<String, String>, HashSet<String>) {
        let mut enum_names = HashSet::new();
        let mut nominal_type_names = HashSet::new();
        let mut variant_counts = HashMap::new();
        let mut enum_variant_owners = HashMap::new();
        let mut brace_depth = 0usize;
        let mut index = 0usize;

        while index < tokens.len() {
            match tokens[index].ttype {
                TokenType::Lbrace => brace_depth += 1,
                TokenType::Rbrace => brace_depth = brace_depth.saturating_sub(1),
                TokenType::Pub | TokenType::Priv
                    if brace_depth == 0 && index + 2 < tokens.len() =>
                {
                    let keyword = tokens[index + 1].ttype;
                    if matches!(
                        keyword,
                        TokenType::Enum | TokenType::Struct | TokenType::Type
                    ) && tokens[index + 2].ttype == TokenType::Ident
                        && let Some(name) = tokens[index + 2].value.as_ref()
                    {
                        match keyword {
                            TokenType::Enum => {
                                enum_names.insert(name.clone());
                                if index + 3 < tokens.len()
                                    && tokens[index + 3].ttype == TokenType::Lbrace
                                {
                                    index = Self::collect_enum_variants_into(
                                        tokens,
                                        index + 4,
                                        name,
                                        &mut variant_counts,
                                        &mut enum_variant_owners,
                                    );
                                    continue;
                                }
                            }
                            TokenType::Struct | TokenType::Type => {
                                nominal_type_names.insert(name.clone());
                            }
                            _ => {}
                        }
                    }
                }
                ttype
                    if brace_depth == 0
                        && matches!(
                            ttype,
                            TokenType::Enum | TokenType::Struct | TokenType::Type
                        ) =>
                {
                    if index + 1 < tokens.len()
                        && tokens[index + 1].ttype == TokenType::Ident
                        && let Some(name) = tokens[index + 1].value.as_ref()
                    {
                        match ttype {
                            TokenType::Enum => {
                                enum_names.insert(name.clone());
                                if index + 2 < tokens.len()
                                    && tokens[index + 2].ttype == TokenType::Lbrace
                                {
                                    index = Self::collect_enum_variants_into(
                                        tokens,
                                        index + 3,
                                        name,
                                        &mut variant_counts,
                                        &mut enum_variant_owners,
                                    );
                                    continue;
                                }
                            }
                            TokenType::Struct | TokenType::Type => {
                                nominal_type_names.insert(name.clone());
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
            index += 1;
        }

        enum_variant_owners.retain(|variant, _| variant_counts.get(variant) == Some(&1));
        (enum_names, enum_variant_owners, nominal_type_names)
    }

    fn collect_enum_variants_into(
        tokens: &[Token],
        mut index: usize,
        enum_name: &str,
        variant_counts: &mut HashMap<String, usize>,
        variant_owners: &mut HashMap<String, String>,
    ) -> usize {
        let mut enum_brace_depth = 1usize;

        while index < tokens.len() && enum_brace_depth > 0 {
            match tokens[index].ttype {
                TokenType::Lbrace => enum_brace_depth += 1,
                TokenType::Rbrace => enum_brace_depth = enum_brace_depth.saturating_sub(1),
                TokenType::Ident if enum_brace_depth == 1 => {
                    if let Some(variant_name) = tokens[index].value.as_ref() {
                        let variant_name = variant_name.clone();
                        *variant_counts.entry(variant_name.clone()).or_insert(0) += 1;
                        variant_owners.insert(variant_name, enum_name.to_string());
                    }
                }
                _ => {}
            }
            index += 1;
        }

        index
    }

    pub fn parse(&mut self) -> Result<Program> {
        let (program, errors) = self.parse_with_recovery();
        if let Some(err) = errors.into_iter().next() {
            return Err(err);
        }
        Ok(program)
    }

    pub fn parse_with_recovery(&mut self) -> (Program, Vec<MireError>) {
        let mut statements = Vec::new();
        while !self.is_at_end() {
            self.skip_newlines();
            if self.is_at_end() {
                break;
            }
            match self.parse_statement() {
                Ok(stmt) => statements.push(stmt),
                Err(err) => {
                    self.errors.push(err);
                    self.skip_to_statement_boundary();
                }
            }
            self.skip_newlines();
        }
        (Program { statements }, std::mem::take(&mut self.errors))
    }

    fn skip_to_statement_boundary(&mut self) {
        while !self.is_at_end() {
            let ttype = self.peek().ttype;
            if matches!(ttype, TokenType::Newline | TokenType::Eof) {
                break;
            }
            if ttype == TokenType::Rbrace {
                self.advance();
                break;
            }
            self.advance();
        }
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::parse;
    use crate::parser::ast::{DataType, Expression, Literal, Statement};

    #[test]
    fn parses_vector_type_annotation_without_bang() {
        let source = "set xs = [] :vec[i64]\n";
        let program = parse(source);
        assert!(program.is_ok(), "{program:?}");
    }

    #[test]
    fn rejects_legacy_vec_bang_type_annotation() {
        let source = "set xs = [] :vec![i64]\n";
        let program = parse(source);
        assert!(program.is_err(), "legacy vec! syntax must fail");
    }

    #[test]
    fn parses_lifecycle_statements() {
        let source = "new::() :vec[i64]\nown::(42) :i64\nmove::(x) to y\ndrop::(a, b)\n";
        let program = parse(source).expect("parse should succeed");
        assert!(matches!(program.statements[0], Statement::New { .. }));
        assert!(matches!(program.statements[1], Statement::Own { .. }));
        assert!(matches!(program.statements[2], Statement::Move { .. }));
        assert!(matches!(program.statements[3], Statement::Drop { .. }));
    }

    #[test]
    fn parses_find_statement() {
        let source = "find item in [1 2 3] {\n    use dasu(item)\n}\n";
        let program = parse(source).expect("parse should succeed");
        assert!(matches!(program.statements[0], Statement::Find { .. }));
    }

    #[test]
    fn parses_function_generics_and_explicit_type_args() {
        let source = "fn identity[T]: (x :T) :T { return x }\npub fn main: () { set a = identity[i64](42) }\n";
        let program = parse(source).expect("parse should succeed");
        let Statement::Function { type_params, .. } = &program.statements[0] else {
            panic!("expected function statement");
        };
        assert_eq!(type_params, &vec!["T".to_string()]);
    }

    #[test]
    fn parses_generic_impl_header() {
        let source = "type Box[T] { value :T }\nimpl[T] Box[T] {\n    fn get: (self) :T { return self.value }\n}\n";
        let program = parse(source).expect("parse should succeed");
        let Statement::Impl { type_params, .. } = &program.statements[1] else {
            panic!("expected impl statement");
        };
        assert_eq!(type_params, &vec!["T".to_string()]);
    }

    #[test]
    fn parses_inline_nested_vector_literal() {
        let source = "set xs = [[1 2] [3 4]] :vec[vec[i64]]\n";
        let program = parse(source);
        assert!(program.is_ok(), "{program:?}");
    }

    #[test]
    fn keeps_unknown_identifier_as_identifier_in_regular_call_arguments() {
        let source = "pub fn main: () {\nuse len(missing)\n}\n";
        let program = parse(source).expect("parse should succeed");

        let Statement::Function { body, .. } = &program.statements[0] else {
            panic!("expected function");
        };
        let Statement::Expression(Expression::Call { args, .. }) = &body[0] else {
            panic!("expected call expression");
        };
        assert!(matches!(args.first(), Some(Expression::Identifier(_))));
    }

    #[test]
    fn parses_load_and_brace_blocks() {
        let source = "load kioto\npub fn main: () {\n    use dasu(\"ok\")\n}\n";
        let program = parse(source);
        assert!(program.is_ok(), "{program:?}");
    }

    #[test]
    fn parses_load_statement() {
        let source = "load kioto\n";
        let program = parse(source).expect("parse should succeed");
        let Statement::Load { path, .. } = &program.statements[0] else {
            panic!("expected load statement");
        };
        assert_eq!(path, &["kioto".to_string()]);
    }

    #[test]
    fn rejects_local_load_with_dot_slash() {
        let source = "load ./utils/helpers\n";
        let program = parse(source);
        assert!(program.is_err(), "local paths should be rejected");
    }

    #[test]
    fn rejects_local_load_with_parent_segments() {
        let source = "load ./../modules/fs_ops\n";
        let program = parse(source);
        assert!(program.is_err(), "local paths should be rejected");
    }

    #[test]
    fn parses_static_impl_call_with_double_colon() {
        let source = "pub fn main: () {\nset p = Point::new(1, 2)\n}\n";
        let program = parse(source).expect("parse should succeed");

        let Statement::Function { body, .. } = &program.statements[0] else {
            panic!("expected function");
        };
        let Statement::Let {
            value: Some(Expression::Call { name, .. }),
            ..
        } = &body[0]
        else {
            panic!("expected call expression");
        };

        assert_eq!(name, "Point.new");
    }

    #[test]
    fn parses_module_chain_call_with_double_colon() {
        let source = "pub fn main: () {\nuse kioto::fs::read(\"Cargo.toml\")\n}\n";
        let program = parse(source).expect("parse should succeed");

        let Statement::Function { body, .. } = &program.statements[0] else {
            panic!("expected function");
        };
        let Statement::Expression(Expression::Call { name, .. }) = &body[0] else {
            panic!("expected call expression");
        };
        assert_eq!(name, "kioto.fs.read");
    }

    #[test]
    fn parses_keyword_member_name_in_namespace_call() {
        let source = "pub fn main: () {\nuse dicts::set(d, k, v)\n}\n";
        let program = parse(source).expect("parse should succeed");

        let Statement::Function { body, .. } = &program.statements[0] else {
            panic!("expected function");
        };
        let Statement::Expression(Expression::Call { name, .. }) = &body[0] else {
            panic!("expected call expression");
        };
        assert_eq!(name, "dicts.set");
    }

    #[test]
    fn parses_brace_map_literal() {
        let source = "set m = {a: 1, b: 2} :map[str i64]\n";
        let program = parse(source).expect("parse should succeed");

        let Statement::Let {
            value: Some(Expression::Dict { entries, .. }),
            ..
        } = &program.statements[0]
        else {
            panic!("expected dict literal");
        };

        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn parses_multiline_match_expression_without_leading_block_open() {
        let source = "pub fn main: () {\nset x = 5 :i64\nset result = match x {\n    1 { 10 }\n    _ { 0 }\n} :i64\n}\n";
        let program = parse(source).expect("parse should succeed");

        let Statement::Function { body, .. } = &program.statements[0] else {
            panic!("expected function");
        };
        let Statement::Let {
            value: Some(Expression::Match { cases, .. }),
            ..
        } = &body[1]
        else {
            panic!("expected match expression");
        };

        assert_eq!(cases.len(), 1);
    }

    #[test]
    fn parses_inline_match_expression_case_bodies() {
        let source = "pub fn main: () {\nset x = 5 :i64\nset result = match x { 1 { 10 } _ { 0 } } :i64\n}\n";
        let program = parse(source).expect("parse should succeed");

        let Statement::Function { body, .. } = &program.statements[0] else {
            panic!("expected function");
        };
        let Statement::Let {
            value: Some(Expression::Match { cases, default, .. }),
            ..
        } = &body[1]
        else {
            panic!("expected match expression");
        };

        assert_eq!(cases.len(), 1);
        assert!(matches!(
            default.as_ref(),
            Expression::Literal(Literal::Int(0))
        ));
    }

    #[test]
    fn parses_match_statement_case_bodies_as_statements() {
        let source = "pub fn main: () {\nset x = 1 :i64\nmatch x {\n    1 { set y = 10 }\n}\n}\n";
        let program = parse(source).expect("parse should succeed");

        let Statement::Function { body, .. } = &program.statements[0] else {
            panic!("expected function");
        };
        let Statement::Match { cases, .. } = &body[1] else {
            panic!("expected match statement");
        };

        assert!(matches!(cases[0].1[0], Statement::Let { .. }));
    }

    #[test]
    fn match_statement_consumes_closing_brace_before_following_statement() {
        let source = "pub fn main: () {\nset x = 1 :i64\nmatch x {\n    1 { set y = 10 }\n}\nset z = 20\n}\n";
        let program = parse(source).expect("parse should succeed");

        let Statement::Function { body, .. } = &program.statements[0] else {
            panic!("expected function");
        };

        assert!(matches!(body[1], Statement::Match { .. }));
        assert!(matches!(body[2], Statement::Let { .. }));
    }

    #[test]
    fn match_pattern_bindings_are_visible_inside_dasu_calls() {
        let source = "enum Result {\n    Ok(value :i64)\n}\n\npub fn main: () {\n    set result = Result.Ok(42)\n    match result {\n        Result.Ok(v) {\n            use dasu(v)\n        }\n    }\n}\n";
        let program = parse(source).expect("parse should succeed");

        let Statement::Function { body, .. } = &program.statements[1] else {
            panic!("expected function");
        };
        let Statement::Match { cases, .. } = &body[1] else {
            panic!("expected match statement");
        };
        let Statement::Expression(Expression::Call { args, .. }) = &cases[0].1[0] else {
            panic!("expected dasu call");
        };
        assert!(matches!(args.first(), Some(Expression::Identifier(_))));
    }

    #[test]
    fn resolves_unique_enum_variant_shorthand_in_match_patterns() {
        let source = "enum Result {\n    Ok(value :i64)\n}\n\npub fn main: () {\n    set result = Result.Ok(42)\n    match result {\n        Ok(v) {\n            use dasu(v)\n        }\n    }\n}\n";
        let program = parse(source).expect("parse should succeed");

        let Statement::Function { body, .. } = &program.statements[1] else {
            panic!("expected function");
        };
        let Statement::Match { cases, .. } = &body[1] else {
            panic!("expected match statement");
        };
        let Expression::EnumVariant {
            enum_name,
            variant_name,
            ..
        } = &cases[0].0
        else {
            panic!("expected enum variant pattern");
        };

        assert_eq!(enum_name, "Result");
        assert_eq!(variant_name, "Ok");
    }

    #[test]
    fn rejects_ambiguous_enum_variant_shorthand_in_match_patterns() {
        let source = "enum Result {\n    Ok(value :i64)\n}\n\nenum Response {\n    Ok(message :str)\n}\n\npub fn main: () {\n    set result = Result.Ok(42)\n    match result {\n        Ok(v) {\n            use dasu(v)\n        }\n    }\n}\n";
        let err = parse(source).expect_err("parse should reject ambiguous shorthand");
        let rendered = format!("{err}");
        assert!(rendered.contains("Cannot resolve enum variant shorthand 'Ok'"));
    }

    #[test]
    fn parses_enum_variants_with_multiple_payloads() {
        let source = "enum Pair {\n    Pair(left :i64 right :i64)\n}\n\npub fn main: () {\n    set pair = Pair.Pair(10 20)\n    match pair {\n        Pair.Pair(a b) {\n            use dasu(\"{a} {b}\")\n        }\n    }\n}\n";
        let program = parse(source).expect("parse should succeed");

        let Statement::Function { body, .. } = &program.statements[1] else {
            panic!("expected function");
        };
        let Statement::Let {
            value: Some(Expression::EnumVariant { payloads, .. }),
            ..
        } = &body[0]
        else {
            panic!("expected enum variant construction");
        };
        assert_eq!(payloads.len(), 2);

        let Statement::Match { cases, .. } = &body[1] else {
            panic!("expected match statement");
        };
        let Expression::EnumVariant { payloads, .. } = &cases[0].0 else {
            panic!("expected enum variant pattern");
        };
        assert_eq!(payloads.len(), 2);
    }

    #[test]
    fn parses_named_enum_variant_arguments() {
        let source = "enum Status {\n    Loading(progress :i64 total :i64)\n}\n\npub fn main: () {\n    set loading = Status.Loading(total: 100, progress: 75)\n}\n";
        let program = parse(source).expect("parse should succeed");

        let Statement::Enum { variants, .. } = &program.statements[0] else {
            panic!("expected enum");
        };
        assert_eq!(variants[0].payload_names, vec!["progress", "total"]);

        let Statement::Function { body, .. } = &program.statements[1] else {
            panic!("expected function");
        };
        let Statement::Let {
            value: Some(Expression::EnumVariant { payloads, .. }),
            ..
        } = &body[0]
        else {
            panic!("expected enum variant construction");
        };
        assert!(matches!(
            &payloads[0],
            Expression::NamedArg { name, .. } if name == "total"
        ));
        assert!(matches!(
            &payloads[1],
            Expression::NamedArg { name, .. } if name == "progress"
        ));
    }

    #[test]
    fn desugars_pipeline_self_placeholder_into_direct_stage_expression() {
        let source = "pub fn main: () {\nuse range(5) => dasu(self)\n}\n";
        let program = parse(source).expect("parse should succeed");

        let Statement::Function { body, .. } = &program.statements[0] else {
            panic!("expected function");
        };

        assert!(matches!(
            body[0],
            Statement::Expression(Expression::Call { .. })
        ));
    }

    #[test]
    fn parses_closure_signature_syntax_with_multiple_params() {
        let source =
            "pub fn main: () {\nset result = lists.fold(0, (acc elem) => acc + elem, [1 2 3])\n}\n";
        let program = parse(source).expect("parse should succeed");

        let Statement::Function { body, .. } = &program.statements[0] else {
            panic!("expected function");
        };
        let Statement::Let {
            value: Some(Expression::Call { args, .. }),
            ..
        } = &body[0]
        else {
            panic!("expected let call");
        };
        let Expression::Closure { params, .. } = &args[1] else {
            panic!("expected closure");
        };

        assert_eq!(params.len(), 2);
        assert_eq!(params[0].0, "acc");
        assert_eq!(params[1].0, "elem");
    }

    #[test]
    fn parses_closure_signature_syntax_with_annotations() {
        let source = "pub fn main: () {\nset result = lists.map((x: i64) => x * 2, [1 2 3])\n}\n";
        let program = parse(source).expect("parse should succeed");

        let Statement::Function { body, .. } = &program.statements[0] else {
            panic!("expected function");
        };
        let Statement::Let {
            value: Some(Expression::Call { args, .. }),
            ..
        } = &body[0]
        else {
            panic!("expected let call");
        };
        let Expression::Closure { params, .. } = &args[0] else {
            panic!("expected closure");
        };

        assert_eq!(params[0].0, "x");
        assert_eq!(params[0].1, DataType::I64);
    }

    #[test]
    fn if_expression_preserves_unknown_identifiers_in_branches() {
        let source = "pub fn main: () {\nset result = if true { missing } else { 0 }\n}\n";
        let program = parse(source).expect("parse should succeed");

        let Statement::Function { body, .. } = &program.statements[0] else {
            panic!("expected function");
        };
        let Statement::Let {
            value: Some(Expression::Call { args, .. }),
            ..
        } = &body[0]
        else {
            panic!("expected let call");
        };
        let Expression::Closure { body, .. } = &args[1] else {
            panic!("expected then-closure");
        };
        let Statement::Return(Some(Expression::Identifier(ident))) = &body[0] else {
            panic!("expected identifier return");
        };
        assert_eq!(ident.name, "missing");
    }

    #[test]
    fn subparsers_preserve_nominal_context_for_conditions() {
        let source = "enum Result {\n    Ok\n}\n\npub fn main: () {\n    if Result.Ok {\n        use dasu(\"ok\")\n    }\n}\n";
        let program = parse(source).expect("parse should succeed");

        let Statement::Function { body, .. } = &program.statements[1] else {
            panic!("expected function");
        };
        let Statement::If { condition, .. } = &body[0] else {
            panic!("expected if statement");
        };

        assert!(matches!(condition, Expression::EnumVariantPath { .. }));
    }

    #[test]
    fn parses_secondary_for_loop_binding() {
        let source = "pub fn main: () {\nfor item, index in [1 2 3] {\n    use dasu(item)\n}\n}\n";
        let program = parse(source).expect("parse should accept two-binding for loop");
        let Statement::Function { body, .. } = &program.statements[0] else {
            panic!("expected function");
        };
        let Statement::For {
            variable, index, ..
        } = &body[0]
        else {
            panic!("expected for");
        };
        assert_eq!(variable, "item");
        assert_eq!(index.as_deref(), Some("index"));
    }

    #[test]
    fn parses_prefixed_integer_literals() {
        let source =
            "pub fn main: () {\nset b = 0b1010 :i64\nset o = 0o12 :i64\nset h = 0xFF :i64\n}\n";
        let program = parse(source).expect("parse should accept based integer literals");
        let Statement::Function { body, .. } = &program.statements[0] else {
            panic!("expected function");
        };
        let Statement::Let {
            value: Some(Expression::Literal(Literal::Int(b))),
            ..
        } = &body[0]
        else {
            panic!("expected first int literal");
        };
        let Statement::Let {
            value: Some(Expression::Literal(Literal::Int(o))),
            ..
        } = &body[1]
        else {
            panic!("expected second int literal");
        };
        let Statement::Let {
            value: Some(Expression::Literal(Literal::Int(h))),
            ..
        } = &body[2]
        else {
            panic!("expected third int literal");
        };
        assert_eq!((*b, *o, *h), (10, 10, 255));
    }

    #[test]
    fn parses_raw_strings_with_hash_delimiters() {
        let source = "pub fn main: () {\nset a = r\"hello\"\nset b = r#\"hello \"world\"\"#\nset c = r##\"hello \"world\" with ##\"##\n}\n";
        let program = parse(source).expect("parse should accept raw strings");
        let Statement::Function { body, .. } = &program.statements[0] else {
            panic!("expected function");
        };
        let Statement::Let {
            value: Some(Expression::Literal(Literal::Str(a))),
            ..
        } = &body[0]
        else {
            panic!("expected first raw string");
        };
        let Statement::Let {
            value: Some(Expression::Literal(Literal::Str(b))),
            ..
        } = &body[1]
        else {
            panic!("expected second raw string");
        };
        let Statement::Let {
            value: Some(Expression::Literal(Literal::Str(c))),
            ..
        } = &body[2]
        else {
            panic!("expected third raw string");
        };
        assert_eq!(a, "hello");
        assert_eq!(b, "hello \"world\"");
        assert_eq!(c, "hello \"world\" with ##");
    }

    #[test]
    fn parses_char_literals_as_unicode_scalar_u32() {
        let source =
            "pub fn main: () {\nset a = 'a' :char\nset n = '\\n' :char\nset u = 'ñ' :char\n}\n";
        let program = parse(source).expect("parse should accept char literals");
        let Statement::Function { body, .. } = &program.statements[0] else {
            panic!("expected function");
        };
        let Statement::Let {
            value: Some(Expression::Literal(Literal::Char(a))),
            ..
        } = &body[0]
        else {
            panic!("expected first char");
        };
        let Statement::Let {
            value: Some(Expression::Literal(Literal::Char(n))),
            ..
        } = &body[1]
        else {
            panic!("expected second char");
        };
        let Statement::Let {
            value: Some(Expression::Literal(Literal::Char(u))),
            ..
        } = &body[2]
        else {
            panic!("expected third char");
        };
        assert_eq!((*a, *n, *u), ('a' as u32, '\n' as u32, 'ñ' as u32));
    }

    #[test]
    fn parses_unsafe_block_statement() {
        let source = "pub fn main: () {\nunsafe {\nset x = 2 :i64\n}\n}\n";
        let program = parse(source).expect("parse should accept unsafe blocks");
        let Statement::Function { body, .. } = &program.statements[0] else {
            panic!("expected function");
        };
        assert!(matches!(body[0], Statement::Unsafe { .. }));
    }

    #[test]
    fn parses_extern_lib_and_fn_statements() {
        let source =
            "extern lib \"c\" \"libc.so.6\"\nextern fn puts: (msg :*const i8) :i32 lib \"c\"\n";
        let program = parse(source).expect("parse should accept extern lib/fn");
        assert!(matches!(program.statements[0], Statement::ExternLib { .. }));
        let Statement::ExternFunction { name, lib_name, .. } = &program.statements[1] else {
            panic!("expected extern fn");
        };
        assert_eq!(name, "puts");
        assert_eq!(lib_name, "c");
    }

    #[test]
    fn parses_inline_asm_block() {
        let source = "pub fn main: () {\nasm {\nmov rax, rbx\nadd rax, rcx\n}\n}\n";
        let program = parse(source).expect("parse should accept asm block");
        let Statement::Function { body, .. } = &program.statements[0] else {
            panic!("expected function");
        };
        let Statement::Asm { instructions } = &body[0] else {
            panic!("expected asm");
        };
        assert_eq!(instructions.len(), 2);
        assert_eq!(instructions[0].0, "mov");
        assert_eq!(instructions[1].0, "add");
    }

    #[test]
    fn rejects_duplicate_visibility_before_type_declaration() {
        let source = "pub pub type Point {\n    x :i64\n}\n";
        assert!(parse(source).is_err());
    }

    #[test]
    fn rejects_legacy_angle_block_syntax() {
        let source = "pub fn main: () >\nuse dasu(\"no\")\n<\n";
        let program = parse(source);
        assert!(program.is_err(), "legacy angle blocks should be rejected");
    }
}
