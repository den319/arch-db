use arch_db::sql::{ast::{Assignment, BinaryOperator, DataType, Expr, SelectItem, Statement}, lexer::Lexer, sql_parser::Parser};



#[test]
fn parser_dispatches_select() {
    let lexer = Lexer::new("SELECT * FROM users");

    let mut parser = Parser::new(lexer);

    let stmt = parser.parse_statement();

    assert!(matches!(stmt, Statement::Select(_)));
}

#[test]
#[should_panic]
fn parser_dispatches_insert() {
    let lexer = Lexer::new(
        "INSERT INTO users VALUES ('john')"
    );

    let mut parser = Parser::new(lexer);

    parser.parse_statement();
}

#[test]
#[should_panic]
fn parser_dispatches_create() {
    let lexer = Lexer::new(
        "CREATE TABLE users"
    );

    let mut parser = Parser::new(lexer);

    parser.parse_statement();
}

#[test]
fn test_parse_create_table_single_column() {
    let lexer = Lexer::new(
        "CREATE TABLE users (id INT);"
    );

    let mut parser = Parser::new(lexer);

    let stmt = parser.parse_statement();

    match stmt {
        Statement::CreateTable(table) => {
            assert_eq!(table.table_name, "users");

            assert_eq!(table.columns.len(), 1);

            assert_eq!(table.columns[0].name, "id");
            assert_eq!(
                table.columns[0].data_type,
                DataType::Int
            );
        }

        _ => panic!("Expected CREATE TABLE"),
    }
}

#[test]
fn test_parse_create_table_multiple_columns() {
    let lexer = Lexer::new(
        "CREATE TABLE users (
            id INT,
            name TEXT,
            age INT
        );"
    );

    let mut parser = Parser::new(lexer);

    let stmt = parser.parse_statement();

    match stmt {
        Statement::CreateTable(table) => {

            assert_eq!(table.table_name, "users");

            assert_eq!(table.columns.len(), 3);

            assert_eq!(table.columns[0].name, "id");
            assert_eq!(table.columns[0].data_type, DataType::Int);

            assert_eq!(table.columns[1].name, "name");
            assert_eq!(table.columns[1].data_type, DataType::Text);

            assert_eq!(table.columns[2].name, "age");
            assert_eq!(table.columns[2].data_type, DataType::Int);
        }

        _ => panic!("Expected CREATE TABLE"),
    }
}

#[test]
fn test_parse_create_table_lowercase_keywords() {
    let lexer = Lexer::new(
        "create table users (
            id int,
            name text
        );"
    );

    let mut parser = Parser::new(lexer);

    let stmt = parser.parse_statement();

    match stmt {
        Statement::CreateTable(table) => {

            assert_eq!(table.table_name, "users");

            assert_eq!(table.columns.len(), 2);

            assert_eq!(table.columns[0].data_type, DataType::Int);
            assert_eq!(table.columns[1].data_type, DataType::Text);
        }

        _ => panic!("Expected CREATE TABLE"),
    }
}

#[test]
fn test_parse_create_table_without_semicolon() {
    let lexer = Lexer::new(
        "CREATE TABLE users (id INT)"
    );

    let mut parser = Parser::new(lexer);

    let stmt = parser.parse_statement();

    match stmt {
        Statement::CreateTable(table) => {
            assert_eq!(table.table_name, "users");
            assert_eq!(table.columns.len(), 1);
        }

        _ => panic!("Expected CREATE TABLE"),
    }
}

#[test]
#[should_panic]
fn test_parse_create_table_empty_columns() {
    let lexer = Lexer::new(
        "CREATE TABLE users ();"
    );

    let mut parser = Parser::new(lexer);

    parser.parse_statement();
}

#[test]
#[should_panic]
fn test_parse_create_table_missing_type() {
    let lexer = Lexer::new(
        "CREATE TABLE users (
            id
        );"
    );

    let mut parser = Parser::new(lexer);

    parser.parse_statement();
}

#[test]
#[should_panic]
fn test_parse_create_table_missing_right_paren() {
    let lexer = Lexer::new(
        "CREATE TABLE users (
            id INT"
    );

    let mut parser = Parser::new(lexer);

    parser.parse_statement();
}

