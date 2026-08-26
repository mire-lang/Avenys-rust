use super::*;

impl Parser {
    pub(super) fn collect_top_level_metadata(
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
                    if matches!(keyword, TokenType::Enum | TokenType::Struct | TokenType::Type)
                        && tokens[index + 2].ttype == TokenType::Ident
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
                        && matches!(ttype, TokenType::Enum | TokenType::Struct | TokenType::Type) =>
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
}
