#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    CreateTable(CreateTable),

    Insert(Insert),

    Select(Select),

    Delete(Delete),

    Update(Update),
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreateTable {
    pub table_name: String,
    pub columns: Vec<ColumnDef>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ColumnDef {
    pub name: String,
    pub data_type: DataType,
    pub primary_key: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DataType {
    Int,
    Text,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Insert {
    pub table_name: String,
    pub columns: Vec<String>,
    pub values: Vec<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Select {
    pub columns: Vec<SelectItem>,
    pub table_name: String,
    pub where_clause: Option<Expr>,

    pub order_by: Option<OrderBy>,

    pub limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Delete {
    pub table_name: String,
    pub where_clause: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Update {
    pub table_name: String,
    pub assignments: Vec<Assignment>,
    pub where_clause: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Assignment {
    pub column: String,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SelectItem {
    Wildcard,
    Column(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Identifier(String),

    Number(i64),

    String(String),

    Binary {
        left: Box<Expr>,
        op: BinaryOperator,
        right: Box<Expr>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOperator {
    Equal,
    NotEqual,

    GreaterThan,
    GreaterThanOrEqual,

    LessThan,
    LessThanOrEqual,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderBy {
    pub column: String,
    pub direction: OrderDirection,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OrderDirection {
    Asc,
    Desc,
}