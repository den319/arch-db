use arch_db::sql::token::Token;


#[test]
fn identifier_token() {
    let token = Token::Identifier("users".to_string());

    assert_eq!(token, Token::Identifier("users".to_string()));
}

#[test]
fn number_token() {
    let token = Token::Number(42);

    assert_eq!(token, Token::Number(42));
}

#[test]
fn string_token() {
    let token = Token::String("hello".to_string());

    assert_eq!(token, Token::String("hello".to_string()));
}

#[test]
fn keyword_token() {
    assert_eq!(Token::Select, Token::Select);
    assert_eq!(Token::Insert, Token::Insert);
}
