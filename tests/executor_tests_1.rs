use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use arch_db::sql::ast::{CreateIndex, OrderBy, OrderDirection};
use arch_db::sql::catalog::IndexSchema;
use arch_db::sql::lexer::Lexer;
use arch_db::sql::sql_parser::SQLParser;
use arch_db::{
    engine::{Engine, Value},
    sql::{
        ast::{
            self, Assignment, BinaryOperator, ColumnDef, CreateTable, DataType, Delete, Expr,
            Insert, Select, SelectItem, Statement, Update,
        },
        catalog::{self as catalog_mod, Catalog, CatalogDataType, Column, TableSchema},
        executor::{Executor, QueryResult},
        row::{Row, RowValue},
    },
};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

fn make_engine() -> Engine {
    let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = format!("storage/tests/test_executor_{}", id);
    let _ = fs::remove_dir_all(&path);
    Engine::with_storage_path(&path)
}

fn create_executor() -> Executor<'static> {
    let catalog = Catalog::new();
    let engine = make_engine();
    Executor::new(Box::leak(Box::new(catalog)), Box::leak(Box::new(engine)))
}

fn execute_sql(executor: &mut Executor, sql: &str) -> QueryResult {
    let lexer = Lexer::new(sql);
    let mut parser = SQLParser::new(lexer);
    let statement = parser.parse_statement();
    executor.execute(statement)
}

fn assert_rows_eq(result: QueryResult, expected: Vec<Vec<&str>>) {
    match result {
        QueryResult::Rows(rows) => {
            let expected: Vec<Vec<String>> = expected
                .into_iter()
                .map(|row| row.into_iter().map(|s| s.to_string()).collect())
                .collect();
            assert_eq!(rows, expected);
        }
        other => panic!("Expected Rows, got {:?}", other),
    }
}

fn extract_rows(result: &QueryResult) -> &Vec<Vec<String>> {
    match result {
        QueryResult::Rows(rows) => rows,
        other => panic!("Expected Rows, got {:?}", other),
    }
}

fn scan_index_keys(engine: &mut Engine) -> Vec<String> {
    engine
        .scan("__index__", "__index__~")
        .into_iter()
        .map(|(k, _)| k)
        .collect()
}

fn create_users_table(executor: &mut Executor) {
    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
            ColumnDef {
                name: "age".into(),
                data_type: DataType::Int,
                primary_key: false,
            },
        ],
    }));
}

fn insert_user(executor: &mut Executor, id: i64, name: &str, age: i64) {
    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec!["id".into(), "name".into(), "age".into()],
        values: vec![
            Expr::Number(id),
            Expr::String(name.into()),
            Expr::Number(age),
        ],
    }));
}


#[test]
fn test_execute_create_table() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(&mut catalog, &mut engine);

    let stmt = Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: ast::DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: ast::DataType::Text,
                primary_key: false,
            },
        ],
    });

    executor.execute(stmt);

    assert!(catalog.exists("users"));

    let table = catalog.table("users").unwrap();

    assert_eq!(table.columns.len(), 2);

    assert_eq!(table.columns[0].name, "id");

    assert_eq!(table.columns[1].name, "name");
}

#[test]
fn test_execute_create_existing_table() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(&mut catalog, &mut engine);

    let stmt = Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![],
    });

    let result = executor.execute(stmt);

    // First execution should succeed
    assert_eq!(
        result,
        QueryResult::Message("Error: table must contain exactly one PRIMARY KEY".into())
    );

    // Second execution with same table name should fail
    let stmt2 = Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![],
    });

    let result2 = executor.execute(stmt2);

    assert!(matches!(result2, QueryResult::Message(msg) if msg.starts_with("Error:")));
}

#[test]
fn test_execute_dispatch_create_table() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![],
    }));
}

#[test]
fn test_insert_unknown_table() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(&mut catalog, &mut engine);

    let stmt = Insert {
        table_name: "users".into(),
        columns: vec!["id".into(), "name".into()],
        values: vec![Expr::Number(1), Expr::String("Alice".into())],
    };

    let result = executor.execute_insert(stmt);

    assert!(result.is_err());
}

#[test]
fn test_register_table() {
    let mut catalog = Catalog::new();

    let schema = TableSchema {
        name: "users".into(),
        columns: vec![
            Column {
                name: "id".into(),
                data_type: CatalogDataType::Integer,
                primary_key: true,
                nullable: false,
            },
            Column {
                name: "name".into(),
                data_type: CatalogDataType::Text,
                primary_key: false,
                nullable: false,
            },
        ],
    };

    catalog.create_table(schema).unwrap();
}

#[test]
fn test_insert_in_table() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(&mut catalog, &mut engine);

    // First register the table
    let create_stmt = Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: ast::DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: ast::DataType::Text,
                primary_key: false,
            },
        ],
    });
    executor.execute(create_stmt);

    let stmt = Insert {
        table_name: "users".into(),
        columns: vec!["id".into(), "name".into()],
        values: vec![Expr::Number(1), Expr::String("Alice".into())],
    };

    let result = executor.execute_insert(stmt);

    assert!(result.is_ok());
}

#[test]
fn test_select_by_integer_primary_key() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
        ],
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec!["id".into(), "name".into()],
        values: vec![Expr::Number(1), Expr::String("Alice".into())],
    }));

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        table_name: "users".into(),
        limit: None,
        order_by: None,
        group_by: None,
        distinct: false,
        having: None,

        where_clause: Some(Expr::Binary {
            left: Box::new(Expr::Identifier("id".into())),
            op: BinaryOperator::Equal,
            right: Box::new(Expr::Number(1)),
        }),
    }));

    assert_eq!(
        result,
        QueryResult::Rows(vec![vec!["1".into(), "Alice".into(),]])
    );
}

#[test]
fn test_select_by_text_primary_key() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "username".into(),
                data_type: DataType::Text,
                primary_key: true,
            },
            ColumnDef {
                name: "age".into(),
                data_type: DataType::Int,
                primary_key: false,
            },
        ],
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec!["username".into(), "age".into()],
        values: vec![Expr::String("alice".into()), Expr::Number(25)],
    }));

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        table_name: "users".into(),
        limit: None,
        order_by: None,
        group_by: None,
        distinct: false,
        having: None,

        where_clause: Some(Expr::Binary {
            left: Box::new(Expr::Identifier("username".into())),
            op: BinaryOperator::Equal,
            right: Box::new(Expr::String("alice".into())),
        }),
    }));

    assert_eq!(
        result,
        QueryResult::Rows(vec![vec!["alice".into(), "25".into(),]])
    );
}

#[test]
fn test_select_missing_table() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(&mut catalog, &mut engine);

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        table_name: "users".into(),
        limit: None,
        order_by: None,
        group_by: None,
        distinct: false,
        having: None,

        where_clause: Some(Expr::Binary {
            left: Box::new(Expr::Identifier("id".into())),
            op: BinaryOperator::Equal,
            right: Box::new(Expr::Number(1)),
        }),
    }));
    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        table_name: "users".into(),
        limit: None,
        group_by: None,
        distinct: false,
        having: None,

        order_by: None,
        where_clause: Some(Expr::Binary {
            left: Box::new(Expr::Identifier("id".into())),
            op: BinaryOperator::Equal,
            right: Box::new(Expr::Number(1)),
        }),
    }));

    assert_eq!(
        result,
        QueryResult::Message("Error: table 'users' does not exist".into())
    );
}

#[test]
fn test_select_missing_row() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![ColumnDef {
            name: "id".into(),
            data_type: DataType::Int,
            primary_key: true,
        }],
    }));

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        table_name: "users".into(),
        limit: None,
        order_by: None,
        group_by: None,
        distinct: false,
        having: None,

        where_clause: Some(Expr::Binary {
            left: Box::new(Expr::Identifier("id".into())),
            op: BinaryOperator::Equal,
            right: Box::new(Expr::Number(1)),
        }),
    }));

    assert_eq!(result, QueryResult::Rows(vec![]));
}

#[test]
fn test_select_without_where_clause() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![ColumnDef {
            name: "id".into(),
            data_type: DataType::Int,
            primary_key: true,
        }],
    }));

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        table_name: "users".into(),
        where_clause: None,
        group_by: None,
        distinct: false,
        having: None,
        limit: None,
        order_by: None,
    }));

    assert_eq!(result, QueryResult::Rows(vec![]));
}

#[test]
fn test_select_with_invalid_where_clause() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![ColumnDef {
            name: "id".into(),
            data_type: DataType::Int,
            primary_key: true,
        }],
    }));

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        table_name: "users".into(),
        limit: None,
        group_by: None,
        distinct: false,
        having: None,

        order_by: None,
        where_clause: Some(Expr::Identifier("id".into())),
    }));

    assert_eq!(result, QueryResult::Rows(vec![]));
}

#[test]
fn test_delete_existing_row() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();
    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
        ],
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec!["id".into(), "name".into()],
        values: vec![Expr::Number(1), Expr::String("Alice".into())],
    }));

    let result = executor.execute(Statement::Delete(Delete {
        table_name: "users".into(),
        where_clause: Some(Expr::Binary {
            left: Box::new(Expr::Identifier("id".into())),
            op: BinaryOperator::Equal,
            right: Box::new(Expr::Number(1)),
        }),
    }));

    assert_eq!(result, QueryResult::Message("1 row deleted".into()));

    match executor.engine.get("users:1") {
        Some(Value::Tombstone) => {}
        other => panic!("Expected tombstone, got {:?}", other),
    }
}

#[test]
fn test_delete_from_missing_table() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();
    let mut executor = Executor::new(&mut catalog, &mut engine);

    let result = executor.execute(Statement::Delete(Delete {
        table_name: "users".into(),
        where_clause: Some(Expr::Binary {
            left: Box::new(Expr::Identifier("id".into())),
            op: BinaryOperator::Equal,
            right: Box::new(Expr::Number(1)),
        }),
    }));

    assert_eq!(
        result,
        QueryResult::Message("Error: table 'users' does not exist".into())
    );
}

#[test]
fn test_delete_without_where_clause() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();
    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
        ],
    }));

    let result = executor.execute(Statement::Delete(Delete {
        table_name: "users".into(),
        where_clause: None,
    }));

    assert_eq!(
        result,
        QueryResult::Message("Error: DELETE without WHERE is not supported".into())
    );
}

#[test]
fn test_delete_requires_primary_key() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();
    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
        ],
    }));

    let result = executor.execute(Statement::Delete(Delete {
        table_name: "users".into(),
        where_clause: Some(Expr::Binary {
            left: Box::new(Expr::Identifier("name".into())),
            op: BinaryOperator::Equal,
            right: Box::new(Expr::String("Alice".into())),
        }),
    }));

    assert_eq!(result, QueryResult::Message("0 rows deleted".into()));
}

