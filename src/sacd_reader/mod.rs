use anyhow::Result;

mod iso_reader;
mod net_reader;

pub use iso_reader::IsoReader;
pub use net_reader::NetReader;

/// Trait for reading SACD data from various sources (ISO file, network, etc.)
pub trait SacdReader {
    /// Read sectors from the disc
    ///
    /// # Arguments
    /// * `start_lsn` - Starting logical sector number
    /// * `sector_count` - Number of sectors to read
    ///
    /// # Returns
    /// A vector containing the raw sector data (sector_count * 2048 bytes)
    fn read_data(&mut self, start_lsn: u32, sector_count: u32) -> Result<Vec<u8>>;

    /// Get the total number of sectors
    fn get_total_sectors(&mut self) -> Result<u32>;
}

/// Enumeration of SACD data sources
pub enum SacdSource {
    /// ISO file on disk
    Iso(iso_reader::IsoReader),
    /// Network SACD server
    Network(net_reader::NetReader),
}

impl SacdReader for SacdSource {
    fn read_data(&mut self, start_lsn: u32, sector_count: u32) -> Result<Vec<u8>> {
        match self {
            SacdSource::Iso(reader) => reader.read_data(start_lsn, sector_count),
            SacdSource::Network(reader) => reader.read_data(start_lsn, sector_count),
        }
    }

    fn get_total_sectors(&mut self) -> Result<u32> {
        match self {
            SacdSource::Iso(reader) => reader.get_total_sectors(),
            SacdSource::Network(reader) => reader.get_total_sectors(),
        }
    }
}