#[test]
fn test_parse_insert_single_column() {
    let lexer = Lexer::new(
        "INSERT INTO users (id) VALUES (1);"
    );

    let mut parser = Parser::new(lexer);

    let stmt = parser.parse_statement();

    match stmt {
        Statement::Insert(insert) => {
            assert_eq!(insert.table_name, "users");

            assert_eq!(
                insert.columns,
                vec!["id"]
            );

            assert_eq!(
                insert.values,
                vec![Expr::Number(1)]
            );
        }

        _ => panic!("Expected INSERT"),
    }
}

#[test]
fn test_parse_insert_multiple_columns() {
    let lexer = Lexer::new(
        "INSERT INTO users (id, name, age)
         VALUES (1, 'John', 25);"
    );

    let mut parser = Parser::new(lexer);

    let stmt = parser.parse_statement();

    match stmt {
        Statement::Insert(insert) => {

            assert_eq!(insert.table_name, "users");

            assert_eq!(
                insert.columns,
                vec!["id", "name", "age"]
            );

            assert_eq!(
                insert.values,
                vec![
                    Expr::Number(1),
                    Expr::String("John".into()),
                    Expr::Number(25),
                ]
            );
        }

        _ => panic!("Expected INSERT"),
    }
}

#[test]
fn test_parse_insert_strings() {
    let lexer = Lexer::new(
        "INSERT INTO users (first, last)
         VALUES ('John', 'Doe');"
    );

    let mut parser = Parser::new(lexer);

    let stmt = parser.parse_statement();

    match stmt {
        Statement::Insert(insert) => {

            assert_eq!(
                insert.values,
                vec![
                    Expr::String("John".into()),
                    Expr::String("Doe".into()),
                ]
            );
        }

        _ => panic!("Expected INSERT"),
    }
}

#[test]
fn test_parse_insert_without_semicolon() {
    let lexer = Lexer::new(
        "INSERT INTO users (id)
         VALUES (5)"
    );

    let mut parser = Parser::new(lexer);

    let stmt = parser.parse_statement();

    assert!(matches!(stmt, Statement::Insert(_)));
}

#[test]
fn test_parse_insert_lowercase_keywords() {
    let lexer = Lexer::new(
        "insert into users (id)
         values (1);"
    );

    let mut parser = Parser::new(lexer);

    let stmt = parser.parse_statement();

    assert!(matches!(stmt, Statement::Insert(_)));
}

#[test]
#[should_panic]
fn test_parse_insert_missing_values_keyword() {
    let lexer = Lexer::new(
        "INSERT INTO users (id)
         (1);"
    );

    let mut parser = Parser::new(lexer);

    parser.parse_statement();
}

#[test]
#[should_panic]
fn test_parse_insert_missing_right_paren() {
    let lexer = Lexer::new(
        "INSERT INTO users (id
         VALUES (1);"
    );

    let mut parser = Parser::new(lexer);

    parser.parse_statement();
}

#[test]
#[should_panic]
fn test_parse_insert_missing_left_paren_after_values() {
    let lexer = Lexer::new(
        "INSERT INTO users (id)
         VALUES 1);"
    );

    let mut parser = Parser::new(lexer);

    parser.parse_statement();
}

#[test]
#[should_panic]
fn test_parse_insert_missing_comma_between_columns() {
    let lexer = Lexer::new(
        "INSERT INTO users (id name)
         VALUES (1, 'John');"
    );

    let mut parser = Parser::new(lexer);

    parser.parse_statement();
}

#[test]
#[should_panic]
fn test_parse_insert_missing_comma_between_values() {
    let lexer = Lexer::new(
        "INSERT INTO users (id, name)
         VALUES (1 'John');"
    );

    let mut parser = Parser::new(lexer);

    parser.parse_statement();
}

#[test]
#[should_panic]
fn test_parse_insert_column_value_count_mismatch() {
    let lexer = Lexer::new(
        "INSERT INTO users (id, name)
         VALUES (1);"
    );

    let mut parser = Parser::new(lexer);

    parser.parse_statement();
}

#[test]
fn test_parse_expression_number() {
    let lexer = Lexer::new("id = 1");

    let mut parser = Parser::new(lexer);

    let expr = parser.parse_expression();

    assert_eq!(
        expr,
        Expr::Binary {
            left: Box::new(Expr::Identifier("id".into())),
            op: BinaryOperator::Equal,
            right: Box::new(Expr::Number(1)),
        }
    );
}

#[test]
fn test_parse_expression_string() {
    let lexer = Lexer::new("name = 'John'");

    let mut parser = Parser::new(lexer);

    let expr = parser.parse_expression();

    assert_eq!(
        expr,
        Expr::Binary {
            left: Box::new(Expr::Identifier("name".into())),
            op: BinaryOperator::Equal,
            right: Box::new(Expr::String("John".into())),
        }
    );
}

