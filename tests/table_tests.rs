use arch_db::sql::{catalog::{Column, DataType, TableSchema}, row::{Row, Value}, table::Table};

#[test]
fn test_primary_key_integer() {
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

    let table = Table::new(schema);

    let row = Row::from_columns(
        vec!["id".into(), "name".into()],
        vec![
            Value::Integer(1),
            Value::Text("Alice".into()),
        ],
    )
    .unwrap();

    assert_eq!(
        table.primary_key_value(&row),
        Some("1".into())
    );
}

#[test]
fn test_primary_key_text() {
    let schema = TableSchema {
        name: "users".into(),
        columns: vec![
            Column {
                name: "username".into(),
                data_type: DataType::Text,
                primary_key: true,
                nullable: false,
            },
        ],
    };

    let table = Table::new(schema);

    let row = Row::from_columns(
        vec!["username".into()],
        vec![
            Value::Text("dharmik".into()),
        ],
    )
    .unwrap();

    assert_eq!(
        table.primary_key_value(&row),
        Some("dharmik".into())
    );
}

#[test]
fn test_storage_key() {
    let schema = TableSchema {
        name: "users".into(),
        columns: vec![
            Column {
                name: "id".into(),
                data_type: DataType::Integer,
                primary_key: true,
                nullable: false,
            },
        ],
    };

    let table = Table::new(schema);

    let row = Row::from_columns(
        vec!["id".into()],
        vec![
            Value::Integer(42),
        ],
    )
    .unwrap();

    assert_eq!(
        table.storage_key(&row),
        Some("users:42".into())
    );
}

#[test]
fn test_no_primary_key() {
    let schema = TableSchema {
        name: "users".into(),
        columns: vec![
            Column {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
                nullable: false,
            },
        ],
    };

    let table = Table::new(schema);

    let row = Row::from_columns(
        vec!["name".into()],
        vec![
            Value::Text("Alice".into()),
        ],
    )
    .unwrap();

    assert_eq!(
        table.primary_key_value(&row),
        None
    );
}

#[test]
fn test_missing_primary_key_value() {
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

    let table = Table::new(schema);

    let row = Row::from_columns(
        vec!["name".into()],
        vec![
            Value::Text("Alice".into()),
        ],
    )
    .unwrap();

    assert_eq!(
        table.primary_key_value(&row),
        None
    );
}

#[test]
fn test_storage_key_without_primary_key() {
    let schema = TableSchema {
        name: "users".into(),
        columns: vec![
            Column {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
                nullable: false,
            },
        ],
    };

    let table = Table::new(schema);

    let row = Row::from_columns(
        vec!["name".into()],
        vec![
            Value::Text("Alice".into()),
        ],
    )
    .unwrap();

    assert_eq!(
        table.storage_key(&row),
        None
    );
}

#[test]
fn test_storage_key_from_integer_primary_key() {
    let schema = TableSchema {
        name: "users".to_string(),
        columns: vec![
            Column {
                name: "id".to_string(),
                data_type: DataType::Integer,
                primary_key: true,
                nullable: false,
            },
            Column {
                name: "name".to_string(),
                data_type: DataType::Text,
                primary_key: false,
                nullable: true,
            },
        ],
    };

    let table = Table::new(schema);

    let key = table.storage_key_from_primary_key(
        &Value::Integer(42),
    );

    assert_eq!(
        key,
        Some("users:42".to_string())
    );
}

#[test]
fn test_storage_key_from_text_primary_key() {
    let schema = TableSchema {
        name: "users".to_string(),
        columns: vec![
            Column {
                name: "username".to_string(),
                data_type: DataType::Text,
                primary_key: true,
                nullable: false,
            },
        ],
    };

    let table = Table::new(schema);

    let key = table.storage_key_from_primary_key(
        &Value::Text("alice".into()),
    );

    assert_eq!(
        key,
        Some("users:alice".to_string())
    );
}

#[test]
fn test_storage_key_without_primary_key_returns_none() {
    let schema = TableSchema {
        name: "users".to_string(),
        columns: vec![
            Column {
                name: "id".to_string(),
                data_type: DataType::Integer,
                primary_key: false,
                nullable: true,
            },
        ],
    };

    let table = Table::new(schema);

    let key = table.storage_key_from_primary_key(
        &Value::Integer(1),
    );

    assert_eq!(key, None);
}