#[test]
fn test_delete_non_existing_row() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();
    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
        ],
    }));

    let result = executor.execute(Statement::Delete(Delete {
        table_name: "users".into(),
        where_clause: Some(Expr::Binary {
            left: Box::new(Expr::Identifier("id".into())),
            op: BinaryOperator::Equal,
            right: Box::new(Expr::Number(99)),
        }),
    }));

    assert_eq!(result, QueryResult::Message("0 rows deleted".into()));
}

#[test]
fn test_update_existing_row() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
        ],
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec!["id".into(), "name".into()],
        values: vec![Expr::Number(1), Expr::String("Alice".into())],
    }));

    let result = executor.execute(Statement::Update(Update {
        table_name: "users".into(),
        assignments: vec![Assignment {
            column: "name".into(),
            value: Expr::String("Bob".into()),
        }],
        where_clause: Some(Expr::Binary {
            left: Box::new(Expr::Identifier("id".into())),
            op: BinaryOperator::Equal,
            right: Box::new(Expr::Number(1)),
        }),
    }));

    assert_eq!(result, QueryResult::Message("1 row updated".into()));

    let key = "users:1";

    let stored = executor.engine.get(key).unwrap();

    match stored {
        Value::Data(data) => {
            let row = Row::deserialize(&data);

            assert_eq!(row.get("name"), Some(&RowValue::Text("Bob".into())));

            assert_eq!(row.get("id"), Some(&RowValue::Integer(1)));
        }

        _ => panic!("expected row"),
    }
}

#[test]
fn test_update_non_existing_row() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
        ],
    }));

    let result = executor.execute(Statement::Update(Update {
        table_name: "users".into(),
        assignments: vec![Assignment {
            column: "name".into(),
            value: Expr::String("Bob".into()),
        }],
        where_clause: Some(Expr::Binary {
            left: Box::new(Expr::Identifier("id".into())),
            op: BinaryOperator::Equal,
            right: Box::new(Expr::Number(99)),
        }),
    }));

    assert_eq!(result, QueryResult::Message("0 rows updated".into()));
}

#[test]
fn test_update_multiple_columns() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
            ColumnDef {
                name: "city".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
        ],
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec!["id".into(), "name".into(), "city".into()],
        values: vec![
            Expr::Number(1),
            Expr::String("Alice".into()),
            Expr::String("London".into()),
        ],
    }));

    let result = executor.execute(Statement::Update(Update {
        table_name: "users".into(),
        assignments: vec![
            Assignment {
                column: "name".into(),
                value: Expr::String("Bob".into()),
            },
            Assignment {
                column: "city".into(),
                value: Expr::String("Paris".into()),
            },
        ],
        where_clause: Some(Expr::Binary {
            left: Box::new(Expr::Identifier("id".into())),
            op: BinaryOperator::Equal,
            right: Box::new(Expr::Number(1)),
        }),
    }));

    assert_eq!(result, QueryResult::Message("1 row updated".into()));

    let stored = executor.engine.get("users:1").unwrap();

    match stored {
        Value::Data(data) => {
            let row = Row::deserialize(&data);

            assert_eq!(row.get("name"), Some(&RowValue::Text("Bob".into())));

            assert_eq!(row.get("city"), Some(&RowValue::Text("Paris".into())));
        }

        _ => panic!("expected row"),
    }
}

#[test]
fn test_update_does_not_change_primary_key() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
        ],
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec!["id".into(), "name".into()],
        values: vec![Expr::Number(1), Expr::String("Alice".into())],
    }));

    let result = executor.execute(Statement::Update(Update {
        table_name: "users".into(),
        assignments: vec![Assignment {
            column: "id".into(),
            value: Expr::Number(2),
        }],
        where_clause: Some(Expr::Binary {
            left: Box::new(Expr::Identifier("id".into())),
            op: BinaryOperator::Equal,
            right: Box::new(Expr::Number(1)),
        }),
    }));

    match result {
        QueryResult::Message(_) => {}
        _ => panic!("unexpected result"),
    }
}

#[test]
fn test_select_all_rows() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();
    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
        ],
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec!["id".into(), "name".into()],
        values: vec![Expr::Number(1), Expr::String("Alice".into())],
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec!["id".into(), "name".into()],
        values: vec![Expr::Number(2), Expr::String("Bob".into())],
    }));

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        table_name: "users".into(),
        where_clause: None,
        group_by: None,
        distinct: false,
        having: None,
        limit: None,
        order_by: None,
    }));

    assert_eq!(
        result,
        QueryResult::Rows(vec![
            vec!["1".into(), "Alice".into()],
            vec!["2".into(), "Bob".into()],
        ])
    );
}

#[test]
fn test_select_all_empty_table() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();
    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
        ],
    }));

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        table_name: "users".into(),
        where_clause: None,
        group_by: None,
        distinct: false,
        having: None,
        limit: None,
        order_by: None,
    }));

    assert_eq!(result, QueryResult::Rows(vec![]));
}

#[test]
fn test_select_all_single_row() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();
    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
        ],
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec!["id".into(), "name".into()],
        values: vec![Expr::Number(1), Expr::String("Alice".into())],
    }));

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        table_name: "users".into(),
        where_clause: None,
        group_by: None,
        distinct: false,
        having: None,
        limit: None,
        order_by: None,
    }));

    assert_eq!(
        result,
        QueryResult::Rows(vec![vec!["1".into(), "Alice".into()],])
    );
}

#[test]
fn test_select_all_multiple_rows() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();
    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
        ],
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec!["id".into(), "name".into()],
        values: vec![Expr::Number(1), Expr::String("Alice".into())],
    }));
    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec!["id".into(), "name".into()],
        values: vec![Expr::Number(2), Expr::String("Bob".into())],
    }));
    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec!["id".into(), "name".into()],
        values: vec![Expr::Number(3), Expr::String("Charlie".into())],
    }));

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        table_name: "users".into(),
        where_clause: None,
        group_by: None,
        distinct: false,
        having: None,
        limit: None,
        order_by: None,
    }));

    assert_eq!(
        result,
        QueryResult::Rows(vec![
            vec!["1".into(), "Alice".into()],
            vec!["2".into(), "Bob".into()],
            vec!["3".into(), "Charlie".into()],
        ])
    );
}

#[test]
fn test_select_all_skips_deleted_rows() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();
    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
        ],
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec!["id".into(), "name".into()],
        values: vec![Expr::Number(1), Expr::String("Alice".into())],
    }));
    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec!["id".into(), "name".into()],
        values: vec![Expr::Number(2), Expr::String("Bob".into())],
    }));

    executor.execute(Statement::Delete(Delete {
        table_name: "users".into(),
        where_clause: Some(Expr::Binary {
            left: Box::new(Expr::Identifier("id".into())),
            op: BinaryOperator::Equal,
            right: Box::new(Expr::Number(1)),
        }),
    }));

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        table_name: "users".into(),
        where_clause: None,
        group_by: None,
        distinct: false,
        having: None,
        limit: None,
        order_by: None,
    }));

    assert_eq!(
        result,
        QueryResult::Rows(vec![vec!["2".into(), "Bob".into()],])
    );
}

#[test]
fn test_select_all_after_update() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();
    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
        ],
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec!["id".into(), "name".into()],
        values: vec![Expr::Number(1), Expr::String("Alice".into())],
    }));

    executor.execute(Statement::Update(Update {
        table_name: "users".into(),
        assignments: vec![Assignment {
            column: "name".into(),
            value: Expr::String("Alice Smith".into()),
        }],
        where_clause: Some(Expr::Binary {
            left: Box::new(Expr::Identifier("id".into())),
            op: BinaryOperator::Equal,
            right: Box::new(Expr::Number(1)),
        }),
    }));

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        table_name: "users".into(),
        where_clause: None,
        group_by: None,
        distinct: false,
        having: None,
        limit: None,
        order_by: None,
    }));

    assert_eq!(
        result,
        QueryResult::Rows(vec![vec!["1".into(), "Alice Smith".into()],])
    );
}

#[test]
fn test_select_all_after_multiple_updates() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();
    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
        ],
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec!["id".into(), "name".into()],
        values: vec![Expr::Number(1), Expr::String("Alice".into())],
    }));

    executor.execute(Statement::Update(Update {
        table_name: "users".into(),
        assignments: vec![Assignment {
            column: "name".into(),
            value: Expr::String("Bob".into()),
        }],
        where_clause: Some(Expr::Binary {
            left: Box::new(Expr::Identifier("id".into())),
            op: BinaryOperator::Equal,
            right: Box::new(Expr::Number(1)),
        }),
    }));

    executor.execute(Statement::Update(Update {
        table_name: "users".into(),
        assignments: vec![Assignment {
            column: "name".into(),
            value: Expr::String("Charlie".into()),
        }],
        where_clause: Some(Expr::Binary {
            left: Box::new(Expr::Identifier("id".into())),
            op: BinaryOperator::Equal,
            right: Box::new(Expr::Number(1)),
        }),
    }));

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        table_name: "users".into(),
        where_clause: None,
        group_by: None,
        distinct: false,
        having: None,
        limit: None,
        order_by: None,
    }));

    assert_eq!(
        result,
        QueryResult::Rows(vec![vec!["1".into(), "Charlie".into()],])
    );
}

#[test]
fn test_select_all_after_flush() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();
    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
        ],
    }));

    // Insert enough rows to trigger a flush.
    for i in 0..50 {
        executor.execute(Statement::Insert(Insert {
            table_name: "users".into(),
            columns: vec!["id".into(), "name".into()],
            values: vec![Expr::Number(i), Expr::String(format!("User{}", i))],
        }));
    }

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        table_name: "users".into(),
        where_clause: None,
        group_by: None,
        distinct: false,
        having: None,
        limit: None,
        order_by: None,
    }));

    match result {
        QueryResult::Rows(rows) => {
            assert_eq!(rows.len(), 50);
        }
        _ => panic!("Expected rows"),
    }
}

#[test]
fn test_select_all_only_requested_table() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();
    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
        ],
    }));

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "products".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
        ],
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec!["id".into(), "name".into()],
        values: vec![Expr::Number(1), Expr::String("Alice".into())],
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "products".into(),
        columns: vec!["id".into(), "name".into()],
        values: vec![Expr::Number(1), Expr::String("Laptop".into())],
    }));

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        table_name: "users".into(),
        where_clause: None,
        group_by: None,
        distinct: false,
        having: None,
        limit: None,
        order_by: None,
    }));

    assert_eq!(
        result,
        QueryResult::Rows(vec![vec!["1".into(), "Alice".into()]])
    );
}