#[test]
fn test_parse_expression_identifier() {
    let lexer = Lexer::new("id = user_id");

    let mut parser = Parser::new(lexer);

    let expr = parser.parse_expression();

    assert_eq!(
        expr,
        Expr::Binary {
            left: Box::new(Expr::Identifier("id".into())),
            op: BinaryOperator::Equal,
            right: Box::new(Expr::Identifier("user_id".into())),
        }
    );
}

#[test]
#[should_panic]
fn test_parse_expression_missing_operator() {
    let lexer = Lexer::new("id 1");

    let mut parser = Parser::new(lexer);

    parser.parse_expression();
}

#[test]
#[should_panic]
fn test_parse_expression_missing_rhs() {
    let lexer = Lexer::new("id =");

    let mut parser = Parser::new(lexer);

    parser.parse_expression();
}

#[test]
#[should_panic]
fn test_parse_expression_missing_lhs() {
    let lexer = Lexer::new("= 10");

    let mut parser = Parser::new(lexer);

    parser.parse_expression();
}

#[test]
fn test_parse_select_wildcard() {
    let lexer = Lexer::new(
        "SELECT * FROM users;"
    );

    let mut parser = Parser::new(lexer);

    let stmt = parser.parse_statement();

    match stmt {
        Statement::Select(select) => {
            assert_eq!(
                select.columns,
                vec![SelectItem::Wildcard]
            );

            assert_eq!(select.table_name, "users");

            assert_eq!(select.where_clause, None);
        }

        _ => panic!("Expected SELECT"),
    }
}

#[test]
fn test_parse_select_columns() {
    let lexer = Lexer::new(
        "SELECT id, name, age FROM users;"
    );

    let mut parser = Parser::new(lexer);

    let stmt = parser.parse_statement();

    match stmt {
        Statement::Select(select) => {

            assert_eq!(
                select.columns,
                vec![
                    SelectItem::Column("id".into()),
                    SelectItem::Column("name".into()),
                    SelectItem::Column("age".into()),
                ]
            );

            assert_eq!(select.table_name, "users");

            assert_eq!(select.where_clause, None);
        }

        _ => panic!("Expected SELECT"),
    }
}

#[test]
fn test_parse_select_where() {
    let lexer = Lexer::new(
        "SELECT * FROM users WHERE id = 1;"
    );

    let mut parser = Parser::new(lexer);

    let stmt = parser.parse_statement();

    match stmt {
        Statement::Select(select) => {

            assert_eq!(
                select.columns,
                vec![SelectItem::Wildcard]
            );

            assert_eq!(select.table_name, "users");

            assert_eq!(
                select.where_clause,
                Some(
                    Expr::Binary {
                        left: Box::new(
                            Expr::Identifier("id".into())
                        ),
                        op: BinaryOperator::Equal,
                        right: Box::new(
                            Expr::Number(1)
                        ),
                    }
                )
            );
        }

        _ => panic!("Expected SELECT"),
    }
}

#[test]
fn test_parse_select_where_string() {
    let lexer = Lexer::new(
        "SELECT name FROM users WHERE name = 'John';"
    );

    let mut parser = Parser::new(lexer);

    let stmt = parser.parse_statement();

    match stmt {
        Statement::Select(select) => {

            assert_eq!(
                select.where_clause,
                Some(
                    Expr::Binary {
                        left: Box::new(
                            Expr::Identifier("name".into())
                        ),
                        op: BinaryOperator::Equal,
                        right: Box::new(
                            Expr::String("John".into())
                        ),
                    }
                )
            );
        }

        _ => panic!("Expected SELECT"),
    }
}

#[test]
fn test_parse_select_lowercase() {
    let lexer = Lexer::new(
        "select * from users;"
    );

    let mut parser = Parser::new(lexer);

    let stmt = parser.parse_statement();

    assert!(matches!(stmt, Statement::Select(_)));
}

#[test]
fn test_parse_select_without_semicolon() {
    let lexer = Lexer::new(
        "SELECT * FROM users"
    );

    let mut parser = Parser::new(lexer);

    let stmt = parser.parse_statement();

    assert!(matches!(stmt, Statement::Select(_)));
}

#[test]
#[should_panic]
fn test_parse_select_missing_from() {
    let lexer = Lexer::new(
        "SELECT * users;"
    );

    let mut parser = Parser::new(lexer);

    parser.parse_statement();
}

