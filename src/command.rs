#[derive(Debug)]
pub enum Command {
    Set(String, String),
    Get(String),
    Del(String),
    Exit,
    Invalid,
}

impl Command {
    pub fn serialize(&self) -> Vec<u8> {
        let mut bytes= Vec::new();

        match self {
            Command::Set(key, val) => {
                bytes.push(1);

                bytes.extend((key.len() as u32).to_be_bytes());
                bytes.extend((val.len() as u32).to_be_bytes());

                bytes.extend(key.as_bytes());
                bytes.extend(val.as_bytes());
            }

            Command::Del(key) => {
                bytes.push(2);

                bytes.extend((key.len() as u32).to_be_bytes());

                bytes.extend(key.as_bytes());
            }

            _=>{}
        }

        bytes
    }

    pub fn deserialize(bytes:&[u8]) -> Option<(Command, usize)> {
        if bytes.is_empty() {
            return None;
        }

        match bytes[0] {
            1 => {
                if bytes.len() < 9 {
                    return None;
                }
                let key_len= u32::from_be_bytes([
                    bytes[1],
                    bytes[2],
                    bytes[3],
                    bytes[4],
                ]) as usize;

                let val_len= u32::from_be_bytes([
                    bytes[5],
                    bytes[6],
                    bytes[7],
                    bytes[8],
                ]) as usize;

                let total= 9 + key_len + val_len;

                if bytes.len() < total {
                    return None;
                }

                let key= String::from_utf8(
                    bytes[9..9+key_len].to_vec()
                ).ok()?;

                let val= String::from_utf8(
                    bytes[9 + key_len..total].to_vec()
                ).ok()?;

                Some((Command::Set(key, val), total))
            }
            2 => {
                if bytes.len() < 5 {
                    return None;
                }
                let key_len= u32::from_be_bytes([
                    bytes[1],
                    bytes[2],
                    bytes[3],
                    bytes[4],
                ]) as usize;

                let total= 5 + key_len;

                if bytes.len() < total {
                    return None;
                }

                let key= String::from_utf8(
                    bytes[5..total].to_vec()
                ).ok()?;

                Some((Command::Del(key), total))
            }
            _=> None,
        }

    }
}