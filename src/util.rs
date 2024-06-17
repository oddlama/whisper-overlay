use color_eyre::eyre::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub async fn send_message<S: AsyncWriteExt + std::marker::Unpin>(
    socket: &mut S,
    value: serde_json::Value,
) -> Result<()> {
    let json_str = value.to_string();
    let data = json_str.as_bytes();
    let message_length = (data.len() as u32).to_be_bytes();

    socket.write_all(&message_length).await?;
    socket.write_all(data).await?;
    socket.flush().await?;

    Ok(())
}

pub async fn recv_message<S: AsyncReadExt + std::marker::Unpin>(
    socket: &mut S,
) -> Result<serde_json::Value> {
    let mut length_buf = [0u8; 4];
    socket.read_exact(&mut length_buf).await?;

    let message_length = u32::from_be_bytes(length_buf) as usize;
    let mut data_buf = vec![0u8; message_length];
    socket.read_exact(&mut data_buf).await?;

    let json_str = String::from_utf8(data_buf)?;
    let value = serde_json::from_str(&json_str)?;
    Ok(value)
}
