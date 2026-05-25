use std::{fs::File, io::Write};

use crate::error::Result;


pub fn write_sstable(
    path:&str, 
    data: &[(String, String)]
) -> Result<()> {
    let mut file= File::create(path)?;
    
    for (key, val) in data  {
        file.write_all(&(key.len() as u32).to_be_bytes())?;
        file.write_all(&(val.len() as u32).to_be_bytes())?;
        file.write_all(key.as_bytes())?;
        file.write_all(val.as_bytes())?;
    }

    Ok(())
}