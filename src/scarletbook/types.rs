use crate::scarletbook::consts;
use anyhow::{Context, Result};
use byteorder::{BigEndian, ReadBytesExt};
use std::io::{self, Cursor, Read};

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

/// Language and character set information
#[derive(Debug, Clone)]
pub struct LocaleTable {
    /// ISO-639-1 language code (e.g., "en")
    pub language_code: [u8; 2],
    /// Character set code (1-7)
    pub character_set: u8,
    pub _reserved: u8,
}

impl LocaleTable {
    fn parse<R: Read>(reader: &mut R) -> Result<Self> {
        let mut language_code = [0u8; 2];
        reader.read_exact(&mut language_code)?;
        Ok(Self {
            language_code,
            character_set: reader.read_u8()?,
            _reserved: reader.read_u8()?,
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
    pub id: [u8; 8],      // SACDMTOC; Master_TOC_Signature
    pub version: Version, // Spec_Version   1.20 / 0x0114
    pub reserved01: [u8; 6],
    // Album_Info (48 bytes)
    pub album_set_size: u16,        // Album_Set_Size,2bytes,  1..6553
    pub album_sequence_number: u16, // Album_Sequence_Number
    pub reserved02: [u8; 4],
    pub album_catalog_number: [u8; 16], // Album_Catalog_Number, 16 bytes, String; 0x00 when empty, else padded with spaces for shorter strings
    pub album_genre: [GenreTable; 4],   // Album_Genre, 4x4 bytes;
    pub reserved03: [u8; 8],
    // Disc_Info (64 bytes)
    pub area_1_toc_1_start: u32, // LSN for AREA_TOC_1 of 2 channel - 2CH_TOC_1_Address, 4bytes, Uint32, values 0, 544
    pub area_1_toc_2_start: u32, // LSN for AREA_TOC_2 of 2 channel - 2CH_TOC_2_Address, 4bytes, Uint32
    pub area_2_toc_1_start: u32, // LSN for AREA_TOC_1 of M channel - MC_TOC_1_Address, 4bytes, Uint32
    pub area_2_toc_2_start: u32, // LSN for AREA_TOC_2 of M channel   - MC_TOC_2_Address, 4bytes, Uint32
    pub disc_flags: u8,          // Disc_Flags, 1 byte - Hybr, b7
    pub reserved04: [u8; 3],
    pub area_1_toc_size: u16, // Length in Sectors of AREA_TOC of  2ch - 2CH_TOC_Length, 2byte, Uint16, value 0, 5..
    pub area_2_toc_size: u16, // Length in Sectors of AREA_TOC of M channel - MC_TOC_Length, 2byte, Uint16, value 0, 37..
    pub disc_catalog_number: [u8; 16], // Disc_Catalog_Number, 16 bytes, String; 0x00 when empty, else padded with spaces for shorter strings
    pub disc_genre: [GenreTable; 4],   // Disc_Genre, 4x4 bytes
    pub disc_date_year: u16,           // Disc_Date , 4 bytes
    pub disc_date_month: u8,
    pub disc_date_day: u8,
    pub reserved05: [u8; 4],
    // Text_Channels (40 bytes)
    pub text_area_count: u8, // N_Text_Channels, 1 byte, Uint8  , values =0..8
    pub reserved06: [u8; 7],
    pub locales: [LocaleTable; 8], // N_Text_Channels values= 0...8
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
            LocaleTable::parse(reader)?,
            LocaleTable::parse(reader)?,
            LocaleTable::parse(reader)?,
            LocaleTable::parse(reader)?,
            LocaleTable::parse(reader)?,
            LocaleTable::parse(reader)?,
            LocaleTable::parse(reader)?,
            LocaleTable::parse(reader)?,
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

/// Frame format types for SACD audio
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FrameFormat {
    /// DST coded. Flexible format
    Dst = 0,
    /// Reserved
    Reserved = 1,
    /// Fixed format. 2-Channel Stereo, Plain DSD, 3 Frames in 14 Sectors
    Dsd3In14 = 2,
    /// Fixed format. 2-Channel Stereo, Plain DSD, 3 Frames in 16 Sectors
    Dsd3In16 = 3,
    /// Reserved for future use (4..15)
    Unknown(u8),
}

impl From<u8> for FrameFormat {
    fn from(val: u8) -> Self {
        match val {
            0 => FrameFormat::Dst,
            1 => FrameFormat::Reserved,
            2 => FrameFormat::Dsd3In14,
            3 => FrameFormat::Dsd3In16,
            n => FrameFormat::Unknown(n),
        }
    }
}

/// Total_Area_Play_Time
#[derive(Debug, Clone, Copy)]
pub struct PlayTime {
    pub minutes: u8, // 0-255
    pub seconds: u8, // 0-59
    pub frames: u8,  // 0-74
}

impl PlayTime {
    fn parse<R: Read>(reader: &mut R) -> io::Result<Self> {
        Ok(PlayTime {
            minutes: reader.read_u8()?,
            seconds: reader.read_u8()?,
            frames: reader.read_u8()?,
        })
    }
}

impl Version {
    fn parse<R: Read>(reader: &mut R) -> io::Result<Self> {
        Ok(Version {
            major: reader.read_u8()?,
            minor: reader.read_u8()?,
        })
    }
}

/// SACD Area Table of Contents
/// Scarlet Book: 'Area_TOC'
/// This struct represents the complete Area TOC structure (2048 bytes)
/// for either 2-channel or multi-channel audio areas on an SACD disc.
#[derive(Debug, Clone)]
pub struct AreaToc {
    // ===== A_TOC_0_Header (16 bytes) =====
    /// Area_TOC_Signature: "TWOCHTOC" or "MULCHTOC"
    pub id: [u8; 8],
    /// Spec_Version: 1.20 / 0x0114
    pub version: Version,
    /// Area_TOC_Length:  5..40 (total size of TOC); length of the Area_TOC in Sectors
    pub size: u16,

    // ===== Area_Data (112 bytes) =====
    /// Max_Byte_Rate: Max Average Byte Rate of Multiplexed Frames
    pub max_byte_rate: u32,
    /// FS_Code: 0x04 = (64 * 44.1 kHz) - others reserved
    pub sample_frequency: u8,
    /// Frame_Format: (bits 3-0 of Area_Flags byte)
    pub frame_format: FrameFormat,
    /// N_Channels
    pub channel_count: u8,
    /// Loudspeaker_Config: (bits 4-0 of Area_Config byte)
    pub loudspeaker_config: u8,
    /// Extra_Setting: (bits 7-5 of Area_Config byte)
    pub extra_setting: u8,
    /// Max_Available_Channels
    pub max_available_channels: u8,
    /// Area_Mute_Flags
    pub area_mute_flags: u8,
    /// Track_Attribute (bits 3-0 of Area_Copy_Management byte)
    pub track_attribute: u8,
    /// Total_Area_Play_Time
    pub total_playtime: PlayTime,
    /// Track_Offset
    pub track_offset: u8,
    /// N_Tracks (1-255)
    pub track_count: u8,
    /// Track_Area_Start_Address
    pub track_start: u32,
    /// Track_Area_End_Address
    pub track_end: u32,

    // ===== Text_Channels (40 bytes) =====
    /// N_Text_Channels
    pub text_area_count: u8,
    /// Language and character set for each text channel
    pub languages: Vec<LocaleTable>,

    // ===== List_Pointers (16 bytes) =====
    /// Track_Text_Ptr
    pub track_text_offset: u16,
    /// Index_List_Ptr
    pub index_list_offset: u16,
    /// Access_List_Ptr
    pub access_list_offset: u16,

    // ===== Area_Text (1904 bytes) =====
    /// Area_Description_Ptr
    pub area_description_offset: u16,
    /// Area_Copyright_Ptr
    pub copyright_offset: u16,
    /// Area_Description_Phonetic_Ptr
    pub area_description_phonetic_offset: u16,
    /// Area_Copyright_Phonetic_Ptr
    pub copyright_phonetic_offset: u16,
    /// Area_Text
    pub data: Vec<u8>,
}

impl AreaToc {
    /// Parse an Area TOC structure from a reader
    ///
    /// # Arguments
    /// * `reader` - A reader positioned at the start of an Area TOC sector
    ///
    /// # Format
    /// SACD uses big-endian byte order for multi-byte integers.
    /// Single-byte bitfields are extracted using the on-disc bit positions (big-endian layout).
    pub fn parse<R: Read>(reader: &mut R) -> Result<Self> {
        let mut id = [0u8; 8];
        reader.read_exact(&mut id)?;

        let version = Version::parse(reader)?;
        let size = reader.read_u16::<BigEndian>()?;

        let mut reserved01 = [0u8; 4];
        reader.read_exact(&mut reserved01)?;

        let max_byte_rate = reader.read_u32::<BigEndian>()?;
        let sample_frequency = reader.read_u8()?;

        // Area_Flags byte: On-disc format is big-endian layout
        // Bits 7-4: reserved02
        // Bits 3-0: frame_format
        let area_flags_byte = reader.read_u8()?;
        let frame_format = FrameFormat::from(area_flags_byte & 0x0F);

        let mut reserved03 = [0u8; 10];
        reader.read_exact(&mut reserved03)?;

        let channel_count = reader.read_u8()?;

        // Area_Config byte: On-disc format is big-endian layout
        // Bits 7-5: extra_setting
        // Bits 4-0: loudspeaker_config
        let area_config_byte = reader.read_u8()?;
        let extra_setting = (area_config_byte >> 5) & 0x07;
        let loudspeaker_config = area_config_byte & 0x1F;

        let max_available_channels = reader.read_u8()?;
        let area_mute_flags = reader.read_u8()?;

        let mut reserved04 = [0u8; 12];
        reader.read_exact(&mut reserved04)?;

        // Area_Copy_Management byte: On-disc format is big-endian layout
        // Bits 7-4: reserved05
        // Bits 3-0: track_attribute
        let copy_mgmt_byte = reader.read_u8()?;
        let track_attribute = copy_mgmt_byte & 0x0F;

        let mut reserved06 = [0u8; 15];
        reader.read_exact(&mut reserved06)?;

        let total_playtime = PlayTime::parse(reader)?;

        let _reserved07 = reader.read_u8()?;
        let track_offset = reader.read_u8()?;
        let track_count = reader.read_u8()?;

        let mut reserved08 = [0u8; 2];
        reader.read_exact(&mut reserved08)?;

        let track_start = reader.read_u32::<BigEndian>()?;
        let track_end = reader.read_u32::<BigEndian>()?;

        let text_area_count = reader.read_u8()?;

        let mut reserved09 = [0u8; 7];
        reader.read_exact(&mut reserved09)?;

        let mut languages = Vec::with_capacity(consts::MAX_LANGUAGE_COUNT);
        for _ in 0..consts::MAX_LANGUAGE_COUNT {
            languages.push(LocaleTable::parse(reader)?);
        }

        let mut reserved091 = [0u8; 8];
        reader.read_exact(&mut reserved091)?;

        let track_text_offset = reader.read_u16::<BigEndian>()?;
        let index_list_offset = reader.read_u16::<BigEndian>()?;
        let access_list_offset = reader.read_u16::<BigEndian>()?;

        let mut reserved10 = [0u8; 10];
        reader.read_exact(&mut reserved10)?;

        let area_description_offset = reader.read_u16::<BigEndian>()?;
        let copyright_offset = reader.read_u16::<BigEndian>()?;
        let area_description_phonetic_offset = reader.read_u16::<BigEndian>()?;
        let copyright_phonetic_offset = reader.read_u16::<BigEndian>()?;

        let mut data = vec![0u8; 1896];
        reader.read_exact(&mut data)?;

        Ok(AreaToc {
            id,
            version,
            size,
            max_byte_rate,
            sample_frequency,
            frame_format,
            channel_count,
            loudspeaker_config,
            extra_setting,
            max_available_channels,
            area_mute_flags,
            track_attribute,
            total_playtime,
            track_offset,
            track_count,
            track_start,
            track_end,
            text_area_count,
            languages,
            track_text_offset,
            index_list_offset,
            access_list_offset,
            area_description_offset,
            copyright_offset,
            area_description_phonetic_offset,
            copyright_phonetic_offset,
            data,
        })
    }

    /// Get the area ID as a string (e.g., "TWOCHTOC" or "MULCHTOC")
    pub fn id_string(&self) -> Result<String, std::str::Utf8Error> {
        std::str::from_utf8(&self.id).map(|s| s.to_string())
    }

    /// Check if this is a 2-channel area
    pub fn is_two_channel(&self) -> bool {
        &self.id == consts::AREA_TOC_SIGNATURE_STEREO
    }

    /// Check if this is a multi-channel area
    pub fn is_multi_channel(&self) -> bool {
        &self.id == consts::AREA_TOC_SIGNATURE_MCH
    }
}
