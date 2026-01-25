use crate::{
    sacd_ripper::server_request::Type as req_type,
    sacd_ripper::server_response::Type as resp_type,
    sacd_ripper::{ServerRequest, ServerResponse},
};
use anyhow::{Context, Result};
use log::{debug, info};
use prost::Message;
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::fs::File;
use std::path::Path;

pub struct SacdNetReader {
    stream: TcpStream,
}

impl Drop for SacdNetReader {
    fn drop(&mut self) {
        self.close_reader();
    }
}

impl SacdNetReader {
    fn close_reader(&mut self) {
        let req = ServerRequest {
            r#type: req_type::DiscClose as i32,
            sector_offset: Some(0),
            sector_count: Some(0),
        };

        let _ = self.send_req(req);
        debug!("reader dropped and closed");
    }

    fn send_req(&mut self, req: ServerRequest) -> Result<ServerResponse> {
        let mut encoded_request = Vec::new();
        req.encode(&mut encoded_request)
            .context("couldn't encode request")?;

        self.stream
            .write_all(&encoded_request)
            .context("couldn't write stream")?;

        // The original C implementation of the ripper protocol
        // terminates the protobuf payload with a zero.
        let zero: u8 = 0;
        self.stream
            .write_all(&[zero])
            .context("couldn't write stream terminator")?;
        self.stream.flush()?;

        // Read response from the socket.
        // TCP is a stream protocol - we may need multiple read() calls to get the full message.
        // Strategy: Read chunks, and after each read, check if we have a zero terminator.
        // If so, try decoding everything *except* the terminator.
        let mut buffer = Vec::new();
        let mut temp_buf = [0u8; 8192];
        let max_size = 1024 * 1024 + 1024; // 1MB data + overhead

        loop {
            let bytes_read = self
                .stream
                .read(&mut temp_buf)
                .context("couldn't read from stream")?;
            if bytes_read == 0 {
                anyhow::bail!("Connection closed before receiving complete message");
            }

            buffer.extend_from_slice(&temp_buf[..bytes_read]);

            // Check if we have a zero terminator at the end
            if buffer.last() == Some(&0) {
                // Try to decode everything except the terminator
                let msg_bytes = &buffer[..buffer.len() - 1];
                match ServerResponse::decode(msg_bytes) {
                    Ok(response) => {
                        debug!(
                            "Successfully decoded message ({} bytes + terminator)",
                            msg_bytes.len()
                        );
                        debug!(
                            "Decoded response: type={}, result={}",
                            response.r#type, response.result
                        );
                        return Ok(response);
                    }
                    Err(_) => {
                        // Decode failed - we need more data (the zero we found was in the middle of the data)
                        debug!("incomplete response, reading more");
                    }
                }
            }

            // Safety check
            if buffer.len() > max_size {
                anyhow::bail!(
                    "Message size exceeded maximum ({}MB)",
                    max_size / (1024 * 1024)
                );
            }
        }
    }

    /// Get the total number of sectors on the disc
    ///
    /// # Returns
    /// * `Ok(u32)` - Total number of sectors
    /// * `Err` - If the request fails
    pub fn get_total_sectors(&mut self) -> Result<u32> {
        let req = ServerRequest {
            r#type: req_type::DiscSize as i32,
            sector_offset: Some(0),
            sector_count: Some(0),
        };

        let response = self.send_req(req)?;

        if response.r#type != resp_type::DiscSize as i32 {
            anyhow::bail!("Expected DISC_SIZE response, got type {}", response.r#type);
        }

        Ok(response.result as u32)
    }

