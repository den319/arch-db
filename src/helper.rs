use std::time::{SystemTime, UNIX_EPOCH};

use crate::engine::Value;

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

pub fn is_sorted(data: &[(String, Value)]) -> bool {
    data.windows(2)
        .all(|w| w[0].0 < w[1].0)
}

