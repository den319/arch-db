use std::{fmt::Display, io::Error};

#[derive(Debug)]
pub enum DatabaseError {
    Io(Error),
    InvalidCommand,
}

impl Display for DatabaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DatabaseError::InvalidCommand => write!(f, "invalid command!"),
            DatabaseError::Io(err) => write!(f, "IO error: {}", err),
        }
    }

}

impl From<Error> for DatabaseError {
    fn from(err: Error) -> Self {
        DatabaseError::Io(err)
    }
}

pub type Result<T> = std::result::Result<T, DatabaseError>;