#[test]
fn test_select_all_from_memtable_and_sstable() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();
    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
        ],
    }));

    // Enough inserts to flush the memtable.
    for i in 0..50 {
        executor.execute(Statement::Insert(Insert {
            table_name: "users".into(),
            columns: vec!["id".into(), "name".into()],
            values: vec![Expr::Number(i), Expr::String(format!("User{}", i))],
        }));
    }

    // These rows should remain only in the memtable.
    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec!["id".into(), "name".into()],
        values: vec![Expr::Number(100), Expr::String("Extra1".into())],
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec!["id".into(), "name".into()],
        values: vec![Expr::Number(101), Expr::String("Extra2".into())],
    }));

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        table_name: "users".into(),
        where_clause: None,
        group_by: None,
        distinct: false,
        having: None,
        limit: None,
        order_by: None,
    }));

    match result {
        QueryResult::Rows(rows) => {
            assert_eq!(rows.len(), 52);
        }
        _ => panic!("Expected rows"),
    }
}

#[test]
fn test_select_specific_column_by_primary_key() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();
    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
        ],
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec!["id".into(), "name".into()],
        values: vec![Expr::Number(1), Expr::String("Alice".into())],
    }));

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Column("name".into())],
        table_name: "users".into(),
        limit: None,
        group_by: None,
        distinct: false,
        having: None,

        order_by: None,
        where_clause: Some(Expr::Binary {
            left: Box::new(Expr::Identifier("id".into())),
            op: BinaryOperator::Equal,
            right: Box::new(Expr::Number(1)),
        }),
    }));

    match result {
        QueryResult::Rows(rows) => {
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0], vec!["Alice"]);
        }

        _ => panic!("Expected rows"),
    }
}

#[test]
fn test_select_all_rows_without_where() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();
    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
        ],
    }));

    for i in 1..=5 {
        executor.execute(Statement::Insert(Insert {
            table_name: "users".into(),

            columns: vec!["id".into(), "name".into()],

            values: vec![Expr::Number(i), Expr::String(format!("User{}", i))],
        }));
    }

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],

        table_name: "users".into(),

        where_clause: None,
        group_by: None,
        distinct: false,
        having: None,
        limit: None,
        order_by: None,
    }));

    match result {
        QueryResult::Rows(rows) => {
            assert_eq!(rows.len(), 5);

            assert_eq!(rows[0], vec!["1", "User1"]);

            assert_eq!(rows[4], vec!["5", "User5"]);
        }

        _ => panic!("Expected rows"),
    }
}

#[test]
fn test_select_with_non_primary_key_where() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();
    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),

        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
        ],
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),

        columns: vec!["id".into(), "name".into()],

        values: vec![Expr::Number(1), Expr::String("Alice".into())],
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),

        columns: vec!["id".into(), "name".into()],

        values: vec![Expr::Number(2), Expr::String("Bob".into())],
    }));

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        limit: None,
        order_by: None,
        group_by: None,
        distinct: false,
        having: None,

        table_name: "users".into(),

        where_clause: Some(Expr::Binary {
            left: Box::new(Expr::Identifier("name".into())),

            op: BinaryOperator::Equal,

            right: Box::new(Expr::String("Bob".into())),
        }),
    }));

    match result {
        QueryResult::Rows(rows) => {
            assert_eq!(rows.len(), 1);

            assert_eq!(rows[0], vec!["2", "Bob"]);
        }

        _ => panic!("Expected rows"),
    }
}

#[test]
fn test_select_after_flush() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();
    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),

        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
        ],
    }));

    for i in 0..50 {
        executor.execute(Statement::Insert(Insert {
            table_name: "users".into(),

            columns: vec!["id".into(), "name".into()],

            values: vec![Expr::Number(i), Expr::String(format!("User{}", i))],
        }));
    }

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],

        table_name: "users".into(),

        where_clause: None,
        group_by: None,
        distinct: false,
        having: None,
        limit: None,
        order_by: None,
    }));

    match result {
        QueryResult::Rows(rows) => {
            assert_eq!(rows.len(), 50);
        }

        _ => panic!("Expected rows"),
    }
}

#[test]
fn test_delete_by_primary_key() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();
    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
        ],
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec!["id".into(), "name".into()],
        values: vec![Expr::Number(1), Expr::String("Alice".into())],
    }));

    executor.execute(Statement::Delete(Delete {
        table_name: "users".into(),
        where_clause: Some(Expr::Binary {
            left: Box::new(Expr::Identifier("id".into())),
            op: BinaryOperator::Equal,
            right: Box::new(Expr::Number(1)),
        }),
    }));

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        table_name: "users".into(),
        where_clause: None,
        group_by: None,
        distinct: false,
        having: None,
        limit: None,
        order_by: None,
    }));

    match result {
        QueryResult::Rows(rows) => {
            assert_eq!(rows.len(), 0);
        }

        _ => panic!("Expected rows"),
    }
}

#[test]
fn test_delete_with_non_primary_key_where() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();
    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
        ],
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec!["id".into(), "name".into()],
        values: vec![Expr::Number(1), Expr::String("Alice".into())],
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec!["id".into(), "name".into()],
        values: vec![Expr::Number(2), Expr::String("Bob".into())],
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec!["id".into(), "name".into()],
        values: vec![Expr::Number(3), Expr::String("Bob".into())],
    }));

    executor.execute(Statement::Delete(Delete {
        table_name: "users".into(),
        where_clause: Some(Expr::Binary {
            left: Box::new(Expr::Identifier("name".into())),
            op: BinaryOperator::Equal,
            right: Box::new(Expr::String("Bob".into())),
        }),
    }));

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        table_name: "users".into(),
        where_clause: None,
        group_by: None,
        distinct: false,
        having: None,
        limit: None,
        order_by: None,
    }));

    match result {
        QueryResult::Rows(rows) => {
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0], vec!["1", "Alice"]);
        }

        _ => panic!("Expected rows"),
    }
}

#[test]
fn test_delete_multiple_rows() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();
    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "age".into(),
                data_type: DataType::Int,
                primary_key: false,
            },
        ],
    }));

    for i in 1..=5 {
        executor.execute(Statement::Insert(Insert {
            table_name: "users".into(),
            columns: vec!["id".into(), "age".into()],
            values: vec![Expr::Number(i), Expr::Number(i * 10)],
        }));
    }

    executor.execute(Statement::Delete(Delete {
        table_name: "users".into(),
        where_clause: Some(Expr::Binary {
            left: Box::new(Expr::Identifier("age".into())),
            op: BinaryOperator::GreaterThan,
            right: Box::new(Expr::Number(20)),
        }),
    }));

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        table_name: "users".into(),
        where_clause: None,
        group_by: None,
        distinct: false,
        having: None,
        limit: None,
        order_by: None,
    }));

    match result {
        QueryResult::Rows(rows) => {
            assert_eq!(rows.len(), 2);

            assert_eq!(rows[0], vec!["1", "10"]);
            assert_eq!(rows[1], vec!["2", "20"]);
        }

        _ => panic!("Expected rows"),
    }
}

#[test]
fn test_delete_no_matching_rows() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();
    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![ColumnDef {
            name: "id".into(),
            data_type: DataType::Int,
            primary_key: true,
        }],
    }));

    for i in 1..=3 {
        executor.execute(Statement::Insert(Insert {
            table_name: "users".into(),
            columns: vec!["id".into()],
            values: vec![Expr::Number(i)],
        }));
    }

    executor.execute(Statement::Delete(Delete {
        table_name: "users".into(),
        where_clause: Some(Expr::Binary {
            left: Box::new(Expr::Identifier("id".into())),
            op: BinaryOperator::GreaterThan,
            right: Box::new(Expr::Number(100)),
        }),
    }));

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        table_name: "users".into(),
        where_clause: None,
        group_by: None,
        distinct: false,
        having: None,
        limit: None,
        order_by: None,
    }));

    match result {
        QueryResult::Rows(rows) => {
            assert_eq!(rows.len(), 3);
        }

        _ => panic!("Expected rows"),
    }
}

#[test]
fn test_delete_after_flush() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();
    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
        ],
    }));

    // Insert enough rows to trigger a flush.
    for i in 0..50 {
        executor.execute(Statement::Insert(Insert {
            table_name: "users".into(),
            columns: vec!["id".into(), "name".into()],
            values: vec![Expr::Number(i), Expr::String(format!("User{}", i))],
        }));
    }

    executor.execute(Statement::Delete(Delete {
        table_name: "users".into(),
        where_clause: Some(Expr::Binary {
            left: Box::new(Expr::Identifier("name".into())),
            op: BinaryOperator::Equal,
            right: Box::new(Expr::String("User25".into())),
        }),
    }));

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        table_name: "users".into(),
        where_clause: None,
        group_by: None,
        distinct: false,
        having: None,
        limit: None,
        order_by: None,
    }));

    match result {
        QueryResult::Rows(rows) => {
            assert_eq!(rows.len(), 49);

            assert!(!rows.iter().any(|r| r[1] == "User25"));
        }

        _ => panic!("Expected rows"),
    }
}

#[test]
fn test_delete_from_memtable_and_sstable() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();
    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
        ],
    }));

    // These rows should flush.
    for i in 1..=40 {
        executor.execute(Statement::Insert(Insert {
            table_name: "users".into(),
            columns: vec!["id".into(), "name".into()],
            values: vec![Expr::Number(i), Expr::String(format!("User{}", i))],
        }));
    }

    // These remain in the memtable.
    for i in 41..=45 {
        executor.execute(Statement::Insert(Insert {
            table_name: "users".into(),
            columns: vec!["id".into(), "name".into()],
            values: vec![Expr::Number(i), Expr::String(format!("User{}", i))],
        }));
    }

    executor.execute(Statement::Delete(Delete {
        table_name: "users".into(),
        where_clause: Some(Expr::Binary {
            left: Box::new(Expr::Identifier("id".into())),
            op: BinaryOperator::GreaterThan,
            right: Box::new(Expr::Number(35)),
        }),
    }));

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        table_name: "users".into(),
        where_clause: None,
        group_by: None,
        distinct: false,
        having: None,
        limit: None,
        order_by: None,
    }));

    match result {
        QueryResult::Rows(rows) => {
            assert_eq!(rows.len(), 35);

            for row in rows {
                let id: i64 = row[0].parse().unwrap();
                assert!(id <= 35);
            }
        }

        _ => panic!("Expected rows"),
    }
}

