use std::path::Path;
use std::fs;

use arch_db::{engine::{Engine, Value}, sql::{ast::{self, Assignment, BinaryOperator, ColumnDef, CreateTable, DataType, Delete, Expr, Insert, Select, SelectItem, Statement, Update}, catalog::{self as catalog_mod, Catalog, CatalogDataType, Column, TableSchema}, executor::{Executor, QueryResult}, row::{Row, RowValue}}};

fn clean_all_state() {
    let path = Path::new("storage/temp");
    if path.exists() {
        for entry in fs::read_dir(path).into_iter().flatten() {
            if let Ok(entry) = entry {
                fs::remove_file(entry.path()).ok();
            }
        }
    }
    for entry in fs::read_dir(".").into_iter().flatten() {
        if let Ok(entry) = entry {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("sst_") && name.ends_with(".bin") {
                fs::remove_file(entry.path()).ok();
            }
        }
    }
    fs::remove_file("MANIFEST.log").ok();
    fs::remove_file("MANIFEST.checkpoint").ok();
}

fn make_engine() -> Engine {
    clean_all_state();
    Engine::new()
}



#[test]
fn test_execute_create_table() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor =
        Executor::new(
            &mut catalog,
            &mut engine,
        );

    let stmt = Statement::CreateTable(
        CreateTable {
            table_name: "users".into(),
            columns: vec![
                ColumnDef {
                    name: "id".into(),
                    data_type: ast::DataType::Int,
                },
                ColumnDef {
                    name: "name".into(),
                    data_type: ast::DataType::Text,
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
    let mut engine = make_engine();

    let mut executor =
        Executor::new(
            &mut catalog,
            &mut engine,
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
    let mut engine = make_engine();

    let mut executor =
        Executor::new(
            &mut catalog,
            &mut engine,
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
    let mut engine = make_engine();

    let mut executor = Executor::new(
        &mut catalog,
        &mut engine,
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

    let mut executor = Executor::new(
        &mut catalog,
        &mut engine,
    );

    // First register the table
    let create_stmt = Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: ast::DataType::Int,
            },
            ColumnDef {
                name: "name".into(),
                data_type: ast::DataType::Text,
            },
        ],
    });
    executor.execute(create_stmt);

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
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
            },
        ],
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec!["id".into(), "name".into()],
        values: vec![
            Expr::Number(1),    
            Expr::String("Alice".into()),
        ],
    }));

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        table_name: "users".into(),
        where_clause: Some(
            Expr::Binary {
                left: Box::new(Expr::Identifier("id".into())),
                op: BinaryOperator::Equal,
                right: Box::new(Expr::Number(1)),
            }
        ),
    }));

    assert_eq!(
        result,
        QueryResult::Rows(vec![
            vec![
                "1".into(),
                "Alice".into(),
            ]
        ])
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
            },
            ColumnDef {
                name: "age".into(),
                data_type: DataType::Int,
            },
        ],
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec![
            "username".into(),
            "age".into(),
        ],
        values: vec![
            Expr::String("alice".into()),
            Expr::Number(25),
        ],
    }));

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        table_name: "users".into(),
        where_clause: Some(
            Expr::Binary {
                left: Box::new(
                    Expr::Identifier("username".into())
                ),
                op: BinaryOperator::Equal,
                right: Box::new(
                    Expr::String("alice".into())
                ),
            }
        ),
    }));

    assert_eq!(
        result,
        QueryResult::Rows(vec![
            vec![
                "alice".into(),
                "25".into(),
            ]
        ])
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
        where_clause: Some(
            Expr::Binary {
                left: Box::new(
                    Expr::Identifier("id".into())
                ),
                op: BinaryOperator::Equal,
                right: Box::new(Expr::Number(1)),
            }
        ),
    }));

    assert_eq!(
        result,
        QueryResult::Message(
            "Error: table 'users' does not exist".into()
        )
    );
}

#[test]
fn test_select_missing_row() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
            },
        ],
    }));

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        table_name: "users".into(),
        where_clause: Some(
            Expr::Binary {
                left: Box::new(
                    Expr::Identifier("id".into())
                ),
                op: BinaryOperator::Equal,
                right: Box::new(Expr::Number(1)),
            }
        ),
    }));

    assert_eq!(
        result,
        QueryResult::Rows(vec![])
    );
}

#[test]
fn test_select_without_where_clause() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
            },
        ],
    }));

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        table_name: "users".into(),
        where_clause: None,
    }));

    assert_eq!(
        result,
        QueryResult::Rows(vec![])
    );
}

