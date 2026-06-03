use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(test)]
pub fn unique_file(prefix: &str, extension: &str) -> String {
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    format!("{}_{}.{}", prefix, id, extension)
}