#[test]
fn test_delete_comparison_operators() {
    let operators = vec![
        (BinaryOperator::GreaterThan, 3, 3, vec![1, 2, 3]),
        (BinaryOperator::GreaterThanOrEqual, 3, 2, vec![1, 2]),
        (BinaryOperator::LessThan, 3, 3, vec![3, 4, 5]),
        (BinaryOperator::LessThanOrEqual, 3, 2, vec![4, 5]),
        (BinaryOperator::NotEqual, 3, 1, vec![3]),
        (BinaryOperator::Equal, 3, 4, vec![1, 2, 4, 5]),
    ];

    for (op, value, expected_remaining, expected_ids) in operators {
        let mut catalog = Catalog::new();
        let mut engine = make_engine();
        let mut executor = Executor::new(&mut catalog, &mut engine);

        executor.execute(Statement::CreateTable(CreateTable {
            table_name: "users".into(),
            columns: vec![ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            }],
        }));

        for i in 1..=5 {
            executor.execute(Statement::Insert(Insert {
                table_name: "users".into(),
                columns: vec!["id".into()],
                values: vec![Expr::Number(i)],
            }));
        }

        executor.execute(Statement::Delete(Delete {
            table_name: "users".into(),
            where_clause: Some(Expr::Binary {
                left: Box::new(Expr::Identifier("id".into())),
                op,
                right: Box::new(Expr::Number(value)),
            }),
        }));

        let result = executor.execute(Statement::Select(Select {
            columns: vec![SelectItem::Wildcard],
            table_name: "users".into(),
            where_clause: None,
            group_by: None,
            distinct: false,
            having: None,
            limit: None,
            order_by: None,
        }));

        match result {
            QueryResult::Rows(rows) => {
                assert_eq!(rows.len(), expected_remaining);

                let ids: Vec<i64> = rows.iter().map(|r| r[0].parse::<i64>().unwrap()).collect();
                println!("ids: {:?} expected_ids: {:?}", ids, expected_ids);
                assert_eq!(ids, expected_ids);
            }

            _ => panic!("Expected rows"),
        }
    }
}

#[test]
fn test_select_limit() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![ColumnDef {
            name: "id".into(),
            data_type: DataType::Int,
            primary_key: true,
        }],
    }));

    for i in 1..=5 {
        executor.execute(Statement::Insert(Insert {
            table_name: "users".into(),
            columns: vec!["id".into()],
            values: vec![Expr::Number(i)],
        }));
    }

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        table_name: "users".into(),
        where_clause: None,
        group_by: None,
        distinct: false,
        having: None,
        order_by: None,
        limit: Some(2),
    }));

    match result {
        QueryResult::Rows(rows) => {
            assert_eq!(rows.len(), 2);

            assert_eq!(rows[0][0], "1");
            assert_eq!(rows[1][0], "2");
        }

        _ => panic!("Expected rows"),
    }
}

#[test]
fn test_select_limit_zero() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![ColumnDef {
            name: "id".into(),
            data_type: DataType::Int,
            primary_key: true,
        }],
    }));

    for i in 1..=5 {
        executor.execute(Statement::Insert(Insert {
            table_name: "users".into(),
            columns: vec!["id".into()],
            values: vec![Expr::Number(i)],
        }));
    }

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        table_name: "users".into(),
        where_clause: None,
        group_by: None,
        distinct: false,
        having: None,
        order_by: None,
        limit: Some(0),
    }));

    match result {
        QueryResult::Rows(rows) => {
            assert_eq!(rows.len(), 0);
        }

        _ => panic!("Expected rows"),
    }
}

#[test]
fn test_select_limit_larger_than_table() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![ColumnDef {
            name: "id".into(),
            data_type: DataType::Int,
            primary_key: true,
        }],
    }));

    for i in 1..=5 {
        executor.execute(Statement::Insert(Insert {
            table_name: "users".into(),
            columns: vec!["id".into()],
            values: vec![Expr::Number(i)],
        }));
    }

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        table_name: "users".into(),
        where_clause: None,
        group_by: None,
        distinct: false,
        having: None,
        order_by: None,
        limit: Some(100),
    }));

    match result {
        QueryResult::Rows(rows) => {
            assert_eq!(rows.len(), 5);
        }

        _ => panic!("Expected rows"),
    }
}

#[test]
fn test_select_order_by_ascending() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![ColumnDef {
            name: "id".into(),
            data_type: DataType::Int,
            primary_key: true,
        }],
    }));

    // Insert deliberately out of order.
    for id in [5, 2, 4, 1, 3] {
        executor.execute(Statement::Insert(Insert {
            table_name: "users".into(),
            columns: vec!["id".into()],
            values: vec![Expr::Number(id)],
        }));
    }

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        table_name: "users".into(),
        where_clause: None,
        group_by: None,
        distinct: false,
        having: None,
        order_by: Some(OrderBy {
            column: "id".into(),
            direction: OrderDirection::Asc,
        }),
        limit: None,
    }));

    match result {
        QueryResult::Rows(rows) => {
            let ids: Vec<i64> = rows.iter().map(|r| r[0].parse::<i64>().unwrap()).collect();

            assert_eq!(ids, vec![1, 2, 3, 4, 5]);
        }

        _ => panic!("Expected rows"),
    }
}

#[test]
fn test_select_order_by_descending() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![ColumnDef {
            name: "id".into(),
            data_type: DataType::Int,
            primary_key: true,
        }],
    }));

    for id in [5, 2, 4, 1, 3] {
        executor.execute(Statement::Insert(Insert {
            table_name: "users".into(),
            columns: vec!["id".into()],
            values: vec![Expr::Number(id)],
        }));
    }

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        table_name: "users".into(),
        where_clause: None,
        group_by: None,
        distinct: false,
        having: None,
        order_by: Some(OrderBy {
            column: "id".into(),
            direction: OrderDirection::Desc,
        }),
        limit: None,
    }));

    match result {
        QueryResult::Rows(rows) => {
            let ids: Vec<i64> = rows.iter().map(|r| r[0].parse::<i64>().unwrap()).collect();

            assert_eq!(ids, vec![5, 4, 3, 2, 1]);
        }

        _ => panic!("Expected rows"),
    }
}

#[test]
fn test_select_order_by_text() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
        ],
    }));

    let data = vec![(1, "Charlie"), (2, "Alice"), (3, "Bob")];

    for (id, name) in data {
        executor.execute(Statement::Insert(Insert {
            table_name: "users".into(),
            columns: vec!["id".into(), "name".into()],
            values: vec![Expr::Number(id), Expr::String(name.into())],
        }));
    }

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Column("name".into())],
        table_name: "users".into(),
        where_clause: None,
        group_by: None,
        distinct: false,
        having: None,
        order_by: Some(OrderBy {
            column: "name".into(),
            direction: OrderDirection::Asc,
        }),
        limit: None,
    }));

    match result {
        QueryResult::Rows(rows) => {
            let names: Vec<String> = rows.iter().map(|r| r[0].clone()).collect();

            assert_eq!(names, vec!["Alice", "Bob", "Charlie",]);
        }

        _ => panic!("Expected rows"),
    }
}

#[test]
fn test_select_where_order_by() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![ColumnDef {
            name: "id".into(),
            data_type: DataType::Int,
            primary_key: true,
        }],
    }));

    for id in [5, 2, 4, 1, 3] {
        executor.execute(Statement::Insert(Insert {
            table_name: "users".into(),
            columns: vec!["id".into()],
            values: vec![Expr::Number(id)],
        }));
    }

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        table_name: "users".into(),
        where_clause: Some(Expr::Binary {
            left: Box::new(Expr::Identifier("id".into())),
            op: BinaryOperator::GreaterThanOrEqual,
            right: Box::new(Expr::Number(2)),
        }),
        group_by: None,
        distinct: false,
        having: None,
        order_by: Some(OrderBy {
            column: "id".into(),
            direction: OrderDirection::Desc,
        }),
        limit: None,
    }));

    match result {
        QueryResult::Rows(rows) => {
            let ids: Vec<i64> = rows.iter().map(|r| r[0].parse::<i64>().unwrap()).collect();

            assert_eq!(ids, vec![5, 4, 3, 2]);
        }

        _ => panic!("Expected rows"),
    }
}

#[test]
fn test_select_order_by_limit() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![ColumnDef {
            name: "id".into(),
            data_type: DataType::Int,
            primary_key: true,
        }],
    }));

    for id in [5, 2, 4, 1, 3] {
        executor.execute(Statement::Insert(Insert {
            table_name: "users".into(),
            columns: vec!["id".into()],
            values: vec![Expr::Number(id)],
        }));
    }

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        table_name: "users".into(),
        where_clause: None,
        group_by: None,
        distinct: false,
        having: None,
        order_by: Some(OrderBy {
            column: "id".into(),
            direction: OrderDirection::Desc,
        }),
        limit: Some(2),
    }));

    match result {
        QueryResult::Rows(rows) => {
            let ids: Vec<i64> = rows.iter().map(|r| r[0].parse::<i64>().unwrap()).collect();

            assert_eq!(ids, vec![5, 4]);
        }

        _ => panic!("Expected rows"),
    }
}

#[test]
fn test_select_where_order_by_limit() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![ColumnDef {
            name: "id".into(),
            data_type: DataType::Int,
            primary_key: true,
        }],
    }));

    for id in [5, 2, 4, 1, 3] {
        executor.execute(Statement::Insert(Insert {
            table_name: "users".into(),
            columns: vec!["id".into()],
            values: vec![Expr::Number(id)],
        }));
    }

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        table_name: "users".into(),
        where_clause: Some(Expr::Binary {
            left: Box::new(Expr::Identifier("id".into())),
            op: BinaryOperator::GreaterThan,
            right: Box::new(Expr::Number(1)),
        }),
        group_by: None,
        distinct: false,
        having: None,
        order_by: Some(OrderBy {
            column: "id".into(),
            direction: OrderDirection::Asc,
        }),
        limit: Some(2),
    }));

    match result {
        QueryResult::Rows(rows) => {
            let ids: Vec<i64> = rows.iter().map(|r| r[0].parse::<i64>().unwrap()).collect();

            assert_eq!(ids, vec![2, 3]);
        }

        _ => panic!("Expected rows"),
    }
}

#[test]
fn test_create_table_with_primary_key() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(&mut catalog, &mut engine);

    let result = executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
        ],
    }));

    match result {
        QueryResult::Message(msg) => {
            assert_eq!(msg, "Table created successfully");
        }
        _ => panic!("Expected success"),
    }

    let schema = catalog.table("users").unwrap();

    assert_eq!(schema.columns.len(), 2);
    assert!(schema.columns[0].primary_key);
    assert!(!schema.columns[1].primary_key);
}

