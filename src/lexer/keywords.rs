use super::{Token, TokenType};

pub(super) fn token_for(ident: String, start_line: usize, start_col: usize) -> Token {
    let token = match ident.as_str() {
        "set" => Token::new(TokenType::Set, start_line, start_col).with_value("set".to_string()),
        "load" => Token::new(TokenType::Load, start_line, start_col),
        "module" => Token::new(TokenType::Module, start_line, start_col),
        "use" => Token::new(TokenType::Use, start_line, start_col).with_value("use".to_string()),
        "return" => {
            Token::new(TokenType::Return, start_line, start_col).with_value("return".to_string())
        }
        "if" => Token::new(TokenType::If, start_line, start_col).with_value("if".to_string()),
        "elif" => Token::new(TokenType::Elif, start_line, start_col),
        "else" => Token::new(TokenType::Else, start_line, start_col).with_value("else".to_string()),
        "while" => {
            Token::new(TokenType::While, start_line, start_col).with_value("while".to_string())
        }
        "for" => Token::new(TokenType::For, start_line, start_col).with_value("for".to_string()),
        "find" => Token::new(TokenType::Find, start_line, start_col).with_value("find".to_string()),
        "do" => Token::new(TokenType::Do, start_line, start_col).with_value("do".to_string()),
        "in" => Token::new(TokenType::In, start_line, start_col).with_value("in".to_string()),
        "fn" => Token::new(TokenType::Fn, start_line, start_col).with_value("fn".to_string()),
        "type" => Token::new(TokenType::Type, start_line, start_col).with_value("type".to_string()),
        "skill" => {
            Token::new(TokenType::Skill, start_line, start_col).with_value("skill".to_string())
        }
        "struct" => {
            Token::new(TokenType::Struct, start_line, start_col).with_value("struct".to_string())
        }
        "impl" => Token::new(TokenType::Impl, start_line, start_col).with_value("impl".to_string()),
        "enum" => Token::new(TokenType::Enum, start_line, start_col).with_value("enum".to_string()),
        "extern" => {
            Token::new(TokenType::Extern, start_line, start_col).with_value("extern".to_string())
        }
        "lib" => Token::new(TokenType::Lib, start_line, start_col).with_value("lib".to_string()),
        "unsafe" => {
            Token::new(TokenType::Unsafe, start_line, start_col).with_value("unsafe".to_string())
        }
        "asm" => Token::new(TokenType::Asm, start_line, start_col).with_value("asm".to_string()),
        "extends" => {
            Token::new(TokenType::Extends, start_line, start_col).with_value("extends".to_string())
        }
        "mu" => Token::new(TokenType::NoneLit, start_line, start_col).with_value("mu".to_string()),
        "match" => {
            Token::new(TokenType::Match, start_line, start_col).with_value("match".to_string())
        }
        "new" => Token::new(TokenType::NewKw, start_line, start_col).with_value("new".to_string()),
        "drop" => {
            Token::new(TokenType::DropKw, start_line, start_col).with_value("drop".to_string())
        }
        "move" => {
            Token::new(TokenType::MoveKw, start_line, start_col).with_value("move".to_string())
        }
        "own" => Token::new(TokenType::OwnKw, start_line, start_col).with_value("own".to_string()),
        "pub" => Token::new(TokenType::Pub, start_line, start_col).with_value("pub".to_string()),
        "priv" => Token::new(TokenType::Priv, start_line, start_col),
        "const" => Token::new(TokenType::Const, start_line, start_col),
        "cons" => Token::new(TokenType::Cons, start_line, start_col).with_value("cons".to_string()),
        "mut" => Token::new(TokenType::Mut, start_line, start_col).with_value("mut".to_string()),
        "as" => Token::new(TokenType::As, start_line, start_col),
        "is" => Token::new(TokenType::Is, start_line, start_col),
        "of" => Token::new(TokenType::Of, start_line, start_col),
        "to" => Token::new(TokenType::To, start_line, start_col),
        "at" => Token::new(TokenType::At, start_line, start_col),
        "self" => Token::new(TokenType::SelfToken, start_line, start_col),
        "break" => Token::new(TokenType::Break, start_line, start_col),
        "continue" => Token::new(TokenType::Continue, start_line, start_col),
        "true" | "false" => Token::new(TokenType::BoolLit, start_line, start_col).with_value(ident),
        _ => Token::new(TokenType::Ident, start_line, start_col).with_value(ident),
    };

    token
}