    /// Read sectors from the disc.
    ///
    /// # Arguments
    /// * `pos` - Starting sector position (sector_offset)
    /// * `block_count` - Number of sectors to read (sector_count)
    ///
    /// # Returns
    /// * `Ok(Vec<u8>)` - The sector data (each sector is SACD_LSN_SIZE bytes = 2048)
    /// * `Err` - If the read fails or server doesn't return data
    pub fn read_data(&mut self, pos: u32, block_count: u32) -> Result<Vec<u8>> {
        let req = ServerRequest {
            r#type: req_type::DiscRead as i32,
            sector_offset: Some(pos),
            sector_count: Some(block_count),
        };

        let response = self.send_req(req)?;

        if response.r#type != resp_type::DiscRead as i32 {
            anyhow::bail!("Expected DISC_READ response, got type {}", response.r#type);
        }

        // Return data if present
        // Note: response.result contains the number of sectors actually read
        if let Some(data) = response.data {
            let sectors_read = response.result as u32;
            debug!("Read {} sectors ({} bytes)", sectors_read, data.len());
            Ok(data)
        } else {
            anyhow::bail!(
                "Server returned DISC_READ response without data (result={})",
                response.result
            );
        }
    }

    /// Dump the entire SACD disc to an ISO file
    ///
    /// This reads all sectors from the disc and writes them to an ISO file.
    /// Note: This function does not handle decryption - it writes raw sectors.
    ///
    /// # Arguments
    /// * `output_path` - Path to the output ISO file
    /// * `progress_callback` - Optional callback for progress updates (current_sector, total_sectors)
    ///
    /// # Returns
    /// * `Ok(u32)` - Total number of sectors written
    /// * `Err` - If the dump fails
    pub fn dump_iso<P: AsRef<Path>, F>(
        &mut self,
        output_path: P,
        mut progress_callback: Option<F>,
    ) -> Result<u32>
    where
        F: FnMut(u32, u32),
    {
        const SACD_LSN_SIZE: usize = 2048;
        const MAX_BLOCK_SIZE: u32 = 512; // Read 512 sectors at a time (1MB)

        // Get total sectors
        let total_sectors = self.get_total_sectors()
            .context("Failed to get total sectors")?;

        info!("Dumping ISO: {} sectors ({} MB)",
            total_sectors,
            (total_sectors as u64 * SACD_LSN_SIZE as u64) / (1024 * 1024)
        );

        // Create output file
        let mut output_file = File::create(output_path.as_ref())
            .context("Failed to create output file")?;

        let mut current_sector = 0u32;

        while current_sector < total_sectors {
            let block_size = std::cmp::min(MAX_BLOCK_SIZE, total_sectors - current_sector);

            // Read sectors
            let data = self.read_data(current_sector, block_size)
                .with_context(|| format!("Failed to read sectors {} to {}",
                    current_sector, current_sector + block_size))?;

            // Write to file
            output_file.write_all(&data)
                .context("Failed to write to output file")?;

            current_sector += block_size;

            // Call progress callback if provided
            if let Some(ref mut callback) = progress_callback {
                callback(current_sector, total_sectors);
            }

            if current_sector % 10240 == 0 || current_sector == total_sectors {
                info!("Progress: {}/{} sectors ({:.1}%)",
                    current_sector,
                    total_sectors,
                    (current_sector as f64 / total_sectors as f64) * 100.0
                );
            }
        }

        output_file.flush()
            .context("Failed to flush output file")?;

        info!("ISO dump complete: {} sectors written", total_sectors);

        Ok(total_sectors)
    }
}

pub fn open_network_reader(ip_addr: IpAddr, port: u16) -> Result<SacdNetReader> {
    let socket_addr = SocketAddr::new(ip_addr, port);
    let stream = TcpStream::connect(socket_addr).context("couldn't connect to server")?;

    let mut handle = SacdNetReader { stream };

    let req = ServerRequest {
        r#type: req_type::DiscOpen as i32,
        sector_offset: Some(0),
        sector_count: Some(0),
    };

    let response = handle.send_req(req)?;

    // Check the response
    if response.result != 0 || response.r#type != resp_type::DiscOpened as i32 {
        anyhow::bail!("response result non-zero or incorrect type");
    }

    Ok(handle)
}

// #[cfg(test)]
// mod tests {
//     use std::net::Ipv4Addr;

//     use super::*;

//     fn init() {
//         let _ = env_logger::builder().is_test(true).try_init();
//     }

//     #[test]
//     fn test_open_network() {
//         init();
//         let handle = open_network_reader(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 130)), 2002).expect("should init");
//     }
// }
