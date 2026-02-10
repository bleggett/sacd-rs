use crate::scarletbook::consts;
use anyhow::Result;
use byteorder::{BigEndian, ReadBytesExt};
use std::io::{self, Cursor, Read};

#[derive(Debug, Clone, Default)]
pub struct MasterText {
    pub album_title: Option<String>,
    pub album_artist: Option<String>,
    pub album_publisher: Option<String>,
    pub album_copyright: Option<String>,
    pub disc_title: Option<String>,
    pub disc_artist: Option<String>,
    pub disc_publisher: Option<String>,
    pub disc_copyright: Option<String>,
}

impl MasterText {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 2048 {
            anyhow::bail!("Master text data too short");
        }

        let mut cursor = Cursor::new(bytes);

        // Read and verify ID
        let mut id = [0u8; 8];
        cursor.read_exact(&mut id)?;
        if &id != b"SACDText" {
            anyhow::bail!("Invalid master text signature");
        }

        // Skip reserved
        cursor.set_position(16);

        // Read all the position pointers
        let album_title_pos = cursor.read_u16::<BigEndian>()?;
        let album_artist_pos = cursor.read_u16::<BigEndian>()?;
        let album_publisher_pos = cursor.read_u16::<BigEndian>()?;
        let album_copyright_pos = cursor.read_u16::<BigEndian>()?;
        let _album_title_phonetic_pos = cursor.read_u16::<BigEndian>()?;
        let _album_artist_phonetic_pos = cursor.read_u16::<BigEndian>()?;
        let _album_publisher_phonetic_pos = cursor.read_u16::<BigEndian>()?;
        let _album_copyright_phonetic_pos = cursor.read_u16::<BigEndian>()?;
        let disc_title_pos = cursor.read_u16::<BigEndian>()?;
        let disc_artist_pos = cursor.read_u16::<BigEndian>()?;
        let disc_publisher_pos = cursor.read_u16::<BigEndian>()?;
        let disc_copyright_pos = cursor.read_u16::<BigEndian>()?;

        // Helper function to extract null-terminated string from position
        let extract_string = |pos: u16| -> Option<String> {
            if pos == 0 || pos as usize >= bytes.len() {
                return None;
            }
            let start = pos as usize;
            let end = bytes[start..]
                .iter()
                .position(|&b| b == 0)
                .map(|i| start + i)
                .unwrap_or(bytes.len());
            let s = String::from_utf8_lossy(&bytes[start..end])
                .trim()
                .to_string();
            if s.is_empty() { None } else { Some(s) }
        };

