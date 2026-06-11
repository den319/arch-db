use std::time::{SystemTime, UNIX_EPOCH};

pub fn unique_file(prefix: &str, extension: &str) -> String {
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    format!("{}_{}.{}", prefix, id, extension)
}

pub fn unique_dir(prefix: &str) -> String {

    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    format!(
        "{}_{}",
        prefix,
        id
    )
}