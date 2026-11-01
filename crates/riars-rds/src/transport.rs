//! The links to the encoders. Blocking, with short timeouts: they run on the RDS
//! thread only.

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs, UdpSocket};
use std::path::PathBuf;
use std::time::Duration;

use riars_core::rds::RdsTransport;

const TIMEOUT: Duration = Duration::from_secs(3);

pub(crate) enum Link {
    Tcp(TcpStream),
    Udp(UdpSocket, SocketAddr),
    Serial(Box<dyn serialport::SerialPort>),
    File(PathBuf),
}

fn resolve(host: &str, port: u16) -> io::Result<SocketAddr> {
    (host, port)
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("{host} has no address")))
}

impl Link {
    pub(crate) fn open(transport: &RdsTransport) -> io::Result<Self> {
        match transport {
            RdsTransport::Tcp { host, port } => {
                let stream = TcpStream::connect_timeout(&resolve(host, *port)?, TIMEOUT)?;
                stream.set_write_timeout(Some(TIMEOUT))?;
                stream.set_nodelay(true)?;
                Ok(Link::Tcp(stream))
            }
            RdsTransport::Udp { host, port } => {
                let to = resolve(host, *port)?;
                let local: SocketAddr = if to.is_ipv6() { "[::]:0".parse() } else { "0.0.0.0:0".parse() }
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
                Ok(Link::Udp(UdpSocket::bind(local)?, to))
            }
            RdsTransport::Serial { port, baud } => serialport::new(port, *baud)
                .timeout(TIMEOUT)
                .open()
                .map(Link::Serial)
                .map_err(io::Error::other),
            RdsTransport::File { path } => Ok(Link::File(PathBuf::from(path))),
        }
    }

    pub(crate) fn send(&mut self, bytes: &[u8]) -> io::Result<()> {
        match self {
            Link::Tcp(stream) => {
                stream.write_all(bytes)?;
                drain_tcp(stream)
            }
            Link::Udp(socket, to) => socket.send_to(bytes, *to).map(|_| ()),
            Link::Serial(port) => {
                port.write_all(bytes)?;
                port.flush()?;
                // Encoders answer with acknowledgements nobody reads; do not let them pile up.
                let waiting = port.bytes_to_read().unwrap_or(0) as usize;
                if waiting > 0 {
                    let mut buf = vec![0; waiting.min(4096)];
                    let _ = port.read(&mut buf);
                }
                Ok(())
            }
            Link::File(path) => write_file(path, bytes),
        }
    }
}

/// Read and drop what the encoder sent back (acknowledgements), and notice when it
/// closed the connection.
fn drain_tcp(stream: &mut TcpStream) -> io::Result<()> {
    stream.set_nonblocking(true)?;
    let mut buf = [0u8; 1024];
    let result = loop {
        match stream.read(&mut buf) {
            Ok(0) => break Err(io::Error::new(io::ErrorKind::ConnectionAborted, "the encoder closed the connection")),
            Ok(_) => continue,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => break Ok(()),
            Err(e) => break Err(e),
        }
    };
    stream.set_nonblocking(false)?;
    result
}

/// Replace the file in one step, so a reader never sees half a text.
fn write_file(path: &PathBuf, bytes: &[u8]) -> io::Result<()> {
    // Checked again here: the path may have become a link since it was set.
    if let Ok(meta) = std::fs::symlink_metadata(path) {
        if !meta.file_type().is_file() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "the path is not a plain file"));
        }
    }
    let tmp = path.with_extension("txt.tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}