#[test]
#[should_panic]
fn test_parse_select_missing_table() {
    let lexer = Lexer::new(
        "SELECT * FROM;"
    );

    let mut parser = Parser::new(lexer);

    parser.parse_statement();
}

#[test]
#[should_panic]
fn test_parse_select_missing_columns() {
    let lexer = Lexer::new(
        "SELECT FROM users;"
    );

    let mut parser = Parser::new(lexer);

    parser.parse_statement();
}

#[test]
#[should_panic]
fn test_parse_select_missing_where_expression() {
    let lexer = Lexer::new(
        "SELECT * FROM users WHERE;"
    );

    let mut parser = Parser::new(lexer);

    parser.parse_statement();
}

#[test]
#[should_panic]
fn test_parse_select_trailing_comma() {
    let lexer = Lexer::new(
        "SELECT id, FROM users;"
    );

    let mut parser = Parser::new(lexer);

    parser.parse_statement();
}

#[test]
#[should_panic]
fn test_parse_select_multiple_wildcards() {
    let lexer = Lexer::new(
        "SELECT *, * FROM users;"
    );

    let mut parser = Parser::new(lexer);

    parser.parse_statement();
}


#[test]
fn test_parse_delete() {
    let lexer = Lexer::new(
        "DELETE FROM users;"
    );

    let mut parser = Parser::new(lexer);

    let stmt = parser.parse_statement();

    match stmt {
        Statement::Delete(delete) => {
            assert_eq!(delete.table_name, "users");
            assert_eq!(delete.where_clause, None);
        }

        _ => panic!("Expected DELETE"),
    }
}

#[test]
fn test_parse_delete_where_string() {
    let lexer = Lexer::new(
        "DELETE FROM users WHERE name = 'John';"
    );

    let mut parser = Parser::new(lexer);

    let stmt = parser.parse_statement();

    match stmt {
        Statement::Delete(delete) => {

            assert_eq!(
                delete.where_clause,
                Some(
                    Expr::Binary {
                        left: Box::new(
                            Expr::Identifier("name".into())
                        ),
                        op: BinaryOperator::Equal,
                        right: Box::new(
                            Expr::String("John".into())
                        ),
                    }
                )
            );
        }

        _ => panic!("Expected DELETE"),
    }
}

#[test]
fn test_parse_delete_where() {
    let lexer = Lexer::new(
        "DELETE FROM users WHERE id = 1;"
    );

    let mut parser = Parser::new(lexer);

    let stmt = parser.parse_statement();

    match stmt {
        Statement::Delete(delete) => {

            assert_eq!(delete.table_name, "users");

            assert_eq!(
                delete.where_clause,
                Some(
                    Expr::Binary {
                        left: Box::new(
                            Expr::Identifier("id".into())
                        ),
                        op: BinaryOperator::Equal,
                        right: Box::new(
                            Expr::Number(1)
                        ),
                    }
                )
            );
        }

        _ => panic!("Expected DELETE"),
    }
}

#[test]
fn test_parse_delete_without_semicolon() {
    let lexer = Lexer::new(
        "DELETE FROM users"
    );

    let mut parser = Parser::new(lexer);

    assert!(matches!(
        parser.parse_statement(),
        Statement::Delete(_)
    ));
}


#[test]
fn test_parse_delete_lowercase() {
    let lexer = Lexer::new(
        "delete from users;"
    );

    let mut parser = Parser::new(lexer);

    assert!(matches!(
        parser.parse_statement(),
        Statement::Delete(_)
    ));
}

#[test]
#[should_panic]
fn test_parse_delete_missing_from() {
    let lexer = Lexer::new(
        "DELETE users;"
    );

    let mut parser = Parser::new(lexer);

    parser.parse_statement();
}

#[test]
#[should_panic]
fn test_parse_delete_missing_table() {
    let lexer = Lexer::new(
        "DELETE FROM;"
    );

    let mut parser = Parser::new(lexer);

    parser.parse_statement();
}

#[test]
#[should_panic]
fn test_parse_delete_missing_where_expression() {
    let lexer = Lexer::new(
        "DELETE FROM users WHERE;"
    );

    let mut parser = Parser::new(lexer);

    parser.parse_statement();
}