#[test]
fn test_select_with_invalid_where_clause() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
            },
        ],
    }));

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        table_name: "users".into(),
        where_clause: Some(
            Expr::Identifier("id".into())
        ),
    }));

    assert_eq!(
        result,
        QueryResult::Rows(vec![])
    );
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
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
            },
        ],
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec!["id".into(), "name".into()],
        values: vec![
            Expr::Number(1),
            Expr::String("Alice".into()),
        ],
    }));

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
        QueryResult::Message("1 row deleted".into())
    );

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
        QueryResult::Message(
            "Error: table 'users' does not exist".into()
        )
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
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
            },
        ],
    }));

    let result = executor.execute(Statement::Delete(Delete {
        table_name: "users".into(),
        where_clause: None,
    }));

    assert_eq!(
        result,
        QueryResult::Message(
            "Error: DELETE without WHERE is not supported".into()
        )
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
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
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

    assert_eq!(
        result,
        QueryResult::Message(
            "0 rows deleted".into()
        )
    );
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
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
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

    assert_eq!(
        result,
        QueryResult::Message("1 row deleted".into())
    );

    match executor.engine.get("users:99") {
        Some(Value::Tombstone) => {}
        other => panic!("Expected tombstone, got {:?}", other),
    }
}

#[test]
fn test_update_existing_row() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(
        &mut catalog,
        &mut engine,
    );

    executor.execute(
        Statement::CreateTable(CreateTable {
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
        }),
    );

    executor.execute(
        Statement::Insert(Insert {
            table_name: "users".into(),
            columns: vec![
                "id".into(),
                "name".into(),
            ],
            values: vec![
                Expr::Number(1),
                Expr::String("Alice".into()),
            ],
        }),
    );

    let result = executor.execute(
        Statement::Update(Update {
            table_name: "users".into(),
            assignments: vec![
                Assignment {
                    column: "name".into(),
                    value: Expr::String("Bob".into()),
                }
            ],
            where_clause: Some(
                Expr::Binary {
                    left: Box::new(
                        Expr::Identifier("id".into())
                    ),
                    op: BinaryOperator::Equal,
                    right: Box::new(
                        Expr::Number(1)
                    ),
                }
            ),
        }),
    );

    assert_eq!(
        result,
        QueryResult::Message(
            "1 row updated".into()
        )
    );

    let key = "users:1";

    let stored = executor
        .engine
        .get(key)
        .unwrap();

    match stored {
        Value::Data(data) => {
            let row = Row::deserialize(&data);

            assert_eq!(
                row.get("name"),
                Some(&RowValue::Text("Bob".into()))
            );

            assert_eq!(
                row.get("id"),
                Some(&RowValue::Integer(1))
            );
        }

        _ => panic!("expected row"),
    }
}

#[test]
fn test_update_non_existing_row() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(
        &mut catalog,
        &mut engine,
    );

    executor.execute(
        Statement::CreateTable(CreateTable {
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
        }),
    );

    let result = executor.execute(
        Statement::Update(Update {
            table_name: "users".into(),
            assignments: vec![
                Assignment {
                    column: "name".into(),
                    value: Expr::String("Bob".into()),
                }
            ],
            where_clause: Some(
                Expr::Binary {
                    left: Box::new(
                        Expr::Identifier("id".into())
                    ),
                    op: BinaryOperator::Equal,
                    right: Box::new(
                        Expr::Number(99)
                    ),
                }
            ),
        }),
    );

    assert_eq!(
        result,
        QueryResult::Message(
            "Error: row not found".into()
        )
    );
}

#[test]
fn test_update_multiple_columns() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(
        &mut catalog,
        &mut engine,
    );

    executor.execute(
        Statement::CreateTable(CreateTable {
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
                ColumnDef {
                    name: "city".into(),
                    data_type: DataType::Text,
                },
            ],
        }),
    );

    executor.execute(
        Statement::Insert(Insert {
            table_name: "users".into(),
            columns: vec![
                "id".into(),
                "name".into(),
                "city".into(),
            ],
            values: vec![
                Expr::Number(1),
                Expr::String("Alice".into()),
                Expr::String("London".into()),
            ],
        }),
    );

    let result = executor.execute(
        Statement::Update(Update {
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
            where_clause: Some(
                Expr::Binary {
                    left: Box::new(
                        Expr::Identifier("id".into())
                    ),
                    op: BinaryOperator::Equal,
                    right: Box::new(
                        Expr::Number(1)
                    ),
                }
            ),
        }),
    );

    assert_eq!(
        result,
        QueryResult::Message(
            "1 row updated".into()
        )
    );

    let stored = executor
        .engine
        .get("users:1")
        .unwrap();

    match stored {
        Value::Data(data) => {
            let row = Row::deserialize(&data);

            assert_eq!(
                row.get("name"),
                Some(&RowValue::Text("Bob".into()))
            );

            assert_eq!(
                row.get("city"),
                Some(&RowValue::Text("Paris".into()))
            );
        }

        _ => panic!("expected row"),
    }
}

