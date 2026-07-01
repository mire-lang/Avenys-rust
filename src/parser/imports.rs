use super::*;

impl Parser {
    pub(super) fn parse_load_statement(&mut self) -> Result<Statement> {
        if self.function_body_depth > 0 {
            return Err(self.error("`load` must be at the top level, not inside a function body"));
        }
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

        Ok(Statement::Load { path, alias, items })
    }
}
