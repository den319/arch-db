use arch_db::sql::{ast::{DataType, Expr, Statement}, lexer::Lexer, parser::Parser};



#[test]
#[should_panic]
fn parser_dispatches_select() {
    let lexer = Lexer::new("SELECT * FROM users");

    let mut parser = Parser::new(lexer);

    parser.parse_statement();
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