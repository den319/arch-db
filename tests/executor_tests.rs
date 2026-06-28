use arch_db::{sql::{ast::{ColumnDef, CreateTable, DataType, Expr, Insert, Statement}, catalog::{Catalog, Column, DataType, TableSchema}, executor::{Executor, QueryResult}}, storage::{Storage, SyncPolicy}};

fn make_storage() -> Storage {
    let dir = std::env::temp_dir().join(format!("exec_test_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    Storage::new(dir.to_str().unwrap(), SyncPolicy::Never).unwrap()
}

#[test]
fn test_execute_create_table() {
    let mut catalog = Catalog::new();
    let mut storage = make_storage();

    let mut executor =
        Executor::new(
            &mut catalog,
            &mut storage,
        );

    let stmt = Statement::CreateTable(
        CreateTable {
            table_name: "users".into(),
            columns: vec![
                ColumnDef {
                    name: "id".into(),
                    data_type: DataType::Int,
                },
                ColumnDef {
                    name: "name".into(),
                    data_type: DataType::Text,
                },
            ],
        },
    );

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
    let mut storage = make_storage();

    let mut executor =
        Executor::new(
            &mut catalog,
            &mut storage,
        );

    let stmt = Statement::CreateTable(
        CreateTable {
            table_name: "users".into(),
            columns: vec![],
        },
    );

    let result = executor.execute(stmt);

    // First execution should succeed
    assert_eq!(result, QueryResult::Message("Table created successfully".into()));

    // Second execution with same table name should fail
    let stmt2 = Statement::CreateTable(
        CreateTable {
            table_name: "users".into(),
            columns: vec![],
        },
    );

    let result2 = executor.execute(stmt2);

    assert!(
        matches!(result2, QueryResult::Message(msg) if msg.starts_with("Error:"))
    );
}


#[test]
fn test_execute_dispatch_create_table() {
    let mut catalog = Catalog::new();
    let mut storage = make_storage();

    let mut executor =
        Executor::new(
            &mut catalog,
            &mut storage,
        );

    executor.execute(
        Statement::CreateTable(CreateTable {
            table_name: "users".into(),
            columns: vec![],
        }),
    );
}

#[test]
fn test_insert_unknown_table() {
    let mut catalog = Catalog::new();
    let mut storage= make_storage();

    let mut executor = Executor::new(
        &mut catalog,
        &mut storage,
    );

    let stmt = Insert {
        table_name: "users".into(),
        columns: vec![
            "id".into(),
            "name".into(),
        ],
        values: vec![
            Expr::Number(1),
            Expr::String("Alice".into()),
        ],
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
                data_type: arch_db::sql::catalog::DataType::Integer,
                primary_key: true,
                nullable: false,
            },
            Column {
                name: "name".into(),
                data_type: arch_db::sql::catalog::DataType::Text,
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
    let mut storage= make_storage();

    let mut executor = Executor::new(
        &mut catalog,
        &mut storage,
    );
    let stmt = Insert {
        table_name: "users".into(),
        columns: vec![
            "id".into(),
            "name".into(),
        ],
        values: vec![
            Expr::Number(1),
            Expr::String("Alice".into()),
        ],
    };

    let result = executor.execute_insert(stmt);

    assert!(result.is_ok());
}