#[test]
fn test_update_does_not_change_primary_key() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();

    let mut executor = Executor::new(
        &mut catalog,
        &mut engine,
    );

    executor.execute(
        Statement::CreateTable(CreateTable {
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
        }),
    );

    executor.execute(
        Statement::Insert(Insert {
            table_name: "users".into(),
            columns: vec![
                "id".into(),
                "name".into(),
            ],
            values: vec![
                Expr::Number(1),
                Expr::String("Alice".into()),
            ],
        }),
    );

    let result = executor.execute(
        Statement::Update(Update {
            table_name: "users".into(),
            assignments: vec![
                Assignment {
                    column: "id".into(),
                    value: Expr::Number(2),
                }
            ],
            where_clause: Some(
                Expr::Binary {
                    left: Box::new(
                        Expr::Identifier("id".into())
                    ),
                    op: BinaryOperator::Equal,
                    right: Box::new(
                        Expr::Number(1)
                    ),
                }
            ),
        }),
    );

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
            ColumnDef { name: "id".into(), data_type: DataType::Int },
            ColumnDef { name: "name".into(), data_type: DataType::Text },
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
            ColumnDef { name: "id".into(), data_type: DataType::Int },
            ColumnDef { name: "name".into(), data_type: DataType::Text },
        ],
    }));

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        table_name: "users".into(),
        where_clause: None,
    }));

    assert_eq!(
        result,
        QueryResult::Rows(vec![])
    );
}

