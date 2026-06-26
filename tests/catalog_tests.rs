use arch_db::sql::catalog::{Catalog, Column, DataType, TableSchema};

#[test]
fn create_table() {
    let mut catalog = Catalog::new();

    let schema = TableSchema {
        name: "users".into(),
        columns: vec![
            Column {
                name: "id".into(),
                data_type: DataType::Integer,
                primary_key: true,
                nullable: false,
            }
        ],
    };

    assert!(catalog.create_table(schema).is_ok());
    assert!(catalog.exists("users"));
}

#[test]
fn duplicate_table() {
    let mut catalog = Catalog::new();

    let schema = TableSchema {
        name: "users".into(),
        columns: vec![],
    };

    catalog.create_table(schema.clone()).unwrap();

    assert!(catalog.create_table(schema).is_err());
}

#[test]
fn lookup_table() {
    let mut catalog = Catalog::new();

    let schema = TableSchema {
        name: "users".into(),
        columns: vec![],
    };

    catalog.create_table(schema).unwrap();

    assert!(catalog.table("users").is_some());
    assert!(catalog.table("posts").is_none());
}

#[test]
fn primary_key_lookup() {
    let schema = TableSchema {
        name: "users".into(),
        columns: vec![
            Column {
                name: "id".into(),
                data_type: DataType::Integer,
                primary_key: true,
                nullable: false,
            },
            Column {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
                nullable: false,
            },
        ],
    };

    assert_eq!(
        schema.primary_key().unwrap().name,
        "id"
    );
}