        Ok(MasterText {
            album_title: extract_string(album_title_pos),
            album_artist: extract_string(album_artist_pos),
            album_publisher: extract_string(album_publisher_pos),
            album_copyright: extract_string(album_copyright_pos),
            disc_title: extract_string(disc_title_pos),
            disc_artist: extract_string(disc_artist_pos),
            disc_publisher: extract_string(disc_publisher_pos),
            disc_copyright: extract_string(disc_copyright_pos),
        })
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenreTable {
    pub category: u8, // Genre_Table_Category, 1 byte, Uint8; 0=Not used, 1=General, 2=Japanese
    pub reserved: u8,
    pub genre: u16, // Genre_Table_Genre, 2 bytes, Uint16
}

impl GenreTable {
    fn read_from<R: Read>(reader: &mut R) -> Result<Self> {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version {
    pub major: u8,
    pub minor: u8,
}
impl Version {
    fn parse<R: Read>(reader: &mut R) -> Result<Self> {
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
    fn parse<R: Read>(reader: &mut R) -> io::Result<Self> {
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
    fn parse<R: Read>(reader: &mut R) -> io::Result<Self> {
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
    fn set_text(&mut self, text_type: TrackTextType, text: String) {
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

        let version = Version::parse(reader)?;

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
        &self.id == consts::MASTER_TOC_SIGNATURE
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

    /// Get disc category (from first genre table entry with category=1)
    pub fn disc_category(&self) -> Option<&'static str> {
        self.disc_genre
            .iter()
            .find(|g| g.category == 1)
            .and_then(|g| g.category_name())
    }

    /// Get disc genre (from first genre table entry with category=1)
    pub fn disc_genre(&self) -> Option<&'static str> {
        self.disc_genre
            .iter()
            .find(|g| g.category == 1)
            .and_then(|g| g.genre_name())
    }
}

/// SACD Area Table of Contents
/// Scarlet Book: 'Area_TOC'
/// This struct represents the complete Area TOC structure (2048 bytes)
/// for either 2-channel or multi-channel audio areas on an SACD disc.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// Area_Description (parsed string)
    pub area_description: Option<String>,
    /// Area_Copyright (parsed string)
    pub copyright: Option<String>,
    /// Area_Description_Phonetic (parsed string)
    pub area_description_phonetic: Option<String>,
    /// Area_Copyright_Phonetic (parsed string)
    pub copyright_phonetic: Option<String>,
    /// Area_Text
    pub track_texts: Vec<TrackText>,

    // ===== Area_Tracklist_Time (from Track_List_2 / SACDTRL2) =====
    /// Track_Start[tno], Vec of area_tracklist_time_t, one per track
    pub track_times_start: Vec<TrackTime>,
    /// Track_Duration[tno], Vec of area_tracklist_time_t, one per track
    pub track_times_duration: Vec<TrackTime>,

    // ===== Area_ISRC_Genre (from ISRC_and_Genre_List / SACD_IGL) =====
    /// ISRC_Code[tno], Vec of isrc_t, one per track; International Standard Recording Code
    pub track_isrc: Vec<Isrc>,
}

impl AreaToc {
    /// Parse an Area TOC structure from a reader
    ///
    /// # Arguments
    /// * `reader` - A reader positioned at the start of an Area TOC sector
    ///
    /// # Note
    /// This reads ALL sectors for the area (based on the `size` field) to parse track text.
    pub fn parse<R: Read>(reader: &mut R) -> Result<Self> {
        // Read first sector to get the size
        let mut first_sector = [0u8; consts::SACD_LSN_SIZE];
        reader.read_exact(&mut first_sector)?;

        let size = u16::from_be_bytes([first_sector[10], first_sector[11]]);

        // Read remaining sectors
        let additional_bytes = ((size as usize).saturating_sub(1)) * consts::SACD_LSN_SIZE;
        let mut complete_area_data = Vec::with_capacity(size as usize * consts::SACD_LSN_SIZE);
        complete_area_data.extend_from_slice(&first_sector);

        if additional_bytes > 0 {
            let start_len = complete_area_data.len();
            complete_area_data.resize(start_len + additional_bytes, 0);
            reader.read_exact(&mut complete_area_data[start_len..])?;
        }

        Self::from_bytes(&complete_area_data)
    }

    /// Parse from complete area data buffer
    ///
    /// # Arguments
    /// * `area_data` - Complete area TOC data (all sectors)
    ///
    /// # Format
    /// SACD uses big-endian byte order for multi-byte integers.
    /// Single-byte bitfields are extracted using the on-disc bit positions (big-endian layout).
    pub fn from_bytes(area_data: &[u8]) -> Result<Self> {
        let mut reader = Cursor::new(area_data);
        let mut id = [0u8; 8];
        reader.read_exact(&mut id)?;

        let version = Version::parse(&mut reader)?;
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

        let total_playtime = PlayTime::parse(&mut reader)?;

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
            languages.push(LocaleTable::parse(&mut reader)?);
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

        // Parse track text from complete area data
        let track_texts =
            Self::parse_track_text(area_data, track_text_offset, track_count, &languages)?;

        // Parse track times and ISRC from complete area data
        let (track_times_start, track_times_duration) =
            Self::parse_track_times(area_data, size, track_count)?;
        let track_isrc = Self::parse_track_isrc(area_data, size, track_count)?;

        // Parse area description and copyright strings
        let area_description = if area_description_offset > 0
            && (area_description_offset as usize) < area_data.len()
        {
            Self::read_text_at_offset(area_data, area_description_offset as usize)
        } else {
            None
        };

        let copyright = if copyright_offset > 0 && (copyright_offset as usize) < area_data.len() {
            Self::read_text_at_offset(area_data, copyright_offset as usize)
        } else {
            None
        };

        let area_description_phonetic = if area_description_phonetic_offset > 0
            && (area_description_phonetic_offset as usize) < area_data.len()
        {
            Self::read_text_at_offset(area_data, area_description_phonetic_offset as usize)
        } else {
            None
        };

        let copyright_phonetic = if copyright_phonetic_offset > 0
            && (copyright_phonetic_offset as usize) < area_data.len()
        {
            Self::read_text_at_offset(area_data, copyright_phonetic_offset as usize)
        } else {
            None
        };

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
            area_description,
            copyright,
            area_description_phonetic,
            copyright_phonetic,
            track_texts,
            track_times_start,
            track_times_duration,
            track_isrc,
        })
    }

    /// Get the area ID as a string (e.g., "TWOCHTOC" or "MULCHTOC")
    pub fn id_string(&self) -> Result<String, std::str::Utf8Error> {
        std::str::from_utf8(&self.id).map(|s| s.to_string())
    }

    /// Check if this is a 2-channel area (Scarlet Book definition)
    /// A 2-channel area has N_Channels=2 and Loudspeaker_Config=0
    pub fn is_two_channel(&self) -> bool {
        self.channel_count == 2 && self.loudspeaker_config == 0
    }

    /// Check if this is a multi-channel area (Scarlet Book definition)
    /// Per the Scarlet Book spec and reference implementation, a multi-channel area
    /// is any area that is not a 2-channel area (i.e., not N_Channels=2 with Loudspeaker_Config=0)
    pub fn is_multi_channel(&self) -> bool {
        !(self.channel_count == 2 && self.loudspeaker_config == 0)
    }

    /// Parse track text from the raw area data
    ///
    /// This parses the "SACDTTxt" section which contains metadata for each track.
    ///
    /// # Arguments
    /// Read null-terminated text at given offset in area data
    ///
    /// # Arguments
    /// * `area_data` - Complete area data buffer
    /// * `offset` - Byte offset to the text
    ///
    /// # Returns
    /// Parsed UTF-8 string, or None if invalid
    fn read_text_at_offset(area_data: &[u8], offset: usize) -> Option<String> {
        if offset >= area_data.len() {
            return None;
        }

        // Find null terminator
        let end = area_data[offset..]
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(area_data.len() - offset);

        // Convert to UTF-8 string
        String::from_utf8(area_data[offset..offset + end].to_vec()).ok()
    }

    /// Parse track text
    ///
    /// # Arguments
    /// * `area_data` - Complete area data (multiple sectors, starting from Area TOC)
    /// * `track_text_offset` - Offset in sectors to the track text
    /// * `track_count` - Number of tracks
    /// * `languages` - Language/charset information
    ///
    /// # Returns
    /// Vector of TrackText, one for each track in the area
    fn parse_track_text(
        area_data: &[u8],
        track_text_offset: u16,
        track_count: u8,
        languages: &[LocaleTable],
    ) -> Result<Vec<TrackText>> {
        if track_text_offset == 0 {
            // No track text present
            return Ok(vec![TrackText::default(); track_count as usize]);
        }

        // Track text starts at track_text_offset sectors from the beginning
        let track_text_start = (track_text_offset as usize) * consts::SACD_LSN_SIZE;

        if track_text_start + 8 > area_data.len() {
            anyhow::bail!("Track text offset beyond area data bounds");
        }

        // Verify signature
        let signature = &area_data[track_text_start..track_text_start + 8];
        if signature != b"SACDTTxt" {
            anyhow::bail!("Invalid track text signature: expected 'SACDTTxt'");
        }

        let mut track_texts = vec![TrackText::default(); track_count as usize];

        // Get character set from first locale
        let _character_set = if !languages.is_empty() {
            languages[0].character_set
        } else {
            2 // Default to ISO-8859-1
        };

        // Parse track text positions
        let positions_start = track_text_start + 8;

        for track_idx in 0..track_count as usize {
            let pos_offset = positions_start + (track_idx * 2);

            if pos_offset + 2 > area_data.len() {
                continue;
            }

            // Read the track text position (big-endian u16)
            let track_text_pos =
                u16::from_be_bytes([area_data[pos_offset], area_data[pos_offset + 1]]) as usize;

            if track_text_pos == 0 {
                continue; // No text for this track
            }

            let text_start = track_text_start + track_text_pos;

            if text_start >= area_data.len() {
                continue;
            }

            // Read N_Items (number of text items for this track)
            let n_items = area_data[text_start] as usize;

            // Skip 4 bytes total (N_Items + 3 reserved bytes)
            let mut ptr = text_start + 4;

            for _ in 0..n_items {
                if ptr >= area_data.len() {
                    break;
                }

                // Read Text_Type (1 byte)
                let text_type = TrackTextType::from(area_data[ptr]);
                ptr += 1;

                // Read Padding1 (1 byte, should be 0x20)
                if ptr >= area_data.len() {
                    break;
                }
                let padding1 = area_data[ptr];
                ptr += 1;

                if padding1 != 0x20 {
                    eprintln!("Warning: Padding1 is not 0x20 (got 0x{:02x})", padding1);
                }

                // Read null-terminated string
                if ptr >= area_data.len() {
                    break;
                }

                let string_start = ptr;
                while ptr < area_data.len() && area_data[ptr] != 0 {
                    ptr += 1;
                }

                if ptr >= area_data.len() {
                    break;
                }

                let string_bytes = &area_data[string_start..ptr];

                // Convert to String, replacing invalid UTF-8 sequences
                // Note: This assumes the text is ASCII-compatible
                // For proper character set conversion, you'd need a charset conversion library
                let text = String::from_utf8_lossy(string_bytes).to_string();

                // Store in the appropriate field
                track_texts[track_idx].set_text(text_type, text);

                // Skip null terminator
                ptr += 1;

                // Skip Padding2 (0-3 bytes of 0x00)
                while ptr < area_data.len() && area_data[ptr] == 0 {
                    ptr += 1;
                }
            }
        }

        Ok(track_texts)
    }

    /// Parse Area_Tracklist_Time (Track_List_2) from the raw area data
    ///
    /// Searches for "SACDTRL2" (Track_List_2_Signature) sector containing:
    /// - Track_Start[255]: area_tracklist_time_t array for start times
    /// - Track_Duration[255]: area_tracklist_time_t array for durations
    ///
    /// # Arguments
    /// * `area_data` - Complete area data (multiple sectors, starting from Area TOC)
    /// * `size` - Area_TOC_Length in sectors
    /// * `track_count` - N_Tracks (1..255)
    ///
    /// # Returns
    /// Tuple of (Track_Start[], Track_Duration[]) vectors
    fn parse_track_times(
        area_data: &[u8],
        size: u16,
        track_count: u8,
    ) -> Result<(Vec<TrackTime>, Vec<TrackTime>)> {
        // Search for SACDTRL2 signature
        let mut offset = consts::SACD_LSN_SIZE; // Skip first sector (Area TOC header)
        let end_offset = (size as usize) * consts::SACD_LSN_SIZE;

        while offset + 8 <= end_offset {
            if &area_data[offset..offset + 8] == b"SACDTRL2" {
                // Found track time list
                let mut reader = Cursor::new(&area_data[offset..]);

                // Skip signature
                reader.set_position(8);

                let mut start_times = Vec::with_capacity(track_count as usize);
                let mut durations = Vec::with_capacity(track_count as usize);

                // SACDTRL2 structure has 255 fixed entries for start times
                // Read all 255 start times, but only keep track_count entries
                for i in 0..255 {
                    let time = TrackTime::parse(&mut reader)?;
                    if i < track_count {
                        start_times.push(time);
                    }
                }

                // Then read all 255 durations, but only keep track_count entries
                for i in 0..255 {
                    let time = TrackTime::parse(&mut reader)?;
                    if i < track_count {
                        durations.push(time);
                    }
                }

                return Ok((start_times, durations));
            }
            offset += consts::SACD_LSN_SIZE;
        }

        // If not found, return empty vectors
        Ok((vec![], vec![]))
    }

    /// Parse Area_ISRC_Genre (ISRC_and_Genre_List) from the raw area data
    ///
    /// Searches for "SACD_IGL" (ISRC_and_Genre_List_Signature) sector containing:
    /// - ISRC_Code[255]: isrc_t array (12 bytes each)
    /// - Track_Genre[255]: genre_table_t array (4 bytes each)
    ///
    /// # Arguments
    /// * `area_data` - Complete area data (multiple sectors, starting from Area TOC)
    /// * `size` - Area_TOC_Length in sectors
    /// * `track_count` - N_Tracks (1..255)
    ///
    /// # Returns
    /// Vector of ISRC_Code[] (one per track)
    fn parse_track_isrc(area_data: &[u8], size: u16, track_count: u8) -> Result<Vec<Isrc>> {
        // Search for SACD_IGL signature
        let mut offset = consts::SACD_LSN_SIZE; // Skip first sector (Area TOC header)
        let end_offset = (size as usize) * consts::SACD_LSN_SIZE;

        while offset + 8 <= end_offset {
            if &area_data[offset..offset + 8] == b"SACD_IGL" {
                // Found ISRC and genre list
                let mut reader = Cursor::new(&area_data[offset..]);

                // Skip signature
                reader.set_position(8);

                let mut isrc_codes = Vec::with_capacity(track_count as usize);

                // SACD_IGL structure has 255 fixed ISRC entries
                // Read all 255 ISRC codes, but only keep track_count entries
                for i in 0..255 {
                    let isrc = Isrc::parse(&mut reader)?;
                    if i < track_count {
                        isrc_codes.push(isrc);
                    }
                }

                return Ok(isrc_codes);
            }
            offset += consts::SACD_LSN_SIZE;
        }

        // If not found, return empty vector
        Ok(vec![])
    }
}
