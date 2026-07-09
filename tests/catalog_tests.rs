use arch_db::{engine::{Engine, Value}, sql::{ast::{ColumnDef, CreateTable, DataType, Statement}, catalog::{Catalog, CatalogDataType, Column, TableSchema}, executor::Executor}};

#[test]
fn create_table() {
    let mut catalog = Catalog::new();

    let schema = TableSchema {
        name: "users".into(),
        columns: vec![
            Column {
                name: "id".into(),
                data_type: CatalogDataType::Integer,
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

    assert_eq!(
        schema.primary_key().unwrap().name,
        "id"
    );
}

#[test]
fn test_schema_serialization_roundtrip() {

    let schema = TableSchema {

        name: "users".into(),

        columns: vec![
            Column {
                name: "id".into(),
                data_type: CatalogDataType::Integer,
                nullable: false,
                primary_key: false,
            },
            Column {
                name: "name".into(),
                data_type: CatalogDataType::Text,
                nullable: false,
                primary_key: false,
            },
        ],
    };

    let serialized = schema.serialize();

    let restored = TableSchema::deserialize(&serialized);

    assert_eq!(schema, restored);
}

#[test]
fn test_empty_schema_serialization() {

    let schema = TableSchema {

        name: "empty".into(),

        columns: vec![],
    };

    let serialized = schema.serialize();

    let restored = TableSchema::deserialize(&serialized);

    assert_eq!(schema, restored);
}

#[test]
fn test_create_table_persists_schema() {

    let mut catalog = Catalog::new();
    let mut engine = Engine::new();

    let mut executor =
        Executor::new(&mut catalog, &mut engine);

    executor.execute(
        Statement::CreateTable(
            CreateTable {
                table_name: "users".into(),
                columns: vec![
                    ColumnDef {
                        name: "id".into(),
                        data_type: DataType::Int,
                        primary_key:false,
                    },
                    ColumnDef {
                        name: "name".into(),
                        data_type: DataType::Text,
                        primary_key: false,
                    },
                ],
            }
        )
    );

    let value = engine.get("__schema__:users");

    match value {

        Some(Value::Data(serialized)) => {

            let schema =
                TableSchema::deserialize(&serialized);

            assert_eq!(schema.name, "users");

            assert_eq!(schema.columns.len(), 2);
        }

        _ => panic!("Schema not persisted"),
    }
}

#[test]
fn test_catalog_loads_schema_from_engine() {

    let mut engine = Engine::new();

    let schema = TableSchema {

        name: "users".into(),

        columns: vec![
            Column {
                name: "id".into(),
                data_type: CatalogDataType::Integer,
                nullable: false,
                primary_key: false
            },
            Column {
                name: "name".into(),
                data_type: CatalogDataType::Text,
                nullable: false,
                primary_key: false
            },
        ],
    };

    engine.put(
        "__schema__:users".into(),
        schema.serialize(),
    ).unwrap();

    let mut catalog = Catalog::new();

    catalog
        .load_from_engine(&mut engine)
        .unwrap();

    assert!(catalog.table("users").is_some());

    let restored =
        catalog.table("users").unwrap();

    assert_eq!(restored.name, "users");

    assert_eq!(restored.columns.len(), 2);
}