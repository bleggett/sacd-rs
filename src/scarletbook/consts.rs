pub const START_OF_MASTER_TOC: u32 = 510;
pub const MASTER_TOC_LEN: u32 = 10;
pub const MAX_LANGUAGE_COUNT: usize = 8;
pub const SACD_LSN_SIZE: usize = 2048;

// Eight-byte ASCII signatures that mark each on-disc structure. Kept as
// named constants here (rather than inlined as byte literals at parser
// sites, the way the C reference does) so all magic strings live in one
// place.
pub const MASTER_TOC_SIGNATURE: &[u8; 8] = b"SACDMTOC";
pub const MASTER_TEXT_SIGNATURE: &[u8; 8] = b"SACDText";
pub const AREA_TOC_SIGNATURE_STEREO: &[u8; 8] = b"TWOCHTOC";
pub const AREA_TOC_SIGNATURE_MCH: &[u8; 8] = b"MULCHTOC";
pub const AREA_TRACK_TEXT_SIGNATURE: &[u8; 8] = b"SACDTTxt";
pub const AREA_TRACK_LIST_1_SIGNATURE: &[u8; 8] = b"SACDTRL1";
pub const AREA_TRACK_LIST_2_SIGNATURE: &[u8; 8] = b"SACDTRL2";
pub const AREA_ISRC_GENRE_SIGNATURE: &[u8; 8] = b"SACD_IGL";
pub const GENRE_NAMES: [&str; 29] = [
    "Not used",
    "Not defined",
    "Adult Contemporary",
    "Alternative Rock",
    "Children's Music",
    "Classical",
    "Contemporary Christian",
    "Country",
    "Dance",
    "Easy Listening",
    "Erotic",
    "Folk",
    "Gospel",
    "Hip Hop",
    "Jazz",
    "Latin",
    "Musical",
    "New Age",
    "Opera",
    "Operetta",
    "Pop Music",
    "RAP",
    "Reggae",
    "Rock Music",
    "Rhythm & Blues",
    "Sound Effects",
    "Sound Track",
    "Spoken Word",
    "World Music",
];
