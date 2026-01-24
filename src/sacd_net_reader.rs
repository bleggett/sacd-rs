use anyhow::Result;
use prost::{Message};
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream};
use crate::{sacd_ripper::{ServerResponse, ServerRequest}, sacd_ripper::server_request::Type as req_type, sacd_ripper::server_response::Type as resp_type};
use log::{debug, info};

pub struct SacdNetReader {
    stream: TcpStream,
}

pub fn open_network_reader(ip_addr: IpAddr, port: u16) -> Result<SacdNetReader> {
    let socket_addr = SocketAddr::new(ip_addr, port);
    let mut stream = TcpStream::connect(socket_addr)?;

    let req = ServerRequest{
        r#type: req_type::DiscOpen as i32,
        sector_offset: Some(0),
        sector_count: Some(0),
    };

    let mut encoded_request = Vec::new();
    req.encode(&mut encoded_request)?;

    stream.write_all(&encoded_request)?;

    // The original C implementation of the ripper protocol
    // terminates the protobuf payload with a zero.
    let zero: u8 = 0;
    stream.write_all(&[zero])?;
    stream.flush()?;

    // Read response into a reasonably sized buffer
    // We can't read byte-by-byte looking for zero because protobuf messages
    // contain zero bytes naturally (e.g., when encoding the value 0)
    //
    // The server will terminate messages with a zero as well, but we
    // can't rely fully on that because the proto response might have
    // zeroes midstream. Fortunately, the C implementation uses nanopb
    // hard size limits on response payloads - `data` field is capped at 1MB,
    // and the other fields are fixed, so we can lean on that for reads
    let mut buffer = vec![0u8; 1024*1024];
    let bytes_read = stream.read(&mut buffer)?;
    buffer.truncate(bytes_read);

    // The C protocol appends a zero byte terminator after the protobuf message
    // We need to strip it before decoding
    if buffer.is_empty() {
        anyhow::bail!("No data received from server");
    }

    debug!("Received {} bytes: {:02x?}", buffer.len(), &buffer[..]);

    if buffer.last() == Some(&0) {
        buffer.pop();
    } else {
        anyhow::bail!("Expected zero terminator byte, got {:02x}", buffer.last().unwrap());
    }

    // Decode the protobuf message
    let response = match ServerResponse::decode(&buffer[..]) {
        Ok(resp) => resp,
        Err(err) => anyhow::bail!("failed to decode response: {}", err),
    };

    debug!("Decoded response: type={}, result={}", response.r#type, response.result);

    // Check the response
    if response.result != 0 || response.r#type != resp_type::DiscOpened as i32 {
        anyhow::bail!("response result non-zero or incorrect type");
    }

    let handle = SacdNetReader{
        stream,
    };

    Ok(handle)
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use super::*;

    #[test]
    fn it_works() {
        let handle = open_network_reader(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 130)), 2002).expect("should init");
    }
}
