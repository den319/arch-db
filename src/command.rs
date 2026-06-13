use crc32fast::hash;

#[derive(Debug)]
pub enum Command {
    Set(String, String),
    Get(String),
    Del(String),
    Exit,
    Invalid,
    Compact,
    Scan(String, String),
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

        let checksum= hash(&bytes);
        let mut result= Vec::new();

        result.extend(checksum.to_be_bytes());

        result.extend(bytes);

        result
    }

    pub fn deserialize(bytes:&[u8]) -> Option<(Command, usize)> {
        const CHECKSUM_SIZE: usize = 4;
        const TYPE_SIZE: usize = 1;
        const LEN_SIZE: usize = 4;

        if bytes.len() <  CHECKSUM_SIZE + TYPE_SIZE {
            return None;
        }

        let stored_checksum= u32::from_be_bytes([
            bytes[0],
            bytes[1],
            bytes[2],
            bytes[3],
        ]);


        let payload_start= 4;

        match bytes[payload_start] {
            1 => {
                if bytes.len() < 9 {
                    return None;
                }
                let key_len= u32::from_be_bytes([
                    bytes[payload_start + 1],
                    bytes[payload_start + 2],
                    bytes[payload_start + 3],
                    bytes[payload_start + 4],
                ]) as usize;

                let val_len= u32::from_be_bytes([
                    bytes[payload_start + 5],
                    bytes[payload_start + 6],
                    bytes[payload_start + 7],
                    bytes[payload_start + 8],
                ]) as usize;

                /*
                    4 checksum
                    + 1 type
                    + 4 key_len
                    + 4 val_len
                    = 13
                 */

                let initial_bytes= CHECKSUM_SIZE + TYPE_SIZE + LEN_SIZE + LEN_SIZE;
                let total= initial_bytes  + key_len + val_len;

                if bytes.len() < total {
                    return None;
                }

                let key= String::from_utf8(
                    bytes[initial_bytes..initial_bytes+key_len].to_vec()
                ).ok()?;

                let val= String::from_utf8(
                    bytes[initial_bytes + key_len..total].to_vec()
                ).ok()?;

                let payload= &bytes[payload_start..total];

                let computed_checksum= hash(payload);

                if computed_checksum != stored_checksum {
                    println!("CORRUPTED WAL RECORD DETECTED");

                    return None;
                }

                Some((Command::Set(key, val), total))
            }
            2 => {
                if bytes.len() < payload_start + 5 {
                    return None;
                }

                /*
                    4 checksum
                    1 type
                    4 key_len
                    = 9 bytes header
                */
                let key_len= u32::from_be_bytes([
                    bytes[payload_start + 1],
                    bytes[payload_start + 2],
                    bytes[payload_start + 3],
                    bytes[payload_start + 4],
                ]) as usize;

                let initial_bytes= CHECKSUM_SIZE + TYPE_SIZE + LEN_SIZE;

                let total= initial_bytes + key_len;

                if bytes.len() < total {
                    return None;
                }

                let key= String::from_utf8(
                    bytes[initial_bytes..total].to_vec()
                ).ok()?;
                
                let payload= &bytes[payload_start..total];

                let computed_checksum= hash(payload);

                if computed_checksum != stored_checksum {
                    println!("CORRUPTED WAL RECORD DETECTED");

                    return None;
                }

                Some((Command::Del(key), total))
            }
            _=> None,
        }

    }
}