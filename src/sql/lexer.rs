use crate::sql::token::Token;

pub struct Lexer {
    pub input: Vec<char>,
    pub position: usize,
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        Self {
            input: input.chars().collect(),
            position: 0,
        }
    }

    pub fn current_char(&self) -> Option<char> {
        self.input.get(self.position).copied()
    }

    pub fn advance(&mut self) {
        self.position += 1;
    }

    pub fn skip_whitespace(&mut self) {
        while let Some(ch)= self.current_char() {
            if ch.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }

    pub fn peek(&self) -> Option<char> {
        self.input.get(self.position + 1).copied()
    }

    pub fn next_token(&mut self) -> Token {
        self.skip_whitespace();

        
        match self.current_char() {
            Some(ch) if ch.is_alphabetic() || ch == '_' => {
                let identifier= self.read_identifier();
                Self::lookup_keyword(&identifier)
            }

            Some(ch) if ch.is_ascii_digit() => {
                Token::Number(self.read_number())
            }

            Some('\'') => {
                Token::String(self.read_string())
            }

            Some('*') => {
                self.advance();
                Token::Star
            }

            Some('=') => {
                self.advance();
                Token::Equal
            }

            Some('!') => {
                if self.peek() == Some('=') {
                    self.advance(); // !
                    self.advance(); // =

                    Token::NotEqual
                } else {
                    panic!("Unexpected character: !");
                }
            }

            Some('>') => {
                if self.peek() == Some('=') {
                    self.advance(); // >
                    self.advance(); // =

                    Token::GreaterThanOrEqual
                } else {
                    self.advance();
                    Token::GreaterThan
                }
            }

            Some('<') => {
                if self.peek() == Some('=') {
                    self.advance(); // <
                    self.advance(); // =

                    Token::LessThanOrEqual
                } else {
                    self.advance();
                    Token::LessThan
                }
            }

            Some('(') => {
                self.advance();
                Token::LeftParen
            }

            Some(')') => {
                self.advance();
                Token::RightParen
            }

            Some(',') => {
                self.advance();
                Token::Comma
            }

            Some(';') => {
                self.advance();
                Token::Semicolon
            }

            

            None => Token::EOF,

            Some(ch) => {
                panic!("Unexpected character: {}", ch);
            }
        }
    }

    pub fn read_identifier(&mut self) -> String {
        let start= self.position;

        while let Some(ch)= self.current_char() {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                self.advance();
            } else { 
                break;
            }
        }

        self.input[start..self.position].iter().collect()
    }

    pub fn lookup_keyword(identifier: &str) -> Token {
        match identifier.to_ascii_uppercase().as_str() {
            "SELECT" => Token::Select,
            "INSERT" => Token::Insert,
            "DELETE" => Token::Delete,
            "UPDATE" => Token::Update,
            "CREATE" => Token::Create,
            "SET" => Token::Set,

            "FROM" => Token::From,
            "WHERE" => Token::Where,
            "INTO" => Token::Into,
            "VALUES" => Token::Values,
            "TABLE" => Token::Table,
            "ORDER" => Token::Order,
            "BY" => Token::By,
            "LIMIT" => Token::Limit,
            "ASC" => Token::Asc,
            "DESC" => Token::Desc,

            "PRIMARY" => Token::Primary,
            "KEY" => Token::Key,


            _ => Token::Identifier(identifier.to_string()),
        }
    }

    pub fn read_number(&mut self) -> i64 {
        let start = self.position;

        while let Some(ch)= self.current_char() {
            if ch.is_ascii_digit() {
                self.advance();
            } else {
                break;
            }
        }

        self.input[start..self.position].iter().collect::<String>().parse().expect("Invalid number!")
    }

    pub fn read_string(&mut self) -> String {
        self.advance();

        let start= self.position;

        while let Some(ch)= self.current_char()  {
            if ch == '\'' {
                break;
            }

            self.advance();
        }

        let value:String= self.input[start..self.position].iter().collect();

        if self.current_char() == Some('\'') {
            self.advance();
        }

        value
    }


}