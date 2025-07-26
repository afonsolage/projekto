use crate::MessageError;

pub fn encode<T>(buf: &mut [u8], msg: &T) -> Result<u32, MessageError>
where
    T: serde::Serialize,
{
    let written = bincode::serde::encode_into_slice(msg, buf, bincode::config::standard())?;
    Ok(written as u32)
}

pub fn decode<'a, T>(buf: &'a [u8]) -> Result<T, MessageError>
where
    T: serde::Deserialize<'a>,
{
    let (msg, _) = bincode::serde::borrow_decode_from_slice(buf, bincode::config::standard())?;
    Ok(msg)
}
