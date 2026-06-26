use arch_db::sql::{lexer::Lexer, token::Token};


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

