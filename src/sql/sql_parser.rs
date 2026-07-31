use std::println;

use crate::sql::{
    ast::*,
    lexer::Lexer,
    token::Token,
};

pub struct SQLParser {
    lexer: Lexer,
    current_token: Token,
}

impl SQLParser {
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

    pub fn parse_primary(&mut self) -> Expr {
        match &self.current_token {
            Token::LeftParen => {

                self.advance();

                let expr = self.parse_expression();

                self.expect(Token::RightParen);

                expr
            }

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
                let name = id.clone();
                self.advance();

                //------------------------------------------------------
                // Aggregate function? e.g. COUNT(*), SUM(age)
                //------------------------------------------------------

                if self.current_token == Token::LeftParen {
                    return self.parse_aggregate_expr(name);
                }

                Expr::Identifier(name)
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

        let mut primary_key= false;

        if self.current_token == Token::Primary {
            self.advance();

            self.expect(Token::Key);

            primary_key= true;
        }

        ColumnDef {
            name,
            data_type,
            primary_key,
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
            exprs.push(self.parse_primary());

            if self.current_token == Token::Comma {
                self.advance();
            } else {
                break;
            }

        }

        exprs
    }

    fn parse_aggregate_expr(
        &mut self,
        function_name: String,
    ) -> Expr {

        self.expect(Token::LeftParen);

        let function = match function_name
            .to_uppercase()
            .as_str()
        {
            "COUNT" => AggregateFunction::Count,

            "MIN" => AggregateFunction::Min,

            "MAX" => AggregateFunction::Max,

            "SUM" => AggregateFunction::Sum,

            "AVG" => AggregateFunction::Avg,

            _ => {
                panic!(
                    "Unknown aggregate '{}'",
                    function_name,
                );
            }
        };

        let argument = match function {

            AggregateFunction::Count => {

                self.expect(Token::Star);

                Expr::Wildcard
            }

            _ => {

                Expr::Identifier(
                    self.parse_identifier()
                )
            }
        };

        self.expect(Token::RightParen);

        Expr::Aggregate {
            function,
            argument: Box::new(argument),
        }
    }

    fn parse_aggregate_select_item(
        &mut self,
    ) -> SelectItem {

        let function_name = self.parse_identifier();

        //------------------------------------------------------
        // Normal column?
        //------------------------------------------------------

        if self.current_token != Token::LeftParen {

            return SelectItem::Column(function_name);
        }

        //------------------------------------------------------
        // Aggregate — delegate to the shared helper
        //------------------------------------------------------

        let expr = self.parse_aggregate_expr(function_name);

        match expr {

            Expr::Aggregate {
                function,
                argument,
            } => {

                SelectItem::Aggregate {
                    function,
                    argument: *argument,
                }
            }

            _ => unreachable!(),
        }
    }

    fn parse_select_items(&mut self) -> Vec<SelectItem> {
        let mut items = Vec::new();
        let mut wildcard_seen = false;

        loop {
            match &self.current_token {
                Token::Star => {
                    if wildcard_seen {
                        panic!("Multiple wildcards are not allowed");
                    }
                    self.advance();
                    items.push(SelectItem::Wildcard);
                    wildcard_seen = true;
                }

                Token::Identifier(_) => {

                    items.push(
                        self.parse_aggregate_select_item()
                    );
                }

                _ => panic!("Expected column name or '*'"),
            }

            if self.current_token == Token::Comma {
                self.advance();
            } else {
                break;
            }
        }

        items
    }


    fn parse_create_table(
        &mut self,
    ) -> Statement {

        self.expect(Token::Create);

        match &self.current_token {

            Token::Table => {
                self.advance();
            }

            Token::Index => {
                self.advance();
                return self.parse_create_index();
            }

            token => {
                panic!(
                    "Expected TABLE or INDEX after CREATE, found {:?}",
                    token
                );
            }
        }

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

    fn consume_optional_semicolon(&mut self) {
        if self.current_token == Token::Semicolon {
            self.advance();
        }
    }

    fn parse_assignment(&mut self) -> Assignment {
        let column = self.parse_identifier();

        self.expect(Token::Equal);

        let value = self.parse_primary();

        Assignment {
            column,
            value,
        }
    }

    fn parse_assignment_list(&mut self) -> Vec<Assignment> {
        let mut assignments = Vec::new();

        loop {
            assignments.push(
                self.parse_assignment()
            );

            if self.current_token == Token::Comma {
                self.advance();
            } else {
                break;
            }
        }

        // After parsing all assignments, verify the next token is valid
        match &self.current_token {
            Token::Where | Token::Semicolon | Token::EOF => {},
            _ => panic!("Expected comma between assignments"),
        }

        assignments
    }

    fn parse_comparison(
        &mut self,
    ) -> Expr {

        // Parenthesized sub-expression: parse the whole inner expression
        // and return without requiring a trailing comparison operator.
        if self.current_token == Token::LeftParen {
            self.advance();
            let expr = self.parse_expression();
            self.expect(Token::RightParen);
            return expr;
        }

        let left = self.parse_primary();

        let op = self.parse_binary_operator();

        let right = self.parse_primary();

        Expr::Binary {
            left: Box::new(left),
            op,
            right: Box::new(right),
        }
    }

    fn parse_and(
        &mut self,
    ) -> Expr {

        let mut left =
            self.parse_comparison();

        while self.current_token == Token::And {

            self.advance();

            let right =
                self.parse_comparison();

            left = Expr::Binary {

                left: Box::new(left),

                op: BinaryOperator::And,

                right: Box::new(right),
            };
        }

        left
    }

    fn parse_or(
        &mut self,
    ) -> Expr {

        let mut left =
            self.parse_and();

        while self.current_token == Token::Or {

            self.advance();

            let right =
                self.parse_and();

            left = Expr::Binary {

                left: Box::new(left),

                op: BinaryOperator::Or,

                right: Box::new(right),
            };
        }

        left
    }

    pub fn parse_expression(&mut self) -> Expr {

        self.parse_or()
    }

    fn parse_create_index(
        &mut self,
    ) -> Statement {

        let index_name = self.parse_identifier();

        self.expect(Token::On);

        let table_name = self.parse_identifier();

        self.expect(Token::LeftParen);

        let column_name = self.parse_identifier();

        self.expect(Token::RightParen);

        Statement::CreateIndex(CreateIndex {
            index_name,
            table_name,
            column_name,
        })
    }

    fn parse_group_by(
        &mut self,
    ) -> Vec<String> {

        let mut columns = Vec::new();

        loop {

            columns.push(
                self.parse_identifier(),
            );

            if self.current_token == Token::Comma {

                self.advance();

                continue;
            }

            break;
        }

        columns
    }

    fn parse_select(&mut self) -> Statement {
        self.expect(Token::Select);

        let distinct =
            if self.current_token == Token::Distinct {

                self.advance();

                true

            } else {

                false
            };

        let columns = self.parse_select_items();

        self.expect(Token::From);

        let table_name = self.parse_identifier();

        let where_clause =
            if self.current_token == Token::Where {
                self.advance();
                Some(self.parse_expression())
            } else {
                None
            };

        let group_by =
            if self.current_token == Token::Group {

                self.advance();

                self.expect(Token::By);

                Some(
                    self.parse_group_by(),
                )
            }
            else {

                None
            };

        let having =
            if self.current_token == Token::Having {

                self.advance();

                Some(
                    self.parse_expression(),
                )
            }
            else {

                None
            };

        let order_by = if self.current_token == Token::Order {

            self.advance();

            self.expect(Token::By);

            let column = self.parse_identifier();

            let direction = match self.current_token {

                Token::Desc => {
                    self.advance();
                    OrderDirection::Desc
                }

                Token::Asc => {
                    self.advance();
                    OrderDirection::Asc
                }

                _ => OrderDirection::Asc,
            };

            Some(OrderBy {
                column,
                direction,
            })

        } else {

            None
        };

        let limit = if self.current_token == Token::Limit {

            self.advance();

            match self.current_token.clone() {

                Token::Number(n) => {

                    self.advance();

                    Some(n as usize)
                }

                _ => panic!("Expected number after LIMIT"),
            }

        } else {

            None
        };

        // println!("distinct: {:?}", distinct);

        Statement::Select(Select {
            columns,
            table_name,
            where_clause,
            group_by,
            having,
            order_by,
            limit,
            distinct,
        })
    }

    fn parse_insert(&mut self) -> Statement {
        self.expect(Token::Insert);

        self.expect(Token::Into);

        let table_name = self.parse_identifier();

        //------------------------------------------------------
        // Column list is optional.
        //
        //   INSERT INTO users VALUES (1, 'Alice');
        //   INSERT INTO users (id, name) VALUES (1, 'Alice');
        //------------------------------------------------------

        let columns = if self.current_token == Token::LeftParen {
            self.advance();
            let cols = self.parse_identifier_list();
            self.expect(Token::RightParen);
            cols
        } else {
            // Leave empty — will be filled in from schema order
            // by the executor.
            Vec::new()
        };

        self.expect(Token::Values);

        self.expect(Token::LeftParen);

        let values = self.parse_expr_list();

        self.expect(Token::RightParen);

        if !columns.is_empty() && columns.len() != values.len() {
            panic!(
                "Column count mismatch: expected {}, got {}",
                columns.len(),
                values.len()
            );
        }

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
        self.expect(Token::Delete);

        self.expect(Token::From);

        let table_name = self.parse_identifier();

        let where_clause =
            if self.current_token == Token::Where {
                self.advance();
                Some(self.parse_expression())
            } else {
                None
            };

        self.consume_optional_semicolon();

        Statement::Delete(Delete {
            table_name,
            where_clause,
        })
    }

    fn parse_update(&mut self) -> Statement {
        self.expect(Token::Update);

        let table_name = self.parse_identifier();

        self.expect(Token::Set);

        let assignments = self.parse_assignment_list();

        let where_clause =
            if self.current_token == Token::Where {
                self.advance();
                Some(self.parse_expression())
            } else {
                None
            };

        self.consume_optional_semicolon();

        Statement::Update(Update {
            table_name,
            assignments,
            where_clause,
        })
    }

    fn parse_binary_operator(&mut self) -> BinaryOperator {
        match &self.current_token {
            Token::Equal => {
                self.advance();
                BinaryOperator::Equal
            }

            Token::NotEqual => {
                self.advance();
                BinaryOperator::NotEqual
            }

            Token::GreaterThan => {
                self.advance();
                BinaryOperator::GreaterThan
            }

            Token::GreaterThanOrEqual => {
                self.advance();
                BinaryOperator::GreaterThanOrEqual
            }

            Token::LessThan => {
                self.advance();
                BinaryOperator::LessThan
            }

            Token::LessThanOrEqual => {
                self.advance();
                BinaryOperator::LessThanOrEqual
            }

            _ => panic!(
                "Expected comparison operator, found {:?}",
                self.current_token
            ),
        }
    }
}