#[test]
fn test_select_all_single_row() {
    let mut catalog = Catalog::new();
    let mut engine = make_engine();
    let mut executor = Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef { name: "id".into(), data_type: DataType::Int },
            ColumnDef { name: "name".into(), data_type: DataType::Text },
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
    }));

    assert_eq!(
        result,
        QueryResult::Rows(vec![
            vec!["1".into(), "Alice".into()],
        ])
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
            ColumnDef { name: "id".into(), data_type: DataType::Int },
            ColumnDef { name: "name".into(), data_type: DataType::Text },
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
            ColumnDef { name: "id".into(), data_type: DataType::Int },
            ColumnDef { name: "name".into(), data_type: DataType::Text },
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
    }));

    assert_eq!(
        result,
        QueryResult::Rows(vec![
            vec!["2".into(), "Bob".into()],
        ])
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
            ColumnDef { name: "id".into(), data_type: DataType::Int },
            ColumnDef { name: "name".into(), data_type: DataType::Text },
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
    }));

    assert_eq!(
        result,
        QueryResult::Rows(vec![
            vec!["1".into(), "Alice Smith".into()],
        ])
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
            ColumnDef { name: "id".into(), data_type: DataType::Int },
            ColumnDef { name: "name".into(), data_type: DataType::Text },
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
    }));

    assert_eq!(
        result,
        QueryResult::Rows(vec![
            vec!["1".into(), "Charlie".into()],
        ])
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
            ColumnDef { name: "id".into(), data_type: DataType::Int },
            ColumnDef { name: "name".into(), data_type: DataType::Text },
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
            ColumnDef { name: "id".into(), data_type: DataType::Int },
            ColumnDef { name: "name".into(), data_type: DataType::Text },
        ],
    }));

    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "products".into(),
        columns: vec![
            ColumnDef { name: "id".into(), data_type: DataType::Int },
            ColumnDef { name: "name".into(), data_type: DataType::Text },
        ],
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec!["id".into(), "name".into()],
        values: vec![
            Expr::Number(1),
            Expr::String("Alice".into()),
        ],
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "products".into(),
        columns: vec!["id".into(), "name".into()],
        values: vec![
            Expr::Number(1),
            Expr::String("Laptop".into()),
        ],
    }));

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        table_name: "users".into(),
        where_clause: None,
    }));

    assert_eq!(
        result,
        QueryResult::Rows(vec![
            vec!["1".into(), "Alice".into()]
        ])
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
            ColumnDef { name: "id".into(), data_type: DataType::Int },
            ColumnDef { name: "name".into(), data_type: DataType::Text },
        ],
    }));

    // Enough inserts to flush the memtable.
    for i in 0..50 {
        executor.execute(Statement::Insert(Insert {
            table_name: "users".into(),
            columns: vec!["id".into(), "name".into()],
            values: vec![
                Expr::Number(i),
                Expr::String(format!("User{}", i)),
            ],
        }));
    }

    // These rows should remain only in the memtable.
    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec!["id".into(), "name".into()],
        values: vec![
            Expr::Number(100),
            Expr::String("Extra1".into()),
        ],
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec!["id".into(), "name".into()],
        values: vec![
            Expr::Number(101),
            Expr::String("Extra2".into()),
        ],
    }));

    let result = executor.execute(Statement::Select(Select {
        columns: vec![SelectItem::Wildcard],
        table_name: "users".into(),
        where_clause: None,
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
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
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

    let result = executor.execute(Statement::Select(Select {
        columns: vec![
            SelectItem::Column("name".into()),
        ],
        table_name: "users".into(),
        where_clause: Some(
            Expr::Binary {
                left: Box::new(
                    Expr::Identifier("id".into())
                ),
                op: BinaryOperator::Equal,
                right: Box::new(
                    Expr::Number(1)
                ),
            }
        ),
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
    let mut executor =
        Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {
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
    }));

    for i in 1..=5 {

        executor.execute(Statement::Insert(Insert {

            table_name: "users".into(),

            columns: vec![
                "id".into(),
                "name".into(),
            ],

            values: vec![
                Expr::Number(i),
                Expr::String(format!("User{}", i)),
            ],
        }));
    }

    let result =
        executor.execute(Statement::Select(Select {

            columns: vec![
                SelectItem::Wildcard,
            ],

            table_name: "users".into(),

            where_clause: None,
        }));

    match result {

        QueryResult::Rows(rows) => {

            assert_eq!(rows.len(), 5);

            assert_eq!(
                rows[0],
                vec!["1", "User1"]
            );

            assert_eq!(
                rows[4],
                vec!["5", "User5"]
            );
        }

        _ => panic!("Expected rows"),
    }
}

#[test]
fn test_select_with_non_primary_key_where() {

    let mut catalog = Catalog::new();
    let mut engine = make_engine();
    let mut executor =
        Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {

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

    let result =
        executor.execute(Statement::Select(Select {

            columns: vec![
                SelectItem::Wildcard,
            ],

            table_name: "users".into(),

            where_clause: Some(
                Expr::Binary {

                    left: Box::new(
                        Expr::Identifier("name".into())
                    ),

                    op: BinaryOperator::Equal,

                    right: Box::new(
                        Expr::String("Bob".into())
                    ),
                }
            ),
        }));

    match result {

        QueryResult::Rows(rows) => {

            assert_eq!(rows.len(), 1);

            assert_eq!(
                rows[0],
                vec!["2", "Bob"]
            );
        }

        _ => panic!("Expected rows"),
    }
}

#[test]
fn test_select_after_flush() {

    let mut catalog = Catalog::new();
    let mut engine = make_engine();
    let mut executor =
        Executor::new(&mut catalog, &mut engine);

    executor.execute(Statement::CreateTable(CreateTable {

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
    }));

    for i in 0..50 {

        executor.execute(Statement::Insert(Insert {

            table_name: "users".into(),

            columns: vec![
                "id".into(),
                "name".into(),
            ],

            values: vec![
                Expr::Number(i),
                Expr::String(format!("User{}", i)),
            ],
        }));
    }

    let result =
        executor.execute(Statement::Select(Select {

            columns: vec![
                SelectItem::Wildcard,
            ],

            table_name: "users".into(),

            where_clause: None,
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
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
            },
        ],
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec!["id".into(), "name".into()],
        values: vec![
            Expr::Number(1),
            Expr::String("Alice".into()),
        ],
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
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
            },
        ],
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec!["id".into(), "name".into()],
        values: vec![
            Expr::Number(1),
            Expr::String("Alice".into()),
        ],
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec!["id".into(), "name".into()],
        values: vec![
            Expr::Number(2),
            Expr::String("Bob".into()),
        ],
    }));

    executor.execute(Statement::Insert(Insert {
        table_name: "users".into(),
        columns: vec!["id".into(), "name".into()],
        values: vec![
            Expr::Number(3),
            Expr::String("Bob".into()),
        ],
    }));

    executor.execute(Statement::Delete(Delete {
        table_name: "users".into(),
        where_clause: Some(
            Expr::Binary {
                left: Box::new(
                    Expr::Identifier("name".into()),
                ),
                op: BinaryOperator::Equal,
                right: Box::new(
                    Expr::String("Bob".into()),
                ),
            },
        ),
    }));

    let result = executor.execute(Statement::Select(Select {
        columns: vec![
            SelectItem::Wildcard,
        ],
        table_name: "users".into(),
        where_clause: None,
    }));

    match result {
        QueryResult::Rows(rows) => {
            assert_eq!(rows.len(), 1);
            assert_eq!(
                rows[0],
                vec!["1", "Alice"]
            );
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
            },
            ColumnDef {
                name: "age".into(),
                data_type: DataType::Int,
            },
        ],
    }));

    for i in 1..=5 {
        executor.execute(Statement::Insert(Insert {
            table_name: "users".into(),
            columns: vec![
                "id".into(),
                "age".into(),
            ],
            values: vec![
                Expr::Number(i),
                Expr::Number(i * 10),
            ],
        }));
    }

    executor.execute(Statement::Delete(Delete {
        table_name: "users".into(),
        where_clause: Some(
            Expr::Binary {
                left: Box::new(
                    Expr::Identifier("age".into()),
                ),
                op: BinaryOperator::GreaterThan,
                right: Box::new(
                    Expr::Number(20),
                ),
            },
        ),
    }));

    let result = executor.execute(Statement::Select(Select {
        columns: vec![
            SelectItem::Wildcard,
        ],
        table_name: "users".into(),
        where_clause: None,
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
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
            },
        ],
    }));

    for i in 1..=3 {
        executor.execute(Statement::Insert(Insert {
            table_name: "users".into(),
            columns: vec!["id".into()],
            values: vec![
                Expr::Number(i),
            ],
        }));
    }

    executor.execute(Statement::Delete(Delete {
        table_name: "users".into(),
        where_clause: Some(
            Expr::Binary {
                left: Box::new(
                    Expr::Identifier("id".into()),
                ),
                op: BinaryOperator::GreaterThan,
                right: Box::new(
                    Expr::Number(100),
                ),
            },
        ),
    }));

    let result = executor.execute(Statement::Select(Select {
        columns: vec![
            SelectItem::Wildcard,
        ],
        table_name: "users".into(),
        where_clause: None,
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
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
            },
        ],
    }));

    // Insert enough rows to trigger a flush.
    for i in 0..50 {
        executor.execute(Statement::Insert(Insert {
            table_name: "users".into(),
            columns: vec!["id".into(), "name".into()],
            values: vec![
                Expr::Number(i),
                Expr::String(format!("User{}", i)),
            ],
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
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
            },
        ],
    }));

    // These rows should flush.
    for i in 1..=40 {
        executor.execute(Statement::Insert(Insert {
            table_name: "users".into(),
            columns: vec!["id".into(), "name".into()],
            values: vec![
                Expr::Number(i),
                Expr::String(format!("User{}", i)),
            ],
        }));
    }

    // These remain in the memtable.
    for i in 41..=45 {
        executor.execute(Statement::Insert(Insert {
            table_name: "users".into(),
            columns: vec!["id".into(), "name".into()],
            values: vec![
                Expr::Number(i),
                Expr::String(format!("User{}", i)),
            ],
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
        (BinaryOperator::LessThan, 3, 3, vec![3,4,5]),
        (BinaryOperator::LessThanOrEqual, 3, 2, vec![4,5]),
        (BinaryOperator::NotEqual, 3, 1, vec![3]),
        (BinaryOperator::Equal,3,4,vec![1, 2, 4, 5]),
    ];

    for (op, value, expected_remaining, expected_ids) in operators {

        let mut catalog = Catalog::new();
        let mut engine = make_engine();
        let mut executor = Executor::new(&mut catalog, &mut engine);

        executor.execute(Statement::CreateTable(CreateTable {
            table_name: "users".into(),
            columns: vec![
                ColumnDef {
                    name: "id".into(),
                    data_type: DataType::Int,
                },
            ],
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
        }));

        match result {
            QueryResult::Rows(rows) => {
                assert_eq!(rows.len(), expected_remaining);

                let  ids: Vec<i64> = rows
                    .iter()
                    .map(|r| r[0].parse::<i64>().unwrap())
                    .collect();
                println!("ids: {:?} expected_ids: {:?}", ids, expected_ids);
                assert_eq!(ids, expected_ids);
            }

            _ => panic!("Expected rows"),
        }
    }
}