#[test]
fn test_create_table_without_primary_key() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(&mut catalog, &mut engine);

    let result = executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: false,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
        ],
    }));

    match result {
        QueryResult::Message(msg) => {
            assert_eq!(msg, "Error: table must contain exactly one PRIMARY KEY");
        }
        _ => panic!("Expected error"),
    }

    assert!(catalog.table("users").is_none());
}

#[test]
fn test_create_table_multiple_primary_keys() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(&mut catalog, &mut engine);

    let result = executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "email".into(),
                data_type: DataType::Text,
                primary_key: true,
            },
        ],
    }));

    match result {
        QueryResult::Message(msg) => {
            assert_eq!(msg, "Error: multiple PRIMARY KEY columns are not allowed");
        }
        _ => panic!("Expected error"),
    }

    assert!(catalog.table("users").is_none());
}

#[test]
fn test_create_index_success() {

    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
        ],
    }));

    let result = executor.execute(
        Statement::CreateIndex(CreateIndex {
            index_name: "idx_users_name".into(),
            table_name: "users".into(),
            column_name: "name".into(),
        }),
    );

    match result {
        QueryResult::Message(msg) => {
            assert_eq!(msg, "Index created successfully");
        }

        _ => panic!("Expected success"),
    }
}

#[test]
fn test_create_index_unknown_table() {

    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(&mut catalog, &mut engine);

    let result = executor.execute(
        Statement::CreateIndex(CreateIndex {
            index_name: "idx".into(),
            table_name: "users".into(),
            column_name: "name".into(),
        }),
    );

    match result {
        QueryResult::Message(msg) => {
            assert!(
                msg.contains("does not exist")
            );
        }

        _ => panic!("Expected error"),
    }
}

#[test]
fn test_create_index_unknown_column() {

    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
        ],
    }));

    let result = executor.execute(
        Statement::CreateIndex(CreateIndex {
            index_name: "idx".into(),
            table_name: "users".into(),
            column_name: "email".into(),
        }),
    );

    match result {
        QueryResult::Message(msg) => {
            assert!(
                msg.contains("unknown column")
            );
        }

        _ => panic!("Expected error")
    }
}

#[test]
fn test_create_index_on_empty_table() {

    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(
        &mut catalog,
        &mut engine,
    );

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
        ],
    }));

    let result = executor.execute(Statement::CreateIndex(CreateIndex {
        index_name: "idx_users_name".into(),
        table_name: "users".into(),
        column_name: "name".into(),
    }));

    match result {
        QueryResult::Message(msg) => {
            assert_eq!(msg, "Index created successfully");
        }
        _ => panic!("Expected success"),
    }

    let entries = scan_index_keys(&mut engine);

    assert_eq!(entries.len(), 0);
}

#[test]
fn test_create_index_single_row() {

    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(
        &mut catalog,
        &mut engine,
    );

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
        ],
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec![
            "id".into(),
            "name".into(),
        ],
        values: vec![
            Expr::Number(1),
            Expr::String("Alice".into()),
        ],
    }));

    executor.execute(Statement::CreateIndex(CreateIndex {
        index_name: "idx_users_name".into(),
        table_name: "users".into(),
        column_name: "name".into(),
    }));

    let entries = scan_index_keys(&mut engine);


    assert_eq!(entries.len(), 1);

    assert_eq!(
        entries[0],
        "__index__:users:name:Alice:1"
    );
}

#[test]
fn test_create_index_duplicate_values() {

    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(
        &mut catalog,
        &mut engine,
    );

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
        ],
    }));

    for (id, name) in [
        (1, "Alice"),
        (2, "Bob"),
        (3, "Alice"),
    ] {

        executor.execute(Statement::Insert(Insert {
            table_name: "users".into(),
            columns: vec![
                "id".into(),
                "name".into(),
            ],
            values: vec![
                Expr::Number(id),
                Expr::String(name.into()),
            ],
        }));
    }

    executor.execute(Statement::CreateIndex(CreateIndex {
        index_name: "idx_users_name".into(),
        table_name: "users".into(),
        column_name: "name".into(),
    }));

    let entries = scan_index_keys(&mut engine);


    let keys: Vec<String> = entries
        .iter()
        .cloned()
        .collect();

    assert_eq!(
        keys,
        vec![
            "__index__:users:name:Alice:1",
            "__index__:users:name:Alice:3",
            "__index__:users:name:Bob:2",
        ]
    );
}

#[test]
fn test_create_index_integer_column() {

    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(
        &mut catalog,
        &mut engine,
    );

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "age".into(),
                data_type: DataType::Int,
                primary_key: false,
            },
        ],
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec![
            "id".into(),
            "age".into(),
        ],
        values: vec![
            Expr::Number(1),
            Expr::Number(25),
        ],
    }));

    executor.execute(Statement::CreateIndex(CreateIndex {
        index_name: "idx_age".into(),
        table_name: "users".into(),
        column_name: "age".into(),
    }));

    let entries = scan_index_keys(&mut engine);


    assert_eq!(
        entries[0],
        "__index__:users:age:00000000000000000025:1"
    );
}

#[test]
fn test_insert_updates_existing_index() {

    let mut catalog = Catalog::new();
    let mut engine =make_engine();

    let mut executor = Executor::new(
        &mut catalog,
        &mut engine,
    );

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
        ],
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec![
            "id".into(),
            "name".into(),
        ],
        values: vec![
            Expr::Number(1),
            Expr::String("Alice".into()),
        ],
    }));

    executor.execute(Statement::CreateIndex(CreateIndex {
        index_name: "idx_name".into(),
        table_name: "users".into(),
        column_name: "name".into(),
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec![
            "id".into(),
            "name".into(),
        ],
        values: vec![
            Expr::Number(2),
            Expr::String("Bob".into()),
        ],
    }));

    let keys = scan_index_keys(&mut engine);

    assert_eq!(
        keys,
        vec![
            "__index__:users:name:Alice:1",
            "__index__:users:name:Bob:2",
        ]
    );
}

#[test]
fn test_insert_duplicate_index_values() {

    let mut catalog = Catalog::new();
    let mut engine =make_engine();

    let mut executor = Executor::new(
        &mut catalog,
        &mut engine,
    );

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
        ],
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec!["id".into(), "name".into()],
        values: vec![Expr::Number(1), Expr::String("Alice".into())],
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec!["id".into(), "name".into()],
        values: vec![Expr::Number(2), Expr::String("Alice".into())],
    }));

    executor.execute(Statement::CreateIndex(CreateIndex {
        index_name: "idx_name".into(),
        table_name: "users".into(),
        column_name: "name".into(),
    }));

    let keys = scan_index_keys(&mut engine);

    assert_eq!(
        keys,
        vec![
            "__index__:users:name:Alice:1",
            "__index__:users:name:Alice:2",
        ]
    );
}

#[test]
fn test_insert_updates_multiple_indexes() {

    let mut catalog = Catalog::new();
    let mut engine =make_engine();

    let mut executor = Executor::new(
        &mut catalog,
        &mut engine,
    );

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
            ColumnDef {
                name: "age".into(),
                data_type: DataType::Int,
                primary_key: false,
            },
        ],
    }));

    executor.execute(Statement::CreateIndex(CreateIndex {
        index_name: "idx_name".into(),
        table_name: "users".into(),
        column_name: "name".into(),
    }));

    executor.execute(Statement::CreateIndex(CreateIndex {
        index_name: "idx_age".into(),
        table_name: "users".into(),
        column_name: "age".into(),
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec![
            "id".into(),
            "name".into(),
            "age".into(),
        ],
        values: vec![
            Expr::Number(1),
            Expr::String("Alice".into()),
            Expr::Number(25),
        ],
    }));

    let keys = scan_index_keys(&mut engine);

    assert_eq!(
        keys,
        vec![
            "__index__:users:age:00000000000000000025:1",
            "__index__:users:name:Alice:1",
        ]
    );
}

#[test]
fn test_create_index_after_multiple_inserts() {

    let mut catalog = Catalog::new();
    let mut engine =make_engine();

    let mut executor = Executor::new(
        &mut catalog,
        &mut engine,
    );

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
        ],
    }));

    for (id, name) in [
        (1, "Alice"),
        (2, "Bob"),
        (3, "Charlie"),
    ] {
        executor.execute(Statement::Insert(Insert {
            table_name: "users".into(),
            columns: vec![
                "id".into(),
                "name".into(),
            ],
            values: vec![
                Expr::Number(id),
                Expr::String(name.into()),
            ],
        }));
    }

    executor.execute(Statement::CreateIndex(CreateIndex {
        index_name: "idx_name".into(),
        table_name: "users".into(),
        column_name: "name".into(),
    }));

    let keys = scan_index_keys(&mut engine);

    assert_eq!(
        keys,
        vec![
            "__index__:users:name:Alice:1",
            "__index__:users:name:Bob:2",
            "__index__:users:name:Charlie:3",
        ]
    );
}

#[test]
fn test_update_updates_index_entry() {

    let mut catalog = Catalog::new();
    let mut engine =make_engine();
    let mut executor = Executor::new(
        &mut catalog,
        &mut engine,
    );

    create_users_table(&mut executor);

    insert_user(&mut executor, 1, "Alice", 20);

    executor.execute(Statement::CreateIndex(CreateIndex {
        index_name: "idx_name".into(),
        table_name: "users".into(),
        column_name: "name".into(),
    }));

    executor.execute(Statement::Update(Update {
        table_name: "users".into(),
        assignments: vec![
            Assignment {
                column: "name".into(),
                value: Expr::String("Bob".into()),
            },
        ],
        where_clause: Some(
            Expr::Binary {
                left: Box::new(Expr::Identifier("id".into())),
                op: BinaryOperator::Equal,
                right: Box::new(Expr::Number(1)),
            },
        ),
    }));

    let keys = scan_index_keys(&mut engine);

    assert_eq!(
        keys,
        vec![
            "__index__:users:name:Bob:1",
        ]
    );
}

#[test]
fn test_update_non_indexed_column_keeps_index() {

    let mut catalog = Catalog::new();
    let mut engine =make_engine();
    let mut executor = Executor::new(
        &mut catalog,
        &mut engine,
    );

    create_users_table(&mut executor);

    insert_user(&mut executor, 1, "Alice", 20);

    executor.execute(Statement::CreateIndex(CreateIndex {
        index_name: "idx_name".into(),
        table_name: "users".into(),
        column_name: "name".into(),
    }));

    executor.execute(Statement::Update(Update {
        table_name: "users".into(),
        assignments: vec![
            Assignment {
                column: "age".into(),
                value: Expr::Number(30),
            },
        ],
        where_clause: Some(
            Expr::Binary {
                left: Box::new(Expr::Identifier("id".into())),
                op: BinaryOperator::Equal,
                right: Box::new(Expr::Number(1)),
            },
        ),
    }));

    let keys = scan_index_keys(&mut engine);

    assert_eq!(
        keys,
        vec![
            "__index__:users:name:Alice:1",
        ]
    );
}

