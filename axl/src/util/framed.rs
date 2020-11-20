#[warn(unused_imports)]
use anyhow::{Context as AnyhowContext, Result};
use futures::future;
use futures::future::*;
use quinn::{
  Certificate, CertificateChain, ClientConfig, ClientConfigBuilder, Endpoint, Incoming, PrivateKey,
  ServerConfig, ServerConfigBuilder, TransportConfig,
};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::{
  path::{Path, PathBuf},
  pin::Pin,
  sync::Arc,
  task::{Context, Poll},
};

pub async fn read_frame_vec<T: tokio::io::AsyncRead + Unpin>(s: &mut T) -> Result<Vec<u8>> {
  use tokio::io::AsyncReadExt;
  let length = s.read_u32().await.context("Failure reading frame length")? as usize;
  let mut buffer = Vec::with_capacity(length);
  buffer.resize_with(length, Default::default);
  s.read_exact(buffer.as_mut_slice())
    .await
    .context("Failure reading frame contents")?;
  Ok(buffer)
}

pub async fn write_frame<T: tokio::io::AsyncWrite + Unpin>(s: &mut T, buffer: &[u8]) -> Result<()> {
  use tokio::io::AsyncWriteExt;
  s.write_u32(buffer.len() as u32)
    .await
    .context("Failure writing frame length")?;
  s.write_all(&buffer)
    .await
    .context("Failed writing frame contents")
}

#[cfg(test)]
mod tests {
  use crate::util::framed::{read_frame_vec, write_frame};

  #[async_std::test]
  async fn stream_framed_roundtrip() {
    use super::{read_frame_vec, write_frame};
    use ::std::io::Seek;
    use ::tokio::io::{AsyncReadExt, AsyncWriteExt};
    const TEST_BLOB_LENGTH: usize = 1234;
    let buffer: Vec<u8> = Vec::with_capacity(TEST_BLOB_LENGTH + std::mem::size_of::<u32>());
    let mut cursor = std::io::Cursor::new(buffer);
    // Test data is a simple array of 0 through (but not including) its capacity
    let test_data = {
      let mut test_data = Vec::with_capacity(TEST_BLOB_LENGTH);
      test_data.extend(
        (0u32..(test_data.capacity() as u32))
          .map(|x| std::ops::Rem::rem(x, std::u8::MAX as u32) as u8),
      );
      test_data
    };
    write_frame(&mut cursor, &test_data)
      .await
      .expect("Writing frame to stream must succeed");
    cursor.set_position(0);
    let deserialized = read_frame_vec(&mut cursor)
      .await
      .expect("Reading frame from stream must succeed");
    let buffer = cursor.into_inner();
    // Stream must receive content of equal length to a u32 plus that of the content
    assert_eq!(buffer.len(), TEST_BLOB_LENGTH + std::mem::size_of::<u32>());
    // After the length of a u32, the stream should be equal to the content
    assert_eq!(&buffer[std::mem::size_of::<u32>()..], &test_data[..]);
    // Input and output data should be the same
    assert_eq!(test_data, deserialized);
  }
}