#[test]
fn test_parse_update() {
    let lexer = Lexer::new(
        "UPDATE users SET name = 'John';"
    );

    let mut parser = Parser::new(lexer);

    let stmt = parser.parse_statement();

    match stmt {
        Statement::Update(update) => {

            assert_eq!(update.table_name, "users");

            assert_eq!(
                update.assignments,
                vec![
                    Assignment {
                        column: "name".into(),
                        value: Expr::String("John".into()),
                    }
                ]
            );

            assert_eq!(update.where_clause, None);
        }

        _ => panic!("Expected UPDATE"),
    }
}

#[test]
fn test_parse_update_multiple_assignments() {
    let lexer = Lexer::new(
        "UPDATE users SET name = 'John', age = 25;"
    );

    let mut parser = Parser::new(lexer);

    let stmt = parser.parse_statement();

    match stmt {
        Statement::Update(update) => {

            assert_eq!(
                update.assignments,
                vec![
                    Assignment {
                        column: "name".into(),
                        value: Expr::String("John".into()),
                    },
                    Assignment {
                        column: "age".into(),
                        value: Expr::Number(25),
                    }
                ]
            );
        }

        _ => panic!("Expected UPDATE"),
    }
}


#[test]
fn test_parse_update_where() {
    let lexer = Lexer::new(
        "UPDATE users SET name = 'John' WHERE id = 1;"
    );

    let mut parser = Parser::new(lexer);

    let stmt = parser.parse_statement();

    match stmt {
        Statement::Update(update) => {

            assert_eq!(update.table_name, "users");

            assert_eq!(
                update.where_clause,
                Some(
                    Expr::Binary {
                        left: Box::new(
                            Expr::Identifier("id".into())
                        ),
                        op: BinaryOperator::Equal,
                        right: Box::new(
                            Expr::Number(1)
                        ),
                    }
                )
            );
        }

        _ => panic!("Expected UPDATE"),
    }
}

#[test]
fn test_parse_update_without_semicolon() {
    let lexer = Lexer::new(
        "UPDATE users SET age = 30"
    );

    let mut parser = Parser::new(lexer);

    assert!(matches!(
        parser.parse_statement(),
        Statement::Update(_)
    ));
}

#[test]
fn test_parse_update_lowercase() {
    let lexer = Lexer::new(
        "update users set age = 30;"
    );

    let mut parser = Parser::new(lexer);

    assert!(matches!(
        parser.parse_statement(),
        Statement::Update(_)
    ));
}

#[test]
#[should_panic]
fn test_parse_update_missing_set() {
    let lexer = Lexer::new(
        "UPDATE users age = 30;"
    );

    let mut parser = Parser::new(lexer);

    parser.parse_statement();
}

#[test]
#[should_panic]
fn test_parse_update_missing_value() {
    let lexer = Lexer::new(
        "UPDATE users SET age = ;"
    );

    let mut parser = Parser::new(lexer);

    parser.parse_statement();
}

#[test]
#[should_panic]
fn test_parse_update_missing_where_expression() {
    let lexer = Lexer::new(
        "UPDATE users SET age = 30 WHERE;"
    );

    let mut parser = Parser::new(lexer);

    parser.parse_statement();
}

#[test]
#[should_panic]
fn test_parse_update_trailing_comma() {
    let lexer = Lexer::new(
        "UPDATE users SET age = 30, WHERE id = 1;"
    );

    let mut parser = Parser::new(lexer);

    parser.parse_statement();
}

#[test]
fn test_parse_update_three_assignments() {
    let lexer = Lexer::new(
        "UPDATE users SET name = 'John', age = 30, city = 'London';"
    );

    let mut parser = Parser::new(lexer);

    let stmt = parser.parse_statement();

    match stmt {
        Statement::Update(update) => {
            assert_eq!(update.assignments.len(), 3);

            assert_eq!(
                update.assignments[0].column,
                "name"
            );

            assert_eq!(
                update.assignments[1].column,
                "age"
            );

            assert_eq!(
                update.assignments[2].column,
                "city"
            );
        }

        _ => panic!("Expected UPDATE"),
    }
}

#[test]
#[should_panic]
fn test_parse_update_missing_comma() {
    let lexer = Lexer::new(
        "UPDATE users SET age = 30 name = 'John';"
    );

    let mut parser = Parser::new(lexer);

    parser.parse_statement();
}

#[test]
#[should_panic]
fn test_parse_update_missing_table_name() {
    let lexer = Lexer::new(
        "UPDATE SET age = 30;"
    );

    let mut parser = Parser::new(lexer);

    parser.parse_statement();
}