#[test]
fn test_update_creates_duplicate_index_values() {

    let mut catalog = Catalog::new();
    let mut engine =make_engine();
    let mut executor = Executor::new(
        &mut catalog,
        &mut engine,
    );

    create_users_table(&mut executor);

    insert_user(&mut executor, 1, "Alice", 20);
    insert_user(&mut executor, 2, "Bob", 21);

    executor.execute(Statement::CreateIndex(CreateIndex {
        index_name: "idx_name".into(),
        table_name: "users".into(),
        column_name: "name".into(),
    }));

    executor.execute(Statement::Update(Update {
        table_name: "users".into(),
        assignments: vec![
            Assignment {
                column: "name".into(),
                value: Expr::String("Alice".into()),
            },
        ],
        where_clause: Some(
            Expr::Binary {
                left: Box::new(Expr::Identifier("id".into())),
                op: BinaryOperator::Equal,
                right: Box::new(Expr::Number(2)),
            },
        ),
    }));

    let keys = scan_index_keys(&mut engine);

    assert_eq!(
        keys,
        vec![
            "__index__:users:name:Alice:1",
            "__index__:users:name:Alice:2",
        ]
    );
}

#[test]
fn test_update_updates_multiple_indexes() {

    let mut catalog = Catalog::new();
    let mut engine =make_engine();
    let mut executor = Executor::new(
        &mut catalog,
        &mut engine,
    );

    create_users_table(&mut executor);

    insert_user(&mut executor, 1, "Alice", 20);

    executor.execute(Statement::CreateIndex(CreateIndex {
        index_name: "idx_name".into(),
        table_name: "users".into(),
        column_name: "name".into(),
    }));

    executor.execute(Statement::CreateIndex(CreateIndex {
        index_name: "idx_age".into(),
        table_name: "users".into(),
        column_name: "age".into(),
    }));

    executor.execute(Statement::Update(Update {
        table_name: "users".into(),
        assignments: vec![
            Assignment {
                column: "name".into(),
                value: Expr::String("Bob".into()),
            },
            Assignment {
                column: "age".into(),
                value: Expr::Number(30),
            },
        ],
        where_clause: Some(
            Expr::Binary {
                left: Box::new(Expr::Identifier("id".into())),
                op: BinaryOperator::Equal,
                right: Box::new(Expr::Number(1)),
            },
        ),
    }));

    let keys = scan_index_keys(&mut engine);

    assert_eq!(
        keys,
        vec![
            "__index__:users:age:00000000000000000030:1",
            "__index__:users:name:Bob:1",
        ]
    );
}

#[test]
fn test_index_schema_round_trip() {
    let schema = IndexSchema {
        name: "idx_name".into(),
        table_name: "users".into(),
        column_name: "name".into(),
    };

    let serialized = schema.serialize();

    let deserialized =
        IndexSchema::deserialize(&serialized).unwrap();

    assert_eq!(schema, deserialized);
}

#[test]
fn test_invalid_index_schema() {
    let result =
        IndexSchema::deserialize("invalid");

    assert!(result.is_err());
}

#[test]
fn test_create_index_persists_metadata() {

    let mut catalog = Catalog::new();

    let mut engine =make_engine();
    let mut executor =
        Executor::new(&mut catalog, &mut engine);

    create_users_table(&mut executor);

    executor.execute(
        Statement::CreateIndex(CreateIndex {
            index_name: "idx_name".into(),
            table_name: "users".into(),
            column_name: "name".into(),
        }),
    );

    let value = engine
        .get("__index_meta__:idx_name")
        .unwrap();

    match value {
        Value::Data(serialized) => {
            let schema =
                IndexSchema::deserialize(&serialized)
                    .unwrap();

            assert_eq!(schema.name, "idx_name");
            assert_eq!(schema.table_name, "users");
            assert_eq!(schema.column_name, "name");
        }

        _ => panic!("expected metadata"),
    }
}

#[test]
fn test_catalog_recovers_index_metadata() {

    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    // First session
    {
        let mut executor =
            Executor::new(
                &mut catalog,
                &mut engine,
            );

        create_users_table(&mut executor);

        executor.execute(
            Statement::CreateIndex(CreateIndex {
                index_name: "idx_name".into(),
                table_name: "users".into(),
                column_name: "name".into(),
            }),
        );
    }

    // Simulate restart by creating a fresh catalog and
    // loading from the same engine.
    let mut recovered_catalog = Catalog::new();

    recovered_catalog
        .load_from_engine(&mut engine)
        .unwrap();

    recovered_catalog
        .load_indexes_from_engine(&mut engine)
        .unwrap();

    let indexes =
        recovered_catalog.indexes_for_table("users");

    assert_eq!(indexes.len(), 1);

    assert_eq!(
        indexes[0].name,
        "idx_name",
    );
}

#[test]
fn test_catalog_recovers_multiple_indexes() {

    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    // First session
    {
        let mut executor =
            Executor::new(
                &mut catalog,
                &mut engine,
            );

        create_users_table(&mut executor);

        executor.execute(
            Statement::CreateIndex(CreateIndex {
                index_name: "idx_name".into(),
                table_name: "users".into(),
                column_name: "name".into(),
            }),
        );

        executor.execute(
            Statement::CreateIndex(CreateIndex {
                index_name: "idx_age".into(),
                table_name: "users".into(),
                column_name: "age".into(),
            }),
        );
    }

    // Simulate restart
    let mut recovered_catalog = Catalog::new();

    recovered_catalog
        .load_from_engine(&mut engine)
        .unwrap();

    recovered_catalog
        .load_indexes_from_engine(&mut engine)
        .unwrap();

    let indexes =
        recovered_catalog.indexes_for_table("users");

    assert_eq!(indexes.len(), 2);
}

#[test]
fn test_select_uses_index_multiple_matches() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (
            id INT PRIMARY KEY,
            name TEXT
        );",
    );

    execute_sql(
        &mut executor,
        "CREATE INDEX idx_name ON users(name);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id,name) VALUES (1,'Alice');",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id,name) VALUES (2,'Bob');",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id,name) VALUES (3,'Alice');",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE name='Alice';",
    );

    assert_rows_eq(
        result,
        vec![
            vec!["1", "Alice"],
            vec!["3", "Alice"],
        ],
    );
}

#[test]
fn test_select_index_no_matches() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (
            id INT PRIMARY KEY,
            name TEXT
        );",
    );

    execute_sql(
        &mut executor,
        "CREATE INDEX idx_name ON users(name);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id,name) VALUES (1,'Alice');",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id,name) VALUES (2,'Bob');",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE name='Charlie';",
    );

    assert_rows_eq(result, vec![]);
}

#[test]
fn test_select_index_many_duplicates() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (
            id INT PRIMARY KEY,
            name TEXT
        );",
    );

    execute_sql(
        &mut executor,
        "CREATE INDEX idx_name ON users(name);",
    );

    for i in 1..=20 {

        execute_sql(
            &mut executor,
            &format!(
                "INSERT INTO users (id,name) VALUES ({},'Alice');",
                i
            ),
        );
    }

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE name='Alice';",
    );

    assert_eq!(
        extract_rows(&result).len(),
        20,
    );
}

#[test]
fn test_integer_index_lookup() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (
            id INT PRIMARY KEY,
            age INT
        );",
    );

    execute_sql(
        &mut executor,
        "CREATE INDEX idx_age ON users(age);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id,age) VALUES (1,20);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id,age) VALUES (2,30);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE age=30;",
    );

    assert_rows_eq(
        result,
        vec![
            vec!["2", "30"],
        ],
    );
}

#[test]
fn test_update_using_index() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (
            id INT PRIMARY KEY,
            name TEXT
        );",
    );

    execute_sql(
        &mut executor,
        "CREATE INDEX idx_name ON users(name);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id,name) VALUES (1,'Alice');",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id,name) VALUES (2,'Bob');",
    );

    execute_sql(
        &mut executor,
        "UPDATE users
         SET name='Charlie'
         WHERE name='Alice';",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users
         WHERE name='Charlie';",
    );

    assert_rows_eq(
        result,
        vec![
            vec!["1", "Charlie"],
        ],
    );
}

#[test]
fn test_delete_using_index() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (
            id INT PRIMARY KEY,
            name TEXT
        );",
    );

    execute_sql(
        &mut executor,
        "CREATE INDEX idx_name ON users(name);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id,name) VALUES (1,'Alice');",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id,name) VALUES (2,'Bob');",
    );

    execute_sql(
        &mut executor,
        "DELETE FROM users
         WHERE name='Alice';",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users;",
    );

    assert_rows_eq(
        result,
        vec![
            vec!["2", "Bob"],
        ],
    );
}

//----------------------------------------------------------
// RANGE INDEX SCAN TESTS
//----------------------------------------------------------

#[test]
fn test_index_scan_greater_than() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (
            id INT PRIMARY KEY,
            age INT
        );",
    );

    execute_sql(
        &mut executor,
        "CREATE INDEX idx_age ON users(age);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id,age) VALUES (1,10);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id,age) VALUES (2,20);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id,age) VALUES (3,25);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id,age) VALUES (4,30);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id,age) VALUES (5,40);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE age > 20 ORDER BY id;",
    );

    assert_rows_eq(
        result,
        vec![
            vec!["3", "25"],
            vec!["4", "30"],
            vec!["5", "40"],
        ],
    );
}

#[test]
fn test_index_scan_greater_than_or_equal() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (
            id INT PRIMARY KEY,
            age INT
        );",
    );

    execute_sql(
        &mut executor,
        "CREATE INDEX idx_age ON users(age);",
    );

    for (id, age) in [
        (1,10),
        (2,20),
        (3,25),
        (4,30),
        (5,40),
    ] {

        execute_sql(
            &mut executor,
            &format!(
                "INSERT INTO users (id,age) VALUES ({},{});",
                id,
                age,
            ),
        );
    }

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE age >= 20 ORDER BY id;",
    );

    assert_rows_eq(
        result,
        vec![
            vec!["2", "20"],
            vec!["3", "25"],
            vec!["4", "30"],
            vec!["5", "40"],
        ],
    );
}

