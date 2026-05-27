use crate::command::Command;

pub fn parse(input: &str) -> Command {
    let parts:Vec<&str>= input.trim().split_whitespace().collect();

    if parts.is_empty() {
        return Command::Invalid;
    }

    match parts[0].to_uppercase().as_str() {
        "SET" if parts.len() >= 3 => {
            Command::Set(parts[1].to_string(), parts[2..].join(" "))
        }

        "GET" if parts.len() == 2 => {
            Command::Get(parts[1].to_string())
        }

        "DEL" if parts.len() == 2 => {
            Command::Del(parts[1].to_string())
        }

        "EXIT" => Command::Exit,
        "COMPACT" => Command::Compact,
        
        _ => Command::Invalid,
    }
}