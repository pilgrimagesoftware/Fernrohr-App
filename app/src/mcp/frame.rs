//! The endpoint's framing: each message is a big-endian `u32` byte length, then
//! that many bytes of JSON. The length is checked against the caller's limit
//! before anything is allocated, so a peer can't make either side buffer more
//! than it agreed to.
//!
//! This owns only the bytes on the wire; what the messages mean is
//! `protocol`'s.

use serde::Serialize;
use serde::de::DeserializeOwned;
use std::fmt;
use std::io;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Why a frame couldn't be read or written.
#[derive(Debug)]
pub(super) enum FrameError {
    /// The peer closed the connection before a frame began.
    Closed,
    /// The frame (announced, or about to be written) is longer than the limit.
    TooLarge,
    /// The bytes were not the JSON message expected.
    Malformed,
    /// The socket failed mid-frame.
    Io(io::Error),
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Closed => f.write_str("connection closed"),
            Self::TooLarge => f.write_str("frame over its size limit"),
            Self::Malformed => f.write_str("malformed message"),
            Self::Io(error) => write!(f, "socket error: {error}"),
        }
    }
}

impl From<io::Error> for FrameError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Reads one frame of at most `limit` bytes and decodes it as `T`.
pub(super) async fn read<T: DeserializeOwned>(
    reader: &mut (impl AsyncRead + Unpin),
    limit: usize,
) -> Result<T, FrameError> {
    let len = match reader.read_u32().await {
        Ok(len) => len as usize,
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
            return Err(FrameError::Closed);
        }
        Err(error) => return Err(error.into()),
    };
    if len > limit {
        return Err(FrameError::TooLarge);
    }
    let mut body = vec![0; len];
    reader.read_exact(&mut body).await?;
    serde_json::from_slice(&body).map_err(|_| FrameError::Malformed)
}

/// Encodes `value` and writes it as one frame, refusing to send more than
/// `limit` bytes.
pub(super) async fn write<T: Serialize>(
    writer: &mut (impl AsyncWrite + Unpin),
    value: &T,
    limit: usize,
) -> Result<(), FrameError> {
    let body = serde_json::to_vec(value).map_err(|_| FrameError::Malformed)?;
    if body.len() > limit {
        return Err(FrameError::TooLarge);
    }
    let len = u32::try_from(body.len()).map_err(|_| FrameError::TooLarge)?;
    writer.write_u32(len).await?;
    writer.write_all(&body).await?;
    writer.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    #[tokio::test]
    async fn a_frame_round_trips() {
        let (mut a, mut b) = tokio::io::duplex(1024);
        write(&mut a, &json!({"hello": [1, 2]}), 1024)
            .await
            .unwrap();
        let back: Value = read(&mut b, 1024).await.unwrap();
        assert_eq!(back, json!({"hello": [1, 2]}));
    }

    #[tokio::test]
    async fn an_oversized_frame_is_refused_before_its_body_is_read() {
        let (mut a, mut b) = tokio::io::duplex(1024);
        // Only the length goes out: a reader that waited for the body would hang.
        a.write_u32(10_000).await.unwrap();
        let result = read::<Value>(&mut b, 100).await;
        assert!(matches!(result, Err(FrameError::TooLarge)));
    }

    #[tokio::test]
    async fn a_writer_refuses_a_value_past_its_limit() {
        let (mut a, _b) = tokio::io::duplex(1024);
        let result = write(&mut a, &"x".repeat(200), 100).await;
        assert!(matches!(result, Err(FrameError::TooLarge)));
    }

    #[tokio::test]
    async fn a_clean_close_is_not_an_io_failure() {
        let (a, mut b) = tokio::io::duplex(1024);
        drop(a);
        assert!(matches!(
            read::<Value>(&mut b, 100).await,
            Err(FrameError::Closed)
        ));
    }

    #[tokio::test]
    async fn bytes_that_are_not_the_message_are_malformed() {
        let (mut a, mut b) = tokio::io::duplex(1024);
        a.write_u32(3).await.unwrap();
        a.write_all(b"{x}").await.unwrap();
        assert!(matches!(
            read::<Value>(&mut b, 100).await,
            Err(FrameError::Malformed)
        ));
    }
}
