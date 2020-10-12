use anyhow::{Error as AnyErr, Result};
use async_std::net::TcpStream;
use futures::future::*;
use futures::AsyncReadExt;
use quinn::{
  Certificate, CertificateChain, ClientConfig, ClientConfigBuilder, Endpoint, Incoming, PrivateKey,
  ServerConfig, ServerConfigBuilder, TransportConfig,
};
use std::path::Path;
use std::task::{Context, Poll};
use std::{error::Error, net::SocketAddr, sync::Arc};
use tokio::io::PollEvented;

pub fn validate_existing_file(v: String) -> Result<(), String> {
  if !Path::new(&v).exists() {
    Err(String::from("A file must exist at the given path"))
  } else {
    Ok(())
  }
}

pub fn parse_socketaddr(v: &str) -> Result<SocketAddr> {
  use std::convert::TryFrom;
  use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
  ToSocketAddrs::to_socket_addrs(v)
    .map_err(|e| e.into())
    .and_then(|mut items| {
      items.nth(0).ok_or(AnyErr::msg(
        "No addresses were resolved from the given host",
      ))
    })
    .into()
}

pub fn validate_socketaddr(v: String) -> Result<(), String> {
  parse_socketaddr(&v).map(|_| ()).map_err(|e| e.to_string())
}

pub async fn proxy_tcp_streams(mut source: TcpStream, mut proxy: TcpStream) -> Result<()> {
  let (mut reader, mut writer) = (&mut source).split();
  let (mut proxy_reader, mut proxy_writer) = (&mut proxy).split();
  let proxy_i2o = Box::pin(async_std::io::copy(&mut reader, &mut proxy_writer).fuse());
  let proxy_o2i = Box::pin(async_std::io::copy(&mut proxy_reader, &mut writer).fuse());
  let res: Either<(), ()> = match futures::future::try_select(proxy_i2o, proxy_o2i).await {
    Ok(Either::Left((_i2o, resume_o2i))) => {
      println!("Source connection closed gracefully, shutting down proxy");
      std::mem::drop(resume_o2i); // Kill the copier, allowing us to send end-of-connection
      Either::Right(())
    }
    Ok(Either::Right((_o2i, resume_i2o))) => {
      println!("Proxy connection closed gracefully, shutting down source");
      std::mem::drop(resume_i2o); // Kill the copier, allowing us to send end-of-connection
      Either::Left(())
    }
    Err(Either::Left((e_i2o, resume_o2i))) => {
      println!(
        "Source connection died with error {:#?}, shutting down proxy connection",
        e_i2o
      );
      std::mem::drop(resume_o2i); // Kill the copier, allowing us to send end-of-connection
      Either::Right(())
    }
    Err(Either::Right((e_o2i, resume_i2o))) => {
      println!(
        "Proxy connection died with error {:#?}, shutting down source connection",
        e_o2i
      );
      std::mem::drop(resume_i2o); // Kill the copier, allowing us to send end-of-connection
      Either::Left(())
    }
  };
  std::mem::drop(reader);
  std::mem::drop(writer);
  std::mem::drop(proxy_reader);
  std::mem::drop(proxy_writer);
  match res {
    Either::Left(_) => {
      if let Err(shutdown_failure) = source.shutdown(async_std::net::Shutdown::Both) {
        eprintln!(
          "Failed to shut down source connection with error:\n{:#?}",
          shutdown_failure
        );
      }
    }
    Either::Right(_) => {
      if let Err(shutdown_failure) = proxy.shutdown(async_std::net::Shutdown::Both) {
        eprintln!(
          "Failed to shut down proxy connection with error:\n{:#?}",
          shutdown_failure
        );
      }
    }
  }
  Ok(())
}

// Utility helpers from quinn/examples/common

/// Constructs a QUIC endpoint configured for use a client only.
///
/// ## Args
///
/// - server_certs: list of trusted certificates.
#[allow(unused)]
pub fn make_client_endpoint(
  bind_addr: SocketAddr,
  server_certs: &[&[u8]],
) -> Result<Endpoint, Box<dyn Error>> {
  let client_cfg = configure_client(server_certs)?;
  let mut endpoint_builder = Endpoint::builder();
  endpoint_builder.default_client_config(client_cfg);
  let (endpoint, _incoming) = endpoint_builder.bind(&bind_addr)?;
  Ok(endpoint)
}

/// Constructs a QUIC endpoint configured to listen for incoming connections on a certain address
/// and port.
///
/// ## Returns
///
/// - a sream of incoming QUIC connections
/// - server certificate serialized into DER format
#[allow(unused)]
pub fn make_server_endpoint(bind_addr: SocketAddr) -> Result<(Incoming, Vec<u8>), Box<dyn Error>> {
  let (server_config, server_cert) = configure_server()?;
  let mut endpoint_builder = Endpoint::builder();
  endpoint_builder.listen(server_config);
  let (_endpoint, incoming) = endpoint_builder.bind(&bind_addr)?;
  Ok((incoming, server_cert))
}

/// Builds default quinn client config and trusts given certificates.
///
/// ## Args
///
/// - server_certs: a list of trusted certificates in DER format.
fn configure_client(server_certs: &[&[u8]]) -> Result<ClientConfig, Box<dyn Error>> {
  let mut cfg_builder = ClientConfigBuilder::default();
  for cert in server_certs {
    cfg_builder.add_certificate_authority(Certificate::from_der(&cert)?)?;
  }
  Ok(cfg_builder.build())
}

/// Returns default server configuration along with its certificate.
fn configure_server() -> Result<(ServerConfig, Vec<u8>), Box<dyn Error>> {
  let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
  let cert_der = cert.serialize_der().unwrap();
  let priv_key = cert.serialize_private_key_der();
  let priv_key = PrivateKey::from_der(&priv_key)?;

  let mut transport_config = TransportConfig::default();
  transport_config.stream_window_uni(0);
  let mut server_config = ServerConfig::default();
  server_config.transport = Arc::new(transport_config);
  let mut cfg_builder = ServerConfigBuilder::new(server_config);
  let cert = Certificate::from_der(&cert_der)?;
  cfg_builder.certificate(CertificateChain::from_certs(vec![cert]), priv_key)?;

  Ok((cfg_builder.build(), cert_der))
}

#[allow(unused)]
pub const ALPN_QUIC_HTTP: &[&[u8]] = &[b"hq-29"];