#[test]
fn test_index_scan_less_than() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (
            id INT PRIMARY KEY,
            age INT
        );",
    );

    execute_sql(
        &mut executor,
        "CREATE INDEX idx_age ON users(age);",
    );

    for (id, age) in [
        (1,10),
        (2,20),
        (3,25),
        (4,30),
        (5,40),
    ] {

        execute_sql(
            &mut executor,
            &format!(
                "INSERT INTO users (id,age) VALUES ({},{});",
                id,
                age,
            ),
        );
    }

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE age < 25 ORDER BY id;",
    );

    assert_rows_eq(
        result,
        vec![
            vec!["1", "10"],
            vec!["2", "20"],
        ],
    );
}

#[test]
fn test_index_scan_less_than_or_equal() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (
            id INT PRIMARY KEY,
            age INT
        );",
    );

    execute_sql(
        &mut executor,
        "CREATE INDEX idx_age ON users(age);",
    );

    for (id, age) in [
        (1,10),
        (2,20),
        (3,25),
        (4,30),
        (5,40),
    ] {

        execute_sql(
            &mut executor,
            &format!(
                "INSERT INTO users (id,age) VALUES ({},{});",
                id,
                age,
            ),
        );
    }

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE age <= 25 ORDER BY id;",
    );

    assert_rows_eq(
        result,
        vec![
            vec!["1", "10"],
            vec!["2", "20"],
            vec!["3", "25"],
        ],
    );
}

#[test]
fn test_index_scan_duplicate_boundary() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (
            id INT PRIMARY KEY,
            age INT
        );",
    );

    execute_sql(
        &mut executor,
        "CREATE INDEX idx_age ON users(age);",
    );

    for (id, age) in [
        (1,20),
        (2,20),
        (3,20),
        (4,25),
        (5,30),
    ] {

        execute_sql(
            &mut executor,
            &format!(
                "INSERT INTO users (id,age) VALUES ({},{});",
                id,
                age,
            ),
        );
    }

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE age > 20 ORDER BY id;",
    );

    assert_rows_eq(
        result,
        vec![
            vec!["4", "25"],
            vec!["5", "30"],
        ],
    );
}

// =============================================================
// AGGREGATE FUNCTION TESTS
// =============================================================

// -------------------------------------------------------------
// COUNT
// -------------------------------------------------------------

#[test]
fn test_count_all_rows() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (
            id INT PRIMARY KEY,
            name TEXT,
            age INT
        );",
    );

    for i in 1..=5 {
        execute_sql(
            &mut executor,
            &format!(
                "INSERT INTO users (id,name,age) VALUES ({},'User{}',{});",
                i, i, i * 10,
            ),
        );
    }

    let result = execute_sql(
        &mut executor,
        "SELECT COUNT(*) FROM users;",
    );

    assert_rows_eq(result, vec![vec!["5"]]);
}

#[test]
fn test_count_where() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (
            id INT PRIMARY KEY,
            name TEXT,
            age INT
        );",
    );

    for i in 1..=5 {
        execute_sql(
            &mut executor,
            &format!(
                "INSERT INTO users (id,name,age) VALUES ({},'User{}',{});",
                i, i, i * 10,
            ),
        );
    }

    let result = execute_sql(
        &mut executor,
        "SELECT COUNT(*) FROM users WHERE age > 20;",
    );

    assert_rows_eq(result, vec![vec!["3"]]);
}

#[test]
fn test_count_empty_table() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (
            id INT PRIMARY KEY,
            name TEXT,
            age INT
        );",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT COUNT(*) FROM users;",
    );

    assert_rows_eq(result, vec![vec!["0"]]);
}

// -------------------------------------------------------------
// MIN
// -------------------------------------------------------------

#[test]
fn test_min_integer() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (
            id INT PRIMARY KEY,
            age INT
        );",
    );

    for (id, age) in [(1, 50), (2, 10), (3, 30), (4, 20), (5, 40)] {
        execute_sql(
            &mut executor,
            &format!(
                "INSERT INTO users (id,age) VALUES ({},{});",
                id, age,
            ),
        );
    }

    let result = execute_sql(
        &mut executor,
        "SELECT MIN(age) FROM users;",
    );

    assert_rows_eq(result, vec![vec!["10"]]);
}

#[test]
fn test_min_text() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (
            id INT PRIMARY KEY,
            name TEXT
        );",
    );

    for (id, name) in [(1, "Charlie"), (2, "Alice"), (3, "Bob")] {
        execute_sql(
            &mut executor,
            &format!(
                "INSERT INTO users (id,name) VALUES ({},'{}');",
                id, name,
            ),
        );
    }

    let result = execute_sql(
        &mut executor,
        "SELECT MIN(name) FROM users;",
    );

    assert_rows_eq(result, vec![vec!["Alice"]]);
}

#[test]
fn test_min_with_index() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (
            id INT PRIMARY KEY,
            age INT
        );",
    );

    execute_sql(
        &mut executor,
        "CREATE INDEX idx_age ON users(age);",
    );

    for (id, age) in [(1, 50), (2, 10), (3, 30), (4, 20), (5, 40)] {
        execute_sql(
            &mut executor,
            &format!(
                "INSERT INTO users (id,age) VALUES ({},{});",
                id, age,
            ),
        );
    }

    let result = execute_sql(
        &mut executor,
        "SELECT MIN(age) FROM users;",
    );

    assert_rows_eq(result, vec![vec!["10"]]);
}

// -------------------------------------------------------------
// MAX
// -------------------------------------------------------------

#[test]
fn test_max_integer() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (
            id INT PRIMARY KEY,
            age INT
        );",
    );

    for (id, age) in [(1, 50), (2, 10), (3, 30), (4, 20), (5, 40)] {
        execute_sql(
            &mut executor,
            &format!(
                "INSERT INTO users (id,age) VALUES ({},{});",
                id, age,
            ),
        );
    }

    let result = execute_sql(
        &mut executor,
        "SELECT MAX(age) FROM users;",
    );

    assert_rows_eq(result, vec![vec!["50"]]);
}

#[test]
fn test_max_text() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (
            id INT PRIMARY KEY,
            name TEXT
        );",
    );

    for (id, name) in [(1, "Alice"), (2, "Charlie"), (3, "Bob")] {
        execute_sql(
            &mut executor,
            &format!(
                "INSERT INTO users (id,name) VALUES ({},'{}');",
                id, name,
            ),
        );
    }

    let result = execute_sql(
        &mut executor,
        "SELECT MAX(name) FROM users;",
    );

    assert_rows_eq(result, vec![vec!["Charlie"]]);
}

#[test]
fn test_max_with_index() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (
            id INT PRIMARY KEY,
            age INT
        );",
    );

    execute_sql(
        &mut executor,
        "CREATE INDEX idx_age ON users(age);",
    );

    for (id, age) in [(1, 50), (2, 10), (3, 30), (4, 20), (5, 40)] {
        execute_sql(
            &mut executor,
            &format!(
                "INSERT INTO users (id,age) VALUES ({},{});",
                id, age,
            ),
        );
    }

    let result = execute_sql(
        &mut executor,
        "SELECT MAX(age) FROM users;",
    );

    assert_rows_eq(result, vec![vec!["50"]]);
}

// -------------------------------------------------------------
// SUM
// -------------------------------------------------------------

#[test]
fn test_sum_integer() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (
            id INT PRIMARY KEY,
            age INT
        );",
    );

    for (id, age) in [(1, 10), (2, 20), (3, 30), (4, 40), (5, 50)] {
        execute_sql(
            &mut executor,
            &format!(
                "INSERT INTO users (id,age) VALUES ({},{});",
                id, age,
            ),
        );
    }

    let result = execute_sql(
        &mut executor,
        "SELECT SUM(age) FROM users;",
    );

    assert_rows_eq(result, vec![vec!["150"]]);
}

#[test]
fn test_sum_where() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (
            id INT PRIMARY KEY,
            age INT
        );",
    );

    for (id, age) in [(1, 10), (2, 20), (3, 30), (4, 40), (5, 50)] {
        execute_sql(
            &mut executor,
            &format!(
                "INSERT INTO users (id,age) VALUES ({},{});",
                id, age,
            ),
        );
    }

    let result = execute_sql(
        &mut executor,
        "SELECT SUM(age) FROM users WHERE age > 20;",
    );

    assert_rows_eq(result, vec![vec!["120"]]);
}

#[test]
fn test_sum_text_error() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (
            id INT PRIMARY KEY,
            name TEXT
        );",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id,name) VALUES (1,'Alice');",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT SUM(name) FROM users;",
    );

    match result {
        QueryResult::Message(msg) => {
            assert!(msg.contains("INTEGER"), "Expected INTEGER error, got: {}", msg);
        }
        other => panic!("Expected Message, got {:?}", other),
    }
}

// -------------------------------------------------------------
// AVG
// -------------------------------------------------------------

#[test]
fn test_avg_integer() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (
            id INT PRIMARY KEY,
            age INT
        );",
    );

    for (id, age) in [(1, 10), (2, 20), (3, 30), (4, 40), (5, 50)] {
        execute_sql(
            &mut executor,
            &format!(
                "INSERT INTO users (id,age) VALUES ({},{});",
                id, age,
            ),
        );
    }

    let result = execute_sql(
        &mut executor,
        "SELECT AVG(age) FROM users;",
    );

    // (10 + 20 + 30 + 40 + 50) / 5 = 30
    assert_rows_eq(result, vec![vec!["30"]]);
}

#[test]
fn test_avg_where() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (
            id INT PRIMARY KEY,
            age INT
        );",
    );

    for (id, age) in [(1, 10), (2, 20), (3, 30), (4, 40), (5, 50)] {
        execute_sql(
            &mut executor,
            &format!(
                "INSERT INTO users (id,age) VALUES ({},{});",
                id, age,
            ),
        );
    }

    let result = execute_sql(
        &mut executor,
        "SELECT AVG(age) FROM users WHERE age > 20;",
    );

    // (30 + 40 + 50) / 3 = 40
    assert_rows_eq(result, vec![vec!["40"]]);
}

#[test]
fn test_avg_text_error() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (
            id INT PRIMARY KEY,
            name TEXT
        );",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id,name) VALUES (1,'Alice');",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT AVG(name) FROM users;",
    );

    match result {
        QueryResult::Message(msg) => {
            assert!(msg.contains("INTEGER"), "Expected INTEGER error, got: {}", msg);
        }
        other => panic!("Expected Message, got {:?}", other),
    }
}

// -------------------------------------------------------------
// EMPTY TABLE (all aggregates)
// -------------------------------------------------------------

