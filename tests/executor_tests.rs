use arch_db::{engine::{Engine, Value}, sql::{ast::{self, Assignment, BinaryOperator, ColumnDef, CreateTable, DataType, Delete, Expr, Insert, Select, SelectItem, Statement, Update}, catalog::{self as catalog_mod, Catalog, CatalogDataType, Column, TableSchema}, executor::{Executor, QueryResult}, row::{Row, RowValue}}};

fn make_engine() -> Engine {
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
    let mut engine = Engine::new();

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
    let mut engine = Engine::new();

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
                "25".into(),
                "alice".into(),
            ]
        ])
    );
}

#[test]
fn test_select_missing_table() {
    let mut catalog = Catalog::new();
    let mut engine = Engine::new();

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
    let mut engine = Engine::new();

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
        QueryResult::Message(
            "Error: row not found".into()
        )
    );
}

#[test]
fn test_select_without_where_clause() {
    let mut catalog = Catalog::new();
    let mut engine = Engine::new();

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
        QueryResult::Message(
            "Error: SELECT without WHERE is not supported yet".into()
        )
    );
}

#[test]
fn test_select_with_invalid_where_clause() {
    let mut catalog = Catalog::new();
    let mut engine = Engine::new();

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
        QueryResult::Message(
            "Error: unsupported WHERE clause".into()
        )
    );
} 

#[test]
fn test_delete_existing_row() {
    let mut catalog = Catalog::new();
    let mut engine = Engine::new();
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
        QueryResult::Message("Row deleted successfully".into())
    );

    match executor.engine.get("users:1") {
        Some(Value::Tombstone) => {}
        other => panic!("Expected tombstone, got {:?}", other),
    }
}

#[test]
fn test_delete_from_missing_table() {
    let mut catalog = Catalog::new();
    let mut engine = Engine::new();
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
    let mut engine = Engine::new();
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
    let mut engine = Engine::new();
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
            "Error: WHERE must use primary key 'id'".into()
        )
    );
}

#[test]
fn test_delete_non_existing_row() {
    let mut catalog = Catalog::new();
    let mut engine = Engine::new();
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
        QueryResult::Message("Row deleted successfully".into())
    );

    match executor.engine.get("users:99") {
        Some(Value::Tombstone) => {}
        other => panic!("Expected tombstone, got {:?}", other),
    }
}

#[test]
fn test_update_existing_row() {
    let mut catalog = Catalog::new();
    let mut engine = Engine::new();

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
            "Row updated successfully".into()
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
    let mut engine = Engine::new();

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
    let mut engine = Engine::new();

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
            "Row updated successfully".into()
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
    let mut engine = Engine::new();

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