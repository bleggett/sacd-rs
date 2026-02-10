use crate::scarletbook::consts;
use anyhow::Result;
use byteorder::{BigEndian, ReadBytesExt};
use std::io::{self, Read};

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayTime {
    pub minutes: u8, // 0-255
    pub seconds: u8, // 0-59
    pub frames: u8,  // 0-74
}

impl PlayTime {
    pub fn parse<R: Read>(reader: &mut R) -> io::Result<Self> {
        Ok(PlayTime {
            minutes: reader.read_u8()?,
            seconds: reader.read_u8()?,
            frames: reader.read_u8()?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenreTable {
    pub category: u8, // Genre_Table_Category, 1 byte, Uint8; 0=Not used, 1=General, 2=Japanese
    pub reserved: u8,
    pub genre: u16, // Genre_Table_Genre, 2 bytes, Uint16
}

impl GenreTable {
    pub fn read_from<R: Read>(reader: &mut R) -> Result<Self> {
        Ok(Self {
            category: reader.read_u8()?,
            reserved: reader.read_u8()?,
            genre: reader.read_u16::<BigEndian>()?,
        })
    }

    /// Get category name
    pub fn category_name(&self) -> Option<&'static str> {
        match self.category {
            0 => Some("Not used"),
            1 => Some("General"),
            2 => Some("Japanese"),
            _ => None,
        }
    }

    /// Get genre name (only valid when category == 1)
    pub fn genre_name(&self) -> Option<&'static str> {
        if self.category == 1 && (self.genre as usize) < consts::GENRE_NAMES.len() {
            Some(consts::GENRE_NAMES[self.genre as usize])
        } else {
            None
        }
    }
}

/// Language and character set information
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocaleTable {
    /// ISO-639-1 language code (e.g., "en")
    pub language_code: [u8; 2],
    /// Character set code (1-7)
    pub character_set: u8,
    pub _reserved: u8,
}

impl LocaleTable {
    pub fn parse<R: Read>(reader: &mut R) -> Result<Self> {
        let mut language_code = [0u8; 2];
        reader.read_exact(&mut language_code)?;
        Ok(Self {
            language_code,
            character_set: reader.read_u8()?,
            _reserved: reader.read_u8()?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version {
    pub major: u8,
    pub minor: u8,
}
impl Version {
    pub fn parse<R: Read>(reader: &mut R) -> Result<Self> {
        Ok(Self {
            major: reader.read_u8()?,
            minor: reader.read_u8()?,
        })
    }
}

/// Track text type identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TrackTextType {
    Title = 0x01,
    Performer = 0x02,
    Songwriter = 0x03,
    Composer = 0x04,
    Arranger = 0x05,
    Message = 0x06,
    ExtraMessage = 0x07,
    Copyright = 0x08,
    TitlePhonetic = 0x81,
    PerformerPhonetic = 0x82,
    SongwriterPhonetic = 0x83,
    ComposerPhonetic = 0x84,
    ArrangerPhonetic = 0x85,
    MessagePhonetic = 0x86,
    ExtraMessagePhonetic = 0x87,
    CopyrightPhonetic = 0x88,
    Unknown(u8),
}

impl From<u8> for TrackTextType {
    fn from(val: u8) -> Self {
        match val {
            0x01 => TrackTextType::Title,
            0x02 => TrackTextType::Performer,
            0x03 => TrackTextType::Songwriter,
            0x04 => TrackTextType::Composer,
            0x05 => TrackTextType::Arranger,
            0x06 => TrackTextType::Message,
            0x07 => TrackTextType::ExtraMessage,
            0x08 => TrackTextType::Copyright,
            0x81 => TrackTextType::TitlePhonetic,
            0x82 => TrackTextType::PerformerPhonetic,
            0x83 => TrackTextType::SongwriterPhonetic,
            0x84 => TrackTextType::ComposerPhonetic,
            0x85 => TrackTextType::ArrangerPhonetic,
            0x86 => TrackTextType::MessagePhonetic,
            0x87 => TrackTextType::ExtraMessagePhonetic,
            0x88 => TrackTextType::CopyrightPhonetic,
            n => TrackTextType::Unknown(n),
        }
    }
}

/// Area_Tracklist_Time (from SACDTRL2)
/// Track time information for start time or duration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackTime {
    pub minutes: u8, // Minutes, 1 byte, Uint8, values 0..255
    pub seconds: u8, // Seconds, 1 byte, Uint8, values 0..59
    pub frames: u8,  // Frames, 1 byte, Uint8, values 0..74
    pub flags: u8,   // Track_Flags, 1 byte; b7=ILP, b4-b1=TMF4-TMF1, b6,b5,b0 reserved
}

impl TrackTime {
    pub fn parse<R: Read>(reader: &mut R) -> io::Result<Self> {
        Ok(TrackTime {
            minutes: reader.read_u8()?,
            seconds: reader.read_u8()?,
            frames: reader.read_u8()?,
            flags: reader.read_u8()?,
        })
    }
}

/// ISRC (International Standard Recording Code) from Area_ISRC_Genre (SACD_IGL)
/// Format: ISRC_Code[tno], 12 bytes, String
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Isrc {
    pub country_code: [u8; 2], // Country_Code, 2 bytes, String; ISO-3166-1 alpha-2 (e.g., "GB")
    pub owner_code: [u8; 3],   // Owner_Code, 3 bytes, String; alphanumeric (e.g., "AAA")
    pub recording_year: [u8; 2], // Year_of_Recording, 2 bytes, String; last 2 digits of year (e.g., "94")
    pub designation_code: [u8; 5], // Designation_Code, 5 bytes, String; numeric (e.g., "00468")
}

impl Isrc {
    pub fn parse<R: Read>(reader: &mut R) -> io::Result<Self> {
        let mut country_code = [0u8; 2];
        reader.read_exact(&mut country_code)?;
        let mut owner_code = [0u8; 3];
        reader.read_exact(&mut owner_code)?;
        let mut recording_year = [0u8; 2];
        reader.read_exact(&mut recording_year)?;
        let mut designation_code = [0u8; 5];
        reader.read_exact(&mut designation_code)?;

        Ok(Isrc {
            country_code,
            owner_code,
            recording_year,
            designation_code,
        })
    }

    /// Check if this ISRC has valid data (non-zero country code)
    pub fn is_valid(&self) -> bool {
        self.country_code[0] != 0
    }

    /// Get ISRC as a formatted string (e.g., "GBAAA9400468")
    pub fn to_string(&self) -> String {
        let mut result = String::with_capacity(12);
        result.push_str(&String::from_utf8_lossy(&self.country_code));
        result.push_str(&String::from_utf8_lossy(&self.owner_code));
        result.push_str(&String::from_utf8_lossy(&self.recording_year));
        result.push_str(&String::from_utf8_lossy(&self.designation_code));
        result
    }
}

/// Track metadata text
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrackText {
    pub title: Option<String>,
    pub performer: Option<String>,
    pub songwriter: Option<String>,
    pub composer: Option<String>,
    pub arranger: Option<String>,
    pub message: Option<String>,
    pub extra_message: Option<String>,
    pub copyright: Option<String>,
    pub title_phonetic: Option<String>,
    pub performer_phonetic: Option<String>,
    pub songwriter_phonetic: Option<String>,
    pub composer_phonetic: Option<String>,
    pub arranger_phonetic: Option<String>,
    pub message_phonetic: Option<String>,
    pub extra_message_phonetic: Option<String>,
    pub copyright_phonetic: Option<String>,
}

impl TrackText {
    pub fn set_text(&mut self, text_type: TrackTextType, text: String) {
        if text.is_empty() {
            return;
        }

        let target = match text_type {
            TrackTextType::Title => &mut self.title,
            TrackTextType::Performer => &mut self.performer,
            TrackTextType::Songwriter => &mut self.songwriter,
            TrackTextType::Composer => &mut self.composer,
            TrackTextType::Arranger => &mut self.arranger,
            TrackTextType::Message => &mut self.message,
            TrackTextType::ExtraMessage => &mut self.extra_message,
            TrackTextType::Copyright => &mut self.copyright,
            TrackTextType::TitlePhonetic => &mut self.title_phonetic,
            TrackTextType::PerformerPhonetic => &mut self.performer_phonetic,
            TrackTextType::SongwriterPhonetic => &mut self.songwriter_phonetic,
            TrackTextType::ComposerPhonetic => &mut self.composer_phonetic,
            TrackTextType::ArrangerPhonetic => &mut self.arranger_phonetic,
            TrackTextType::MessagePhonetic => &mut self.message_phonetic,
            TrackTextType::ExtraMessagePhonetic => &mut self.extra_message_phonetic,
            TrackTextType::CopyrightPhonetic => &mut self.copyright_phonetic,
            TrackTextType::Unknown(_) => return,
        };

        *target = Some(text);
    }
}

// #[derive(Debug, Clone)]
// pub struct MasterToc {
//     // M_TOC_0_Header (16 bytes)
//     pub id: [u8; 8],      // SACDMTOC; Master_TOC_Signature
//     pub version: Version, // Spec_Version   1.20 / 0x0114
//     pub reserved01: [u8; 6],
//     // Album_Info (48 bytes)
//     pub album_set_size: u16,        // Album_Set_Size,2bytes,  1..6553
//     pub album_sequence_number: u16, // Album_Sequence_Number
//     pub reserved02: [u8; 4],
//     pub album_catalog_number: [u8; 16], // Album_Catalog_Number, 16 bytes, String; 0x00 when empty, else padded with spaces for shorter strings
//     pub album_genre: [GenreTable; 4],   // Album_Genre, 4x4 bytes;
//     pub reserved03: [u8; 8],
//     // Disc_Info (64 bytes)
//     pub area_1_toc_1_start: u32, // LSN for AREA_TOC_1 of 2 channel - 2CH_TOC_1_Address, 4bytes, Uint32, values 0, 544
//     pub area_1_toc_2_start: u32, // LSN for AREA_TOC_2 of 2 channel - 2CH_TOC_2_Address, 4bytes, Uint32
//     pub area_2_toc_1_start: u32, // LSN for AREA_TOC_1 of M channel - MC_TOC_1_Address, 4bytes, Uint32
//     pub area_2_toc_2_start: u32, // LSN for AREA_TOC_2 of M channel   - MC_TOC_2_Address, 4bytes, Uint32
//     pub disc_flags: u8,          // Disc_Flags, 1 byte - Hybr, b7
//     pub reserved04: [u8; 3],
//     pub area_1_toc_size: u16, // Length in Sectors of AREA_TOC of  2ch - 2CH_TOC_Length, 2byte, Uint16, value 0, 5..
//     pub area_2_toc_size: u16, // Length in Sectors of AREA_TOC of M channel - MC_TOC_Length, 2byte, Uint16, value 0, 37..
//     pub disc_catalog_number: [u8; 16], // Disc_Catalog_Number, 16 bytes, String; 0x00 when empty, else padded with spaces for shorter strings
//     pub disc_genre: [GenreTable; 4],   // Disc_Genre, 4x4 bytes
//     pub disc_date_year: u16,           // Disc_Date , 4 bytes
//     pub disc_date_month: u8,
//     pub disc_date_day: u8,
//     pub reserved05: [u8; 4],
//     // Text_Channels (40 bytes)
//     pub text_area_count: u8, // N_Text_Channels, 1 byte, Uint8  , values =0..8
//     pub reserved06: [u8; 7],
//     pub locales: [LocaleTable; 8], // N_Text_Channels values= 0...8
// }

// impl MasterToc {
//     pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
//         let mut cursor = Cursor::new(bytes);
//         Self::read_from(&mut cursor)
//     }

//     pub fn read_from<R: Read>(reader: &mut R) -> Result<Self> {
//         let mut id = [0u8; 8];
//         reader.read_exact(&mut id)?;

//         let version = Version::parse(reader)?;

//         let mut reserved01 = [0u8; 6];
//         reader.read_exact(&mut reserved01)?;

//         let album_set_size = reader.read_u16::<BigEndian>()?;
//         let album_sequence_number = reader.read_u16::<BigEndian>()?;

//         let mut reserved02 = [0u8; 4];
//         reader.read_exact(&mut reserved02)?;

//         let mut album_catalog_number = [0u8; 16];
//         reader.read_exact(&mut album_catalog_number)?;

//         let album_genre = [
//             GenreTable::read_from(reader)?,
//             GenreTable::read_from(reader)?,
//             GenreTable::read_from(reader)?,
//             GenreTable::read_from(reader)?,
//         ];

//         let mut reserved03 = [0u8; 8];
//         reader.read_exact(&mut reserved03)?;

//         let area_1_toc_1_start = reader.read_u32::<BigEndian>()?;
//         let area_1_toc_2_start = reader.read_u32::<BigEndian>()?;
//         let area_2_toc_1_start = reader.read_u32::<BigEndian>()?;
//         let area_2_toc_2_start = reader.read_u32::<BigEndian>()?;

//         let disc_flags = reader.read_u8()?;

//         let mut reserved04 = [0u8; 3];
//         reader.read_exact(&mut reserved04)?;

//         let area_1_toc_size = reader.read_u16::<BigEndian>()?;
//         let area_2_toc_size = reader.read_u16::<BigEndian>()?;

//         let mut disc_catalog_number = [0u8; 16];
//         reader.read_exact(&mut disc_catalog_number)?;

//         let disc_genre = [
//             GenreTable::read_from(reader)?,
//             GenreTable::read_from(reader)?,
//             GenreTable::read_from(reader)?,
//             GenreTable::read_from(reader)?,
//         ];

//         let disc_date_year = reader.read_u16::<BigEndian>()?;
//         let disc_date_month = reader.read_u8()?;
//         let disc_date_day = reader.read_u8()?;

//         let mut reserved05 = [0u8; 4];
//         reader.read_exact(&mut reserved05)?;

//         // Text_Channels (40 bytes)
//         let text_area_count = reader.read_u8()?;

//         let mut reserved06 = [0u8; 7];
//         reader.read_exact(&mut reserved06)?;

//         let locales = [
//             LocaleTable::parse(reader)?,
//             LocaleTable::parse(reader)?,
//             LocaleTable::parse(reader)?,
//             LocaleTable::parse(reader)?,
//             LocaleTable::parse(reader)?,
//             LocaleTable::parse(reader)?,
//             LocaleTable::parse(reader)?,
//             LocaleTable::parse(reader)?,
//         ];

//         Ok(Self {
//             id,
//             version,
//             reserved01,
//             album_set_size,
//             album_sequence_number,
//             reserved02,
//             album_catalog_number,
//             album_genre,
//             reserved03,
//             area_1_toc_1_start,
//             area_1_toc_2_start,
//             area_2_toc_1_start,
//             area_2_toc_2_start,
//             disc_flags,
//             reserved04,
//             area_1_toc_size,
//             area_2_toc_size,
//             disc_catalog_number,
//             disc_genre,
//             disc_date_year,
//             disc_date_month,
//             disc_date_day,
//             reserved05,
//             text_area_count,
//             reserved06,
//             locales,
//         })
//     }

//     /// Validate that this is a valid Master TOC
//     pub fn is_valid(&self) -> bool {
//         &self.id == consts::MASTER_TOC_SIGNATURE
//     }

//     /// Get ID as string
//     pub fn id_string(&self) -> String {
//         String::from_utf8_lossy(&self.id).to_string()
//     }

//     /// Get album catalog number as string (trimmed)
//     pub fn album_catalog(&self) -> String {
//         String::from_utf8_lossy(&self.album_catalog_number)
//             .trim_end_matches('\0')
//             .trim()
//             .to_string()
//     }

//     /// Get disc catalog number as string (trimmed)
//     pub fn disc_catalog(&self) -> String {
//         String::from_utf8_lossy(&self.disc_catalog_number)
//             .trim_end_matches('\0')
//             .trim()
//             .to_string()
//     }

//     /// Check if disc is hybrid (has both SACD and CD layers)
//     pub fn is_hybrid(&self) -> bool {
//         (self.disc_flags & 0x80) != 0
//     }

//     /// Has two-channel area
//     pub fn has_two_channel(&self) -> bool {
//         self.area_1_toc_1_start != 0
//     }

//     /// Has multi-channel area
//     pub fn has_multi_channel(&self) -> bool {
//         self.area_2_toc_1_start != 0
//     }

//     /// Get language code for a locale index
//     pub fn get_language(&self, index: usize) -> Option<String> {
//         if index < self.text_area_count as usize {
//             String::from_utf8(self.locales[index].language_code.to_vec()).ok()
//         } else {
//             None
//         }
//     }

//     /// Get disc category (from first genre table entry with category=1)
//     pub fn disc_category(&self) -> Option<&'static str> {
//         self.disc_genre
//             .iter()
//             .find(|g| g.category == 1)
//             .and_then(|g| g.category_name())
//     }

//     /// Get disc genre (from first genre table entry with category=1)
//     pub fn disc_genre(&self) -> Option<&'static str> {
//         self.disc_genre
//             .iter()
//             .find(|g| g.category == 1)
//             .and_then(|g| g.genre_name())
//     }
// }
