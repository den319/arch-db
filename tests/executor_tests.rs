use arch_db::{sql::{ast::{ColumnDef, CreateTable, DataType, Statement}, catalog::Catalog, executor::Executor}, storage::Storage};

#[test]
fn test_execute_create_table() {
    let mut catalog = Catalog::new();
    let mut storage = Storage::new();

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
    let mut storage = Storage::new();

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

    executor.execute(stmt.clone());

    assert!(
        executor.execute(stmt).is_err()
    );
}