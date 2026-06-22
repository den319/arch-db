use std::fmt::{Display, Formatter, Result};

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // keywords
    Select,
    Insert,
    Delete,
    Update,
    Create,

    Into,
    Values,
    From,
    Where,
    Table,

    // symbols
    LeftParen,
    RightParen,
    Comma,
    Semicolon,
    Star,
    Equal,

    // literals
    Identifier(String),
    String(String),
    Number(i64),

    // special
    EOF,
}

impl Display for Token {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            Token::Select => write!(f, "SELECT"),
            Token::Insert => write!(f, "INSERT"),
            Token::Delete => write!(f, "DELETE"),
            Token::Update => write!(f, "UPDATE"),
            Token::Create => write!(f, "CREATE"),

            Token::Into => write!(f, "INTO"),
            Token::Values => write!(f, "VALUES"),
            Token::From => write!(f, "FROM"),
            Token::Where => write!(f, "WHERE"),
            Token::Table => write!(f, "TABLE"),

            Token::LeftParen => write!(f, "("),
            Token::RightParen => write!(f, ")"),
            Token::Comma => write!(f, ","),
            Token::Semicolon => write!(f, ";"),
            Token::Star => write!(f, "*"),
            Token::Equal => write!(f, "="),

            Token::Identifier(s) => write!(f, "Identifier({})", s),
            Token::String(s) => write!(f, "String({})", s),
            Token::Number(n) => write!(f, "Number({})", n),

            Token::EOF => write!(f, "EOF"),
        }
    }
}