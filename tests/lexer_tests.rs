use arch_db::sql::{ast::{BinaryOperator, Expr, Statement}, lexer::Lexer, sql_parser::Parser, token::Token};


#[test]
fn lexer_creation() {
    let lexer = Lexer::new("SELECT");

    assert_eq!(lexer.position, 0);
    assert_eq!(lexer.input.len(), 6);
}

#[test]
fn current_character() {
    let lexer = Lexer::new("ABC");

    assert_eq!(lexer.current_char(), Some('A'));
}

#[test]
fn advance_position() {
    let mut lexer = Lexer::new("ABC");

    lexer.advance();

    assert_eq!(lexer.current_char(), Some('B'));
}

#[test]
fn skip_spaces() {
    let mut lexer = Lexer::new("    SELECT");

    lexer.skip_whitespace();

    assert_eq!(lexer.current_char(), Some('S'));
}

#[test]
fn end_of_input() {
    let mut lexer = Lexer::new("A");

    lexer.advance();

    assert_eq!(lexer.current_char(), None);
}


#[test]
fn lex_single_symbol() {
    let mut lexer = Lexer::new("*");

    assert_eq!(lexer.next_token(), Token::Star);
    assert_eq!(lexer.next_token(), Token::EOF);
}

#[test]
fn lex_multiple_symbols() {
    let mut lexer = Lexer::new("*=(),;");

    assert_eq!(lexer.next_token(), Token::Star);
    assert_eq!(lexer.next_token(), Token::Equal);
    assert_eq!(lexer.next_token(), Token::LeftParen);
    assert_eq!(lexer.next_token(), Token::RightParen);
    assert_eq!(lexer.next_token(), Token::Comma);
    assert_eq!(lexer.next_token(), Token::Semicolon);
    assert_eq!(lexer.next_token(), Token::EOF);
}

#[test]
fn lex_symbols_with_spaces() {
    let mut lexer = Lexer::new("   *   =   ;");

    assert_eq!(lexer.next_token(), Token::Star);
    assert_eq!(lexer.next_token(), Token::Equal);
    assert_eq!(lexer.next_token(), Token::Semicolon);
    assert_eq!(lexer.next_token(), Token::EOF);
}

#[test]
fn lex_select_keyword() {
    let mut lexer = Lexer::new("SELECT");

    assert_eq!(lexer.next_token(), Token::Select);
    assert_eq!(lexer.next_token(), Token::EOF);
}

#[test]
fn lex_lowercase_keyword() {
    let mut lexer = Lexer::new("select");

    assert_eq!(lexer.next_token(), Token::Select);
}

#[test]
fn lex_identifier() {
    let mut lexer = Lexer::new("users");

    assert_eq!(
        lexer.next_token(),
        Token::Identifier("users".to_string())
    );
}

#[test]
fn lex_identifier_with_underscore() {
    let mut lexer = Lexer::new("user_table");

    assert_eq!(
        lexer.next_token(),
        Token::Identifier("user_table".to_string())
    );
}

#[test]
fn lex_simple_query() {
    let mut lexer =
        Lexer::new("SELECT * FROM users;");

    assert_eq!(lexer.next_token(), Token::Select);
    assert_eq!(lexer.next_token(), Token::Star);
    assert_eq!(lexer.next_token(), Token::From);

    assert_eq!(
        lexer.next_token(),
        Token::Identifier("users".to_string())
    );

    assert_eq!(lexer.next_token(), Token::Semicolon);
    assert_eq!(lexer.next_token(), Token::EOF);
}

#[test]
fn lex_number() {
    let mut lexer = Lexer::new("123");

    assert_eq!(
        lexer.next_token(),
        Token::Number(123)
    );

    assert_eq!(lexer.next_token(), Token::EOF);
}

#[test]
fn lex_string() {
    let mut lexer = Lexer::new("'jhon'");

    assert_eq!(
        lexer.next_token(),
        Token::String("jhon".to_string())
    );

    assert_eq!(lexer.next_token(), Token::EOF);
}

#[test]
fn lex_values() {
    let mut lexer = Lexer::new("(1, 'Alice')");

    assert_eq!(lexer.next_token(), Token::LeftParen);
    assert_eq!(lexer.next_token(), Token::Number(1));
    assert_eq!(lexer.next_token(), Token::Comma);

    assert_eq!(
        lexer.next_token(),
        Token::String("Alice".to_string())
    );

    assert_eq!(lexer.next_token(), Token::RightParen);
    assert_eq!(lexer.next_token(), Token::EOF);
}

#[test]
fn lex_insert_statement() {
    let mut lexer =
        Lexer::new("INSERT INTO users VALUES (1, 'jhon');");

    assert_eq!(lexer.next_token(), Token::Insert);
    assert_eq!(lexer.next_token(), Token::Into);

    assert_eq!(
        lexer.next_token(),
        Token::Identifier("users".to_string())
    );

    assert_eq!(lexer.next_token(), Token::Values);
    assert_eq!(lexer.next_token(), Token::LeftParen);
    assert_eq!(lexer.next_token(), Token::Number(1));
    assert_eq!(lexer.next_token(), Token::Comma);

    assert_eq!(
        lexer.next_token(),
        Token::String("jhon".to_string())
    );

    assert_eq!(lexer.next_token(), Token::RightParen);
    assert_eq!(lexer.next_token(), Token::Semicolon);
    assert_eq!(lexer.next_token(), Token::EOF);
}


#[test]
fn test_not_equal_token() {
    let mut lexer = Lexer::new("!=");

    assert_eq!(lexer.next_token(), Token::NotEqual);
    assert_eq!(lexer.next_token(), Token::EOF);
}

