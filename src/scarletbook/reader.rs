use crate::{
    sacd_ripper::server_request::Type as req_type,
    sacd_ripper::server_response::Type as resp_type,
    sacd_ripper::{ServerRequest, ServerResponse},
};
use anyhow::Result;
use byteorder::{BigEndian, ReadBytesExt};
use prost::Message;
use std::io::{Cursor, Read};
use std::net::{IpAddr, SocketAddr, TcpStream};

use crate::sacd_net_reader::SacdNetReader;
use log::{debug, info};

use crate::scarletbook::consts;

#[derive(Debug, Clone)]
pub struct GenreTable {
    pub category: u8,
    pub reserved: u8,
    pub genre: u16,
}

impl GenreTable {
    fn read_from<R: Read>(reader: &mut R) -> Result<Self> {
        Ok(Self {
            category: reader.read_u8()?,
            reserved: reader.read_u8()?,
            genre: reader.read_u16::<BigEndian>()?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct LocaleTable {
    pub language_code: [u8; 2],
    pub character_set: u8,
    pub reserved: u8,
}

impl LocaleTable {
    fn read_from<R: Read>(reader: &mut R) -> Result<Self> {
        let mut language_code = [0u8; 2];
        reader.read_exact(&mut language_code)?;
        Ok(Self {
            language_code,
            character_set: reader.read_u8()?,
            reserved: reader.read_u8()?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Version {
    pub major: u8,
    pub minor: u8,
}
impl Version {
    fn read_from<R: Read>(reader: &mut R) -> Result<Self> {
        Ok(Self {
            major: reader.read_u8()?,
            minor: reader.read_u8()?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct MasterToc {
    // M_TOC_0_Header (16 bytes)
    pub id: [u8; 8], // SACDMTOC; Master_TOC_Signature
    pub version: Version, // Spec_Version   1.20 / 0x0114
    pub reserved01: [u8; 6],
    // Album_Info (48 bytes)
    pub album_set_size: u16, // Album_Set_Size,2bytes,  1..6553
    pub album_sequence_number: u16, // Album_Sequence_Number
    pub reserved02: [u8; 4],
    pub album_catalog_number: [u8; 16], // Album_Catalog_Number, 16 bytes, String; 0x00 when empty, else padded with spaces for shorter strings
    pub album_genre: [GenreTable; 4], // Album_Genre, 4x4 bytes;
    pub reserved03: [u8; 8],
    // Disc_Info (64 bytes)
    pub area_1_toc_1_start: u32, // LSN for AREA_TOC_1 of 2 channel - 2CH_TOC_1_Address, 4bytes, Uint32, values 0, 544
    pub area_1_toc_2_start: u32, // LSN for AREA_TOC_2 of 2 channel - 2CH_TOC_2_Address, 4bytes, Uint32
    pub area_2_toc_1_start: u32, // LSN for AREA_TOC_1 of M channel - MC_TOC_1_Address, 4bytes, Uint32
    pub area_2_toc_2_start: u32, // LSN for AREA_TOC_2 of M channel   - MC_TOC_2_Address, 4bytes, Uint32
    pub disc_flags: u8, // Disc_Flags, 1 byte - Hybr, b7
    pub reserved04: [u8; 3],
    pub area_1_toc_size: u16, // Length in Sectors of AREA_TOC of  2ch - 2CH_TOC_Length, 2byte, Uint16, value 0, 5..
    pub area_2_toc_size: u16, // Length in Sectors of AREA_TOC of M channel - MC_TOC_Length, 2byte, Uint16, value 0, 37..
    pub disc_catalog_number: [u8; 16], // Disc_Catalog_Number, 16 bytes, String; 0x00 when empty, else padded with spaces for shorter strings
    pub disc_genre: [GenreTable; 4], // Disc_Genre, 4x4 bytes
    pub disc_date_year: u16, // Disc_Date , 4 bytes
    pub disc_date_month: u8,
    pub disc_date_day: u8,
    pub reserved05: [u8; 4],
    // Text_Channels (40 bytes)
    pub text_area_count: u8, // N_Text_Channels, 1 byte, Uint8  , values =0..8
    pub reserved06: [u8; 7],
    pub locales: [LocaleTable; 8],  // N_Text_Channels values= 0...8
}

impl MasterToc {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut cursor = Cursor::new(bytes);
        Self::read_from(&mut cursor)
    }

    pub fn read_from<R: Read>(reader: &mut R) -> Result<Self> {
        let mut id = [0u8; 8];
        reader.read_exact(&mut id)?;

        let version = Version::read_from(reader)?;

        let mut reserved01 = [0u8; 6];
        reader.read_exact(&mut reserved01)?;

        let album_set_size = reader.read_u16::<BigEndian>()?;
        let album_sequence_number = reader.read_u16::<BigEndian>()?;

        let mut reserved02 = [0u8; 4];
        reader.read_exact(&mut reserved02)?;

        let mut album_catalog_number = [0u8; 16];
        reader.read_exact(&mut album_catalog_number)?;

        let album_genre = [
            GenreTable::read_from(reader)?,
            GenreTable::read_from(reader)?,
            GenreTable::read_from(reader)?,
            GenreTable::read_from(reader)?,
        ];

        let mut reserved03 = [0u8; 8];
        reader.read_exact(&mut reserved03)?;

        let area_1_toc_1_start = reader.read_u32::<BigEndian>()?;
        let area_1_toc_2_start = reader.read_u32::<BigEndian>()?;
        let area_2_toc_1_start = reader.read_u32::<BigEndian>()?;
        let area_2_toc_2_start = reader.read_u32::<BigEndian>()?;

        let disc_flags = reader.read_u8()?;

        let mut reserved04 = [0u8; 3];
        reader.read_exact(&mut reserved04)?;

        let area_1_toc_size = reader.read_u16::<BigEndian>()?;
        let area_2_toc_size = reader.read_u16::<BigEndian>()?;

        let mut disc_catalog_number = [0u8; 16];
        reader.read_exact(&mut disc_catalog_number)?;

        let disc_genre = [
            GenreTable::read_from(reader)?,
            GenreTable::read_from(reader)?,
            GenreTable::read_from(reader)?,
            GenreTable::read_from(reader)?,
        ];

        let disc_date_year = reader.read_u16::<BigEndian>()?;
        let disc_date_month = reader.read_u8()?;
        let disc_date_day = reader.read_u8()?;

        let mut reserved05 = [0u8; 4];
        reader.read_exact(&mut reserved05)?;

        // Text_Channels (40 bytes)
        let text_area_count = reader.read_u8()?;

        let mut reserved06 = [0u8; 7];
        reader.read_exact(&mut reserved06)?;

        let locales = [
            LocaleTable::read_from(reader)?,
            LocaleTable::read_from(reader)?,
            LocaleTable::read_from(reader)?,
            LocaleTable::read_from(reader)?,
            LocaleTable::read_from(reader)?,
            LocaleTable::read_from(reader)?,
            LocaleTable::read_from(reader)?,
            LocaleTable::read_from(reader)?,
        ];

        Ok(Self {
            id,
            version,
            reserved01,
            album_set_size,
            album_sequence_number,
            reserved02,
            album_catalog_number,
            album_genre,
            reserved03,
            area_1_toc_1_start,
            area_1_toc_2_start,
            area_2_toc_1_start,
            area_2_toc_2_start,
            disc_flags,
            reserved04,
            area_1_toc_size,
            area_2_toc_size,
            disc_catalog_number,
            disc_genre,
            disc_date_year,
            disc_date_month,
            disc_date_day,
            reserved05,
            text_area_count,
            reserved06,
            locales,
        })
    }

    /// Validate that this is a valid Master TOC
    pub fn is_valid(&self) -> bool {
        &self.id == b"SACDMTOC"
    }

    /// Get ID as string
    pub fn id_string(&self) -> String {
        String::from_utf8_lossy(&self.id).to_string()
    }

    /// Get album catalog number as string (trimmed)
    pub fn album_catalog(&self) -> String {
        String::from_utf8_lossy(&self.album_catalog_number)
            .trim_end_matches('\0')
            .trim()
            .to_string()
    }

    /// Get disc catalog number as string (trimmed)
    pub fn disc_catalog(&self) -> String {
        String::from_utf8_lossy(&self.disc_catalog_number)
            .trim_end_matches('\0')
            .trim()
            .to_string()
    }

    /// Check if disc is hybrid (has both SACD and CD layers)
    pub fn is_hybrid(&self) -> bool {
        (self.disc_flags & 0x80) != 0
    }

    /// Has two-channel area
    pub fn has_two_channel(&self) -> bool {
        self.area_1_toc_1_start != 0
    }

    /// Has multi-channel area
    pub fn has_multi_channel(&self) -> bool {
        self.area_2_toc_1_start != 0
    }

    /// Get language code for a locale index
    pub fn get_language(&self, index: usize) -> Option<String> {
        if index < self.text_area_count as usize {
            String::from_utf8(self.locales[index].language_code.to_vec()).ok()
        } else {
            None
        }
    }
}

pub struct ScarletBookReader {
    reader: SacdNetReader,
    stereo_area_index: i32,
    mch_area_index: i32,
    // void                     * sacd;                                      // sacd_reader_t

    // uint8_t                  * master_data;
    // master_toc_t             * master_toc;
    // master_man_t             * master_man;
    // master_text_t              master_text;

    // int                        twoch_area_idx;
    // int                        mulch_area_idx;
    // int                        area_count;
    // scarletbook_area_t         area[4];   // added for backup 2 more areas:  2= TWOCHTOC  TOC-2;    3 =MULCHTOC  TOC-2

    // scarletbook_audio_frame_t  frame;
    // audio_sector_t             audio_sector;

    // int                        frame_info_idx;  // added for retrieving timecode of current frame;   e.g. handle->audio_sector.frame[handle->frame_info_idx].timecode
    // int                        audio_frame_trimming;    // if No pauses included if 1.  Trimm out audioframes in trimecode interval [area_tracklist_time->start...+duration]
    // uint32_t                   count_frames;                              // keep the number of audio frames in a track (for verification)
    // int                        dsf_nopad;
    // int                        concatenate;
    // int                        id3_tag_mode;  // 0=no id3tag inserted; 1=id3v2.3/utf16; 2=miminal id3v2.3/iso8859-1;3=id3v2.3/iso8859-1; 4=id3v2.4/utf8;5=minimal id3v2.4/utf8
    // int                        artist_flag;
    // int                        performer_flag;
    // uint32_t                   total_sectors_iso;
}

// impl Drop for SacdNetReader {
//     fn drop(&mut self) {
//         self.close_reader();
//     }
// }

pub fn new(reader: SacdNetReader) -> Result<ScarletBookReader> {
    let sbreader = ScarletBookReader {
        reader,
        stereo_area_index: -1,
        mch_area_index: -1,
    };

    Ok(sbreader)
}

impl ScarletBookReader {
    pub fn read_master_toc(&mut self) -> Result<MasterToc> {
        let res = self.reader
            .read_data(consts::START_OF_MASTER_TOC, consts::MASTER_TOC_LEN).expect("should read toc bytes");
        Ok(MasterToc::from_bytes(&res)?)
    }
    // fn close_reader(&mut self) {
    //     let req = ServerRequest{
    //         r#type: req_type::DiscClose as i32,
    //         sector_offset: Some(0),
    //         sector_count: Some(0),
    //     };

    //     let _ = self.send_req(req);
    //     debug!("reader dropped and closed");
    // }

    // fn send_req(&mut self, req: ServerRequest) -> Result<ServerResponse> {
    //     let mut encoded_request = Vec::new();
    //     req.encode(&mut encoded_request)?;

    //     self.stream.write_all(&encoded_request)?;

    //     // The original C implementation of the ripper protocol
    //     // terminates the protobuf payload with a zero.
    //     let zero: u8 = 0;
    //     self.stream.write_all(&[zero])?;
    //     self.stream.flush()?;

    //     // Read response into a reasonably sized buffer
    //     // We can't read byte-by-byte looking for zero because protobuf messages
    //     // contain zero bytes naturally (e.g., when encoding the value 0)
    //     //
    //     // The server will terminate messages with a zero as well, but we
    //     // can't rely fully on that because the proto response might have
    //     // zeroes midstream. Fortunately, the C implementation uses nanopb
    //     // hard size limits on response payloads - `data` field is capped at 1MB,
    //     // and the other fields are fixed, so we can lean on that for reads
    //     let mut buffer = vec![0u8; 1024*1024];
    //     let bytes_read = self.stream.read(&mut buffer)?;
    //     buffer.truncate(bytes_read);

    //     // The C protocol appends a zero byte terminator after the protobuf message
    //     // We need to strip it before decoding
    //     if buffer.is_empty() {
    //         anyhow::bail!("No data received from server");
    //     }

    //     debug!("Received {} bytes: {:02x?}", buffer.len(), &buffer[..]);

    //     if buffer.last() == Some(&0) {
    //         buffer.pop();
    //     } else {
    //         anyhow::bail!("Expected zero terminator byte, got {:02x}", buffer.last().unwrap());
    //     }

    //     // Decode the protobuf message
    //     let response = match ServerResponse::decode(&buffer[..]) {
    //         Ok(resp) => resp,
    //         Err(err) => anyhow::bail!("failed to decode response: {}", err),
    //     };

    //     debug!("Decoded response: type={}, result={}", response.r#type, response.result);
    //     Ok(response)
    // }
}

// pub fn open_network_reader(ip_addr: IpAddr, port: u16) -> Result<SacdNetReader> {
//     let socket_addr = SocketAddr::new(ip_addr, port);
//     let stream = TcpStream::connect(socket_addr)?;

//     let mut handle = SacdNetReader{
//         stream,
//     };

//     let req = ServerRequest{
//         r#type: req_type::DiscOpen as i32,
//         sector_offset: Some(0),
//         sector_count: Some(0),
//     };

//     let response = handle.send_req(req)?;

//     // Check the response
//     if response.result != 0 || response.r#type != resp_type::DiscOpened as i32 {
//         anyhow::bail!("response result non-zero or incorrect type");
//     }

//     Ok(handle)
// }

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use super::*;

    fn init() {
        let _ = env_logger::builder().is_test(true).try_init();
    }

    // #[test]
    // fn it_works() {
    //     // init();
    //     // let handle = open_network_reader(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 130)), 2002).expect("should init");
    // }
}
