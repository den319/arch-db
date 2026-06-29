use arch_db::sql::row::{Row, RowError, RowValue};

#[test]
fn test_empty_row() {
    let row = Row::new();

    assert!(row.values.is_empty());

    assert_eq!(row.serialize(), "");

    assert_eq!(Row::deserialize(""), row);
}

#[test]
fn test_insert_and_get() {
    let mut row = Row::new();

    row.insert("id", RowValue::Integer(1));
    row.insert("name", RowValue::Text("Alice".into()));

    assert_eq!(
        row.get("id"),
        Some(&RowValue::Integer(1))
    );

    assert_eq!(
        row.get("name"),
        Some(&RowValue::Text("Alice".into()))
    );

    assert_eq!(
        row.get("age"),
        None
    );
}

#[test]
fn test_integer_serialization() {
    let mut row = Row::new();

    row.insert("id", RowValue::Integer(42));

    let serialized = row.serialize();

    assert_eq!(
        serialized,
        "id=i:42"
    );
}

#[test]
fn test_deserialize_integer() {
    let row = Row::deserialize("id=i:10");

    assert_eq!(
        row.get("id"),
        Some(&RowValue::Integer(10))
    );
}

#[test]
fn test_deserialize_text() {
    let row = Row::deserialize("name=t:Alice");

    assert_eq!(
        row.get("name"),
        Some(&RowValue::Text("Alice".into()))
    );
}

#[test]
fn test_mixed_row() {
    let mut row = Row::new();

    row.insert("id", RowValue::Integer(1));
    row.insert("name", RowValue::Text("Alice".into()));
    row.insert("city", RowValue::Text("London".into()));

    let serialized = row.serialize();

    let parsed = Row::deserialize(&serialized);

    assert_eq!(parsed, row);
}

#[test]
fn test_serialization_is_sorted() {
    let mut row = Row::new();

    row.insert("z", RowValue::Integer(1));
    row.insert("a", RowValue::Integer(2));
    row.insert("m", RowValue::Integer(3));

    assert_eq!(
        row.serialize(),
        "a=i:2|m=i:3|z=i:1"
    );
}

#[test]
fn test_round_trip() {
    let mut row = Row::new();

    row.insert("id", RowValue::Integer(100));
    row.insert("name", RowValue::Text("Dharmik".into()));
    row.insert("city", RowValue::Text("Rajkot".into()));

    let serialized = row.serialize();

    let parsed = Row::deserialize(&serialized);

    assert_eq!(row, parsed);
}

#[test]
fn test_overwrite_column() {
    let mut row = Row::new();

    row.insert("id", RowValue::Integer(1));
    row.insert("id", RowValue::Integer(2));

    assert_eq!(
        row.get("id"),
        Some(&RowValue::Integer(2))
    );
}

#[test]
fn test_from_columns() {
    let row = Row::from_columns(
        vec![
            "id".into(),
            "name".into(),
        ],
        vec![
            RowValue::Integer(1),
            RowValue::Text("Alice".into()),
        ],
    )
    .unwrap();

    assert_eq!(
        row.get("id"),
        Some(&RowValue::Integer(1))
    );

    assert_eq!(
        row.get("name"),
        Some(&RowValue::Text("Alice".into()))
    );
}

#[test]
fn test_from_columns_count_mismatch() {
    let result = Row::from_columns(
        vec![
            "id".into(),
            "name".into(),
        ],
        vec![
            RowValue::Integer(1),
        ],
    );

    assert_eq!(
        result,
        Err(RowError::ColumnValueCountMismatch)
    );
}

#[test]
fn test_from_columns_empty() {
    let row = Row::from_columns(
        vec![],
        vec![],
    )
    .unwrap();

    assert!(row.values.is_empty());
}