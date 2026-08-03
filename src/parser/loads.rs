use super::*;

impl Parser {
    pub(super) fn parse_load_statement(&mut self) -> Result<Statement> {
        if self.function_body_depth > 0 {
            return Err(self.error("`load` must be at the top level, not inside a function body"));
        }
        let load_token = self.peek();
        self.expect(TokenType::Load)?;

        if self.check(TokenType::Dot) {
            return Err(self.error("Local paths are not allowed; declare the dependency in owl.toml"));
        }

        let mut path = vec![self.expect_ident()?];
        while self.check(TokenType::Colon) && self.peek_n(1).ttype == TokenType::Colon {
            self.advance();
            self.advance();
            path.push(self.expect_ident()?);
        }

        let alias = if self.check(TokenType::As) {
            let as_token = self.peek();
            self.advance(); // as
            let _ = self.expect_ident()?;
            let span = crate::error::Span::new(as_token.line, as_token.column);
            return Err(crate::error::type_error_code_at_span(
                span,
                crate::error::DiagnosticCode::E0018,
                "Sorry? Did you just use an alias on a `load` statement (`load mire as foo`)?\
                 \n\nThe alias syntax is intentionally *prohibited*: `load` is the only \
                 path-based import and it exposes symbols under their real, fully-qualified \
                 module path (e.g. `mire::vec::push::i64`). Introducing an alias would hide \
                 where symbols come from and break cross-package analysis."
                    .to_string(),
            ));
        } else {
            None
        };

        let items = if self.check(TokenType::Colon) {
            self.advance();
            self.expect(TokenType::Lparen)?;
            let mut items = Vec::new();
            while !self.check(TokenType::Rparen) && !self.is_at_end() {
                items.push(self.expect_ident()?);
            }
            self.expect(TokenType::Rparen)?;
            Some(items)
        } else {
            None
        };

        Ok(Statement::Load {
            path,
            alias,
            items,
            line: load_token.line,
            column: load_token.column,
        })
    }

    /// Parse `load! math/main` (or `load! /math` for project-root relative).
    /// The path uses `/`-separated bare identifiers (not `::`). A leading `/`
    /// marks the path as absolute (resolved from the project root); otherwise
    /// it is resolved relative to the importing file's directory.
    pub(super) fn parse_load_bang_statement(&mut self) -> Result<Statement> {
        if self.function_body_depth > 0 {
            return Err(self.error("`load!` must be at the top level, not inside a function body"));
        }
        let load_bang_token = self.peek();
        self.expect(TokenType::Load)?;
        self.expect(TokenType::Bang)?;

        let absolute = self.check(TokenType::Slash);
        if absolute {
            self.advance();
        }

        let mut rel_path = vec![self.expect_ident()?];
        while self.check(TokenType::Slash) {
            self.advance();
            rel_path.push(self.expect_ident()?);
        }

        if rel_path.is_empty() {
            return Err(self.error("`load!` requires a relative path, e.g. `load! math/main`"));
        }

        Ok(Statement::LoadLocal {
            rel_path,
            absolute,
            line: load_bang_token.line,
            column: load_bang_token.column,
        })
    }
}