#[test]
fn test_greater_than_token() {
    let mut lexer = Lexer::new(">");

    assert_eq!(lexer.next_token(), Token::GreaterThan);
    assert_eq!(lexer.next_token(), Token::EOF);
}

#[test]
fn test_greater_than_equal_token() {
    let mut lexer = Lexer::new(">=");

    assert_eq!(lexer.next_token(), Token::GreaterThanOrEqual);
    assert_eq!(lexer.next_token(), Token::EOF);
}

#[test]
fn test_less_than_token() {
    let mut lexer = Lexer::new("<");

    assert_eq!(lexer.next_token(), Token::LessThan);
    assert_eq!(lexer.next_token(), Token::EOF);
}

#[test]
fn test_less_than_equal_token() {
    let mut lexer = Lexer::new("<=");

    assert_eq!(lexer.next_token(), Token::LessThanOrEqual);
    assert_eq!(lexer.next_token(), Token::EOF);
}

#[test]
fn test_lexer_not_equal() {
    let mut lexer = Lexer::new("!=");

    assert_eq!(lexer.next_token(), Token::NotEqual);
    assert_eq!(lexer.next_token(), Token::EOF);
}

#[test]
fn test_lexer_greater_than() {
    let mut lexer = Lexer::new(">");

    assert_eq!(lexer.next_token(), Token::GreaterThan);
    assert_eq!(lexer.next_token(), Token::EOF);
}

#[test]
fn test_lexer_greater_than_equal() {
    let mut lexer = Lexer::new(">=");

    assert_eq!(lexer.next_token(), Token::GreaterThanOrEqual);
    assert_eq!(lexer.next_token(), Token::EOF);
}

#[test]
fn test_lexer_less_than() {
    let mut lexer = Lexer::new("<");

    assert_eq!(lexer.next_token(), Token::LessThan);
    assert_eq!(lexer.next_token(), Token::EOF);
}

#[test]
fn test_lexer_less_than_equal() {
    let mut lexer = Lexer::new("<=");

    assert_eq!(lexer.next_token(), Token::LessThanOrEqual);
    assert_eq!(lexer.next_token(), Token::EOF);
}

#[test]
fn test_parse_select_where_not_equal() {
    let sql = "SELECT * FROM users WHERE id != 10;";

    let mut parser = Parser::new(Lexer::new(sql));
    let stmt = parser.parse_statement();

    match stmt {
        Statement::Select(select) => {
            match select.where_clause.unwrap() {
                Expr::Binary { left, op, right } => {
                    assert_eq!(*left, Expr::Identifier("id".into()));
                    assert_eq!(op, BinaryOperator::NotEqual);
                    assert_eq!(*right, Expr::Number(10));
                }
                _ => panic!("Expected binary expression"),
            }
        }
        _ => panic!("Expected SELECT"),
    }
}

#[test]
fn test_parse_select_where_greater_than() {
    let sql = "SELECT * FROM users WHERE age > 18;";

    let mut parser = Parser::new(Lexer::new(sql));
    let stmt = parser.parse_statement();

    match stmt {
        Statement::Select(select) => {
            match select.where_clause.unwrap() {
                Expr::Binary { op, .. } => {
                    assert_eq!(op, BinaryOperator::GreaterThan);
                }
                _ => panic!("Expected binary expression"),
            }
        }
        _ => panic!("Expected SELECT"),
    }
}

#[test]
fn test_parse_select_where_greater_than_equal() {
    let sql = "SELECT * FROM users WHERE age >= 18;";

    let mut parser = Parser::new(Lexer::new(sql));
    let stmt = parser.parse_statement();

    match stmt {
        Statement::Select(select) => {
            match select.where_clause.unwrap() {
                Expr::Binary { op, .. } => {
                    assert_eq!(op, BinaryOperator::GreaterThanOrEqual);
                }
                _ => panic!("Expected binary expression"),
            }
        }
        _ => panic!("Expected SELECT"),
    }
}

#[test]
fn test_parse_select_where_less_than() {
    let sql = "SELECT * FROM users WHERE age < 18;";

    let mut parser = Parser::new(Lexer::new(sql));
    let stmt = parser.parse_statement();

    match stmt {
        Statement::Select(select) => {
            match select.where_clause.unwrap() {
                Expr::Binary { op, .. } => {
                    assert_eq!(op, BinaryOperator::LessThan);
                }
                _ => panic!("Expected binary expression"),
            }
        }
        _ => panic!("Expected SELECT"),
    }
}

#[test]
fn test_parse_select_where_less_than_equal() {
    let sql = "SELECT * FROM users WHERE age <= 18;";

    let mut parser = Parser::new(Lexer::new(sql));
    let stmt = parser.parse_statement();

    match stmt {
        Statement::Select(select) => {
            match select.where_clause.unwrap() {
                Expr::Binary { op, .. } => {
                    assert_eq!(op, BinaryOperator::LessThanOrEqual);
                }
                _ => panic!("Expected binary expression"),
            }
        }
        _ => panic!("Expected SELECT"),
    }
}

#[test]
fn test_parse_missing_operator() {
    let sql = "SELECT * FROM users WHERE id 10;";

    let mut parser = Parser::new(Lexer::new(sql));
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        parser.parse_statement();
    })).is_err());
}

#[test]
fn test_parse_missing_rhs() {
    let sql = "SELECT * FROM users WHERE id >=;";

    let mut parser = Parser::new(Lexer::new(sql));
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        parser.parse_statement();
    })).is_err());
}

#[test]
fn test_parse_missing_lhs() {
    let sql = "SELECT * FROM users WHERE >= 10;";

    let mut parser = Parser::new(Lexer::new(sql));
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        parser.parse_statement();
    })).is_err());
}