#[test]
fn test_aggregate_empty_table() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (
            id INT PRIMARY KEY,
            age INT
        );",
    );

    // COUNT(*) on empty table
    let result = execute_sql(
        &mut executor,
        "SELECT COUNT(*) FROM users;",
    );
    assert_rows_eq(result, vec![vec!["0"]]);

    // MIN on empty table
    let result = execute_sql(
        &mut executor,
        "SELECT MIN(age) FROM users;",
    );
    assert_rows_eq(result, vec![]);

    // MAX on empty table
    let result = execute_sql(
        &mut executor,
        "SELECT MAX(age) FROM users;",
    );
    assert_rows_eq(result, vec![]);

    // SUM on empty table
    let result = execute_sql(
        &mut executor,
        "SELECT SUM(age) FROM users;",
    );
    assert_rows_eq(result, vec![vec!["0"]]);

    // AVG on empty table
    let result = execute_sql(
        &mut executor,
        "SELECT AVG(age) FROM users;",
    );
    assert_rows_eq(result, vec![vec!["0"]]);
}

// =============================================================
// BUILD GROUPS TESTS
// =============================================================

use std::collections::HashMap;

fn make_row(values: Vec<(&str, RowValue)>) -> Row {
    let mut row = Row::new();
    for (col, val) in values {
        row.insert(col, val);
    }
    row
}

#[test]
fn test_build_groups_single_group() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE employees (
            id INT PRIMARY KEY,
            department TEXT,
            name TEXT
        );",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO employees (id,department,name) VALUES (1,'Engineering','Alice');",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO employees (id,department,name) VALUES (2,'Engineering','Bob');",
    );

    let rows = executor.scan_table("employees").unwrap();

    let groups = executor
        .build_groups(rows, &vec!["department".into()])
        .unwrap();

    assert_eq!(groups.len(), 1);

    assert_eq!(
        groups.values().next().unwrap().len(),
        2,
    );
}

#[test]
fn test_build_groups_multiple_groups() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE employees (
            id INT PRIMARY KEY,
            department TEXT,
            name TEXT
        );",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO employees (id,department,name) VALUES (1,'Engineering','Alice');",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO employees (id,department,name) VALUES (2,'Sales','Bob');",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO employees (id,department,name) VALUES (3,'Engineering','Charlie');",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO employees (id,department,name) VALUES (4,'HR','David');",
    );

    let rows = executor.scan_table("employees").unwrap();

    let groups = executor
        .build_groups(rows, &vec!["department".into()])
        .unwrap();

    assert_eq!(groups.len(), 3);

    // Engineering -> 2, Sales -> 1, HR -> 1
    for (key, group_rows) in &groups {
        let dept = match &key[0] {
            RowValue::Text(s) => s.as_str(),
            _ => panic!("expected text key"),
        };
        match dept {
            "Engineering" => assert_eq!(group_rows.len(), 2),
            "Sales" => assert_eq!(group_rows.len(), 1),
            "HR" => assert_eq!(group_rows.len(), 1),
            other => panic!("unexpected department: {}", other),
        }
    }
}

#[test]
fn test_build_groups_multiple_columns() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE employees (
            id INT PRIMARY KEY,
            department TEXT,
            city TEXT
        );",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO employees (id,department,city) VALUES (1,'Engineering','London');",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO employees (id,department,city) VALUES (2,'Engineering','Paris');",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO employees (id,department,city) VALUES (3,'Engineering','London');",
    );

    let rows = executor.scan_table("employees").unwrap();

    let groups = executor
        .build_groups(rows, &vec!["department".into(), "city".into()])
        .unwrap();

    assert_eq!(groups.len(), 2);

    for (key, group_rows) in &groups {
        let dept = match &key[0] {
            RowValue::Text(s) => s.as_str(),
            _ => panic!("expected text key"),
        };
        let city = match &key[1] {
            RowValue::Text(s) => s.as_str(),
            _ => panic!("expected text key"),
        };
        match (dept, city) {
            ("Engineering", "London") => assert_eq!(group_rows.len(), 2),
            ("Engineering", "Paris") => assert_eq!(group_rows.len(), 1),
            other => panic!("unexpected group: {:?}", other),
        }
    }
}

#[test]
fn test_build_groups_unknown_column() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE employees (
            id INT PRIMARY KEY,
            department TEXT
        );",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO employees (id,department) VALUES (1,'Engineering');",
    );

    let rows = executor.scan_table("employees").unwrap();

    let result = executor
        .build_groups(rows, &vec!["salary".into()]);

    assert!(result.is_err());
}

// =============================================================
// GROUP BY + MIN / MAX TESTS
// =============================================================

fn assert_rows_eq_sorted(result: QueryResult, expected: Vec<Vec<&str>>) {
    let mut actual = match result {
        QueryResult::Rows(rows) => rows,
        other => panic!("Expected Rows, got {:?}", other),
    };
    actual.sort();

    let mut expected: Vec<Vec<String>> = expected
        .into_iter()
        .map(|row| row.into_iter().map(|s| s.to_string()).collect())
        .collect();
    expected.sort();

    assert_eq!(actual, expected);
}

#[test]
fn test_group_by_min_integer() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (
            id INT PRIMARY KEY,
            age INT
        );",
    );

    // Insert values with some duplicate ages.
    execute_sql(&mut executor, "INSERT INTO users (id,age) VALUES (1,25);");
    execute_sql(&mut executor, "INSERT INTO users (id,age) VALUES (2,10);");
    execute_sql(&mut executor, "INSERT INTO users (id,age) VALUES (3,40);");
    execute_sql(&mut executor, "INSERT INTO users (id,age) VALUES (4,50);");
    execute_sql(&mut executor, "INSERT INTO users (id,age) VALUES (5,40);");

    let result = execute_sql(
        &mut executor,
        "SELECT age, MIN(age) FROM users GROUP BY age;",
    );

    assert_rows_eq_sorted(
        result,
        vec![
            vec!["10", "10"],
            vec!["25", "25"],
            vec!["40", "40"],
            vec!["50", "50"],
        ],
    );
}

#[test]
fn test_group_by_max_integer() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (
            id INT PRIMARY KEY,
            age INT
        );",
    );

    execute_sql(&mut executor, "INSERT INTO users (id,age) VALUES (1,25);");
    execute_sql(&mut executor, "INSERT INTO users (id,age) VALUES (2,10);");
    execute_sql(&mut executor, "INSERT INTO users (id,age) VALUES (3,40);");
    execute_sql(&mut executor, "INSERT INTO users (id,age) VALUES (4,50);");
    execute_sql(&mut executor, "INSERT INTO users (id,age) VALUES (5,40);");

    let result = execute_sql(
        &mut executor,
        "SELECT age, MAX(age) FROM users GROUP BY age;",
    );

    assert_rows_eq_sorted(
        result,
        vec![
            vec!["10", "10"],
            vec!["25", "25"],
            vec!["40", "40"],
            vec!["50", "50"],
        ],
    );
}

#[test]
fn test_group_by_min_different_column() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE employees (
            id INT PRIMARY KEY,
            department TEXT,
            salary INT
        );",
    );

    execute_sql(&mut executor, "INSERT INTO employees (id,department,salary) VALUES (1,'Engineering',100);");
    execute_sql(&mut executor, "INSERT INTO employees (id,department,salary) VALUES (2,'Engineering',250);");
    execute_sql(&mut executor, "INSERT INTO employees (id,department,salary) VALUES (3,'Engineering',180);");
    execute_sql(&mut executor, "INSERT INTO employees (id,department,salary) VALUES (4,'Sales',90);");
    execute_sql(&mut executor, "INSERT INTO employees (id,department,salary) VALUES (5,'Sales',150);");

    let result = execute_sql(
        &mut executor,
        "SELECT department, MIN(salary) FROM employees GROUP BY department;",
    );

    assert_rows_eq_sorted(
        result,
        vec![
            vec!["Engineering", "100"],
            vec!["Sales", "90"],
        ],
    );
}

#[test]
fn test_group_by_max_different_column() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE employees (
            id INT PRIMARY KEY,
            department TEXT,
            salary INT
        );",
    );

    execute_sql(&mut executor, "INSERT INTO employees (id,department,salary) VALUES (1,'Engineering',100);");
    execute_sql(&mut executor, "INSERT INTO employees (id,department,salary) VALUES (2,'Engineering',250);");
    execute_sql(&mut executor, "INSERT INTO employees (id,department,salary) VALUES (3,'Engineering',180);");
    execute_sql(&mut executor, "INSERT INTO employees (id,department,salary) VALUES (4,'Sales',90);");
    execute_sql(&mut executor, "INSERT INTO employees (id,department,salary) VALUES (5,'Sales',150);");

    let result = execute_sql(
        &mut executor,
        "SELECT department, MAX(salary) FROM employees GROUP BY department;",
    );

    assert_rows_eq_sorted(
        result,
        vec![
            vec!["Engineering", "250"],
            vec!["Sales", "150"],
        ],
    );
}

#[test]
fn test_group_by_min_text() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE employees (
            id INT PRIMARY KEY,
            department TEXT,
            name TEXT
        );",
    );

    execute_sql(&mut executor, "INSERT INTO employees (id,department,name) VALUES (1,'Engineering','Charlie');");
    execute_sql(&mut executor, "INSERT INTO employees (id,department,name) VALUES (2,'Engineering','Alice');");
    execute_sql(&mut executor, "INSERT INTO employees (id,department,name) VALUES (3,'Engineering','Bob');");
    execute_sql(&mut executor, "INSERT INTO employees (id,department,name) VALUES (4,'Sales','David');");
    execute_sql(&mut executor, "INSERT INTO employees (id,department,name) VALUES (5,'Sales','Aaron');");

    let result = execute_sql(
        &mut executor,
        "SELECT department, MIN(name) FROM employees GROUP BY department;",
    );

    assert_rows_eq_sorted(
        result,
        vec![
            vec!["Engineering", "Alice"],
            vec!["Sales", "Aaron"],
        ],
    );
}

#[test]
fn test_group_by_max_text() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE employees (
            id INT PRIMARY KEY,
            department TEXT,
            name TEXT
        );",
    );

    execute_sql(&mut executor, "INSERT INTO employees (id,department,name) VALUES (1,'Engineering','Charlie');");
    execute_sql(&mut executor, "INSERT INTO employees (id,department,name) VALUES (2,'Engineering','Alice');");
    execute_sql(&mut executor, "INSERT INTO employees (id,department,name) VALUES (3,'Engineering','Bob');");
    execute_sql(&mut executor, "INSERT INTO employees (id,department,name) VALUES (4,'Sales','David');");
    execute_sql(&mut executor, "INSERT INTO employees (id,department,name) VALUES (5,'Sales','Aaron');");

    let result = execute_sql(
        &mut executor,
        "SELECT department, MAX(name) FROM employees GROUP BY department;",
    );

    assert_rows_eq_sorted(
        result,
        vec![
            vec!["Engineering", "Charlie"],
            vec!["Sales", "David"],
        ],
    );
}

