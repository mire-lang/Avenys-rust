use super::*;

impl Parser {
    pub(super) fn parse_load_statement(&mut self) -> Result<Statement> {
        if self.function_body_depth > 0 {
            return Err(self.error("`load` must be at the top level, not inside a function body"));
        }
        let load_token = self.peek();
        self.expect(TokenType::Load)?;

        if self.check(TokenType::Dot) {
            return Err(
                self.error("Local paths are not allowed; declare the dependency in owl.toml")
            );
        }

        let mut path = vec![self.expect_ident()?];
        while self.check(TokenType::Colon) && self.peek_n(1).ttype == TokenType::Colon {
            self.advance();
            self.advance();
            path.push(self.expect_ident()?);
        }

        let alias = if self.check(TokenType::As) {
            self.advance();
            Some(self.expect_ident()?)
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

        Ok(Statement::Load { path, alias, items, line: load_token.line, column: load_token.column })
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

        Ok(Statement::LoadLocal { rel_path, absolute, line: load_bang_token.line, column: load_bang_token.column })
    }
}
