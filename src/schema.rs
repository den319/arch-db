use std::{collections::HashMap, fs::File, io::{Write}};


#[derive(Debug, Clone, PartialEq)]
pub enum ColumnType {
    Int,
    Text, 
    Bool,
}

#[derive(Debug, Clone)]
pub struct Column {
    pub name: String,
    pub column_type: ColumnType,
}

#[derive(Debug, Clone)]
pub struct Table {
    pub name: String,
    pub columns: Vec<Column>,
}

#[derive(Debug)]
pub struct Catalog {
    tables: HashMap<String, Table>,
    path: String,
}

impl Catalog {
    pub fn new(path: &str) -> Self {
        Self {
            tables: HashMap::new(),
            path: path.to_string(),
        }
    }

    pub fn create_table(&mut self, table: Table) -> Result<(), String> {
        if self.tables.contains_key(&table.name) {
            return Err(format!("Table '{}' already exists!", table.name));
        }

        self.tables.insert(table.name.clone(), table);

        self.save().map_err(|e| e.to_string())?;

        Ok(())
    }

    pub fn get_table(&self, name:&str) -> Option<&Table> {
        self.tables.get(name)
    }

    pub fn list_tables(&self) -> Vec<&Table> {
        self.tables.values().collect()
    }

    pub fn save(&self) -> Result<()> {
        let mut file= File::create(&self.path)?;

        for table in self.tables.values() {
            file.write_all(table.serialize().as_bytes())?;
        }

        file.sync_all()?;

        Ok(())
    }


}


impl ColumnType {
    pub fn serialize(&self) -> &'static str {
        match self {
            ColumnType::Int => "INT",
            ColumnType::Text => "TEXT",
            ColumnType::Bool => "BOOL",
        }
    }

    pub fn deserialize(s: &str) -> Option<Self> {
        match s {
            "INT" => Some(ColumnType::Int),
            "TEXT" => Some(ColumnType::Text),
            "BOOL" => Some(ColumnType::Bool),
            _ => None,
        }
    }


}


impl Table {
    pub fn serialize(&self) -> String {
        let mut output= String::new();

        output.push_str(&format!("TABLE|{}\n", self.name));

        for column in &self.columns {
            output.push_str(&format!("COLUMN|{}|{}\n", column.name, column.column_type.serialize()));
        }

        output.push_str("END\n");
        output
    }


}
