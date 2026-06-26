use crate::sql::{
    ast::*,
    lexer::Lexer,
    token::Token,
};

pub struct Parser {
    lexer: Lexer,
    current_token: Token,
}

impl Parser {
    pub fn new(mut lexer: Lexer) -> Self {
        let current_token = lexer.next_token();

        Self {
            lexer,
            current_token,
        }
    }
    
    pub fn advance(&mut self) {
        self.current_token = self.lexer.next_token();
    }

    pub fn expect(&mut self, expected: Token) {
        if self.current_token != expected {
            panic!(
                "Expected {:?}, found {:?}",
                expected,
                self.current_token
            );
        }

        self.advance();
    }

    pub fn parse_identifier(&mut self) -> String {
        match &self.current_token {
            Token::Identifier(name) => {
                let name = name.clone();
                self.advance();
                name
            }

            _ => panic!("Expected identifier"),
        }
    }

    pub fn parse_literal(&mut self) -> Expr {
        match &self.current_token {
            Token::Number(n) => {
                let n = *n;
                self.advance();
                Expr::Number(n)
            }

            Token::String(s) => {
                let s = s.clone();
                self.advance();
                Expr::String(s)
            }

            Token::Identifier(id) => {
                let id = id.clone();
                self.advance();
                Expr::Identifier(id)
            }

            _ => panic!("Expected literal"),
        }
    }

    pub fn parse_statement(&mut self) -> Statement {
        match self.current_token {
            Token::Select => self.parse_select(),

            Token::Insert => self.parse_insert(),

            Token::Create => self.parse_create_table(),

            Token::Delete => self.parse_delete(),

            Token::Update => self.parse_update(),

            _ => panic!("Unexpected token {:?}", self.current_token),
        }
    }

    fn parse_data_type(&mut self) -> DataType {
        match &self.current_token {
            Token::Identifier(name)
                if name.eq_ignore_ascii_case("INT") =>
            {
                self.advance();
                DataType::Int
            }

            Token::Identifier(name)
                if name.eq_ignore_ascii_case("TEXT") =>
            {
                self.advance();
                DataType::Text
            }

            _ => panic!("Expected data type"),
        }
    }

    fn parse_column_definition(
        &mut self,
    ) -> ColumnDef {

        let name = self.parse_identifier();

        let data_type = self.parse_data_type();

        ColumnDef {
            name,
            data_type,
        }
    }

    fn parse_identifier_list(&mut self) -> Vec<String> {
        let mut identifiers = Vec::new();

        loop {
            identifiers.push(self.parse_identifier());

            if self.current_token == Token::Comma {
                self.advance();
                continue;
            }

            break;
        }

        identifiers
    }

    fn parse_expr_list(&mut self) -> Vec<Expr> {
        let mut exprs = Vec::new();

        loop {
            exprs.push(self.parse_literal());

            if self.current_token == Token::Comma {
                self.advance();
                continue;
            }

            break;
        }

        exprs
    }

    fn parse_create_table(
        &mut self,
    ) -> Statement {

        self.expect(Token::Create);

        self.expect(Token::Table);

        let table_name = self.parse_identifier();

        self.expect(Token::LeftParen);

        let mut columns = Vec::new();

        loop {

            columns.push(
                self.parse_column_definition()
            );

            if self.current_token == Token::Comma {
                self.advance();
                continue;
            }

            break;
        }

        self.expect(Token::RightParen);

        if self.current_token == Token::Semicolon {
            self.advance();
        }

        Statement::CreateTable(
            CreateTable {
                table_name,
                columns,
            }
        )
    }

    fn current_is(&self, token: &Token) -> bool {
        &self.current_token == token
    }

    fn parse_select(&mut self) -> Statement {
        todo!()
    }

    fn parse_insert(&mut self) -> Statement {
        self.expect(Token::Insert);

        self.expect(Token::Into);

        let table_name = self.parse_identifier();

        self.expect(Token::LeftParen);

        let columns = self.parse_identifier_list();

        self.expect(Token::RightParen);

        self.expect(Token::Values);

        self.expect(Token::LeftParen);

        let values = self.parse_expr_list();

        self.expect(Token::RightParen);

        if self.current_token == Token::Semicolon {
            self.advance();
        }

        Statement::Insert(Insert {
            table_name,
            columns,
            values,
        })
    }

    fn parse_delete(&mut self) -> Statement {
        todo!()
    }

    fn parse_update(&mut self) -> Statement {
        todo!()
    }
}