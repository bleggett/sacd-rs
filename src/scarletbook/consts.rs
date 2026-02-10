pub const START_OF_MASTER_TOC: u32 = 510;
pub const MASTER_TOC_LEN: u32 = 10;
pub const MAX_LANGUAGE_COUNT: usize = 8;
pub const MASTER_TOC_SIGNATURE: &[u8; 8] = b"SACDMTOC";
pub const AREA_TOC_SIGNATURE_STEREO: &[u8; 8] = b"TWOCHTOC";
pub const AREA_TOC_SIGNATURE_MCH: &[u8; 8] = b"MULCHTOC";
pub const SACD_LSN_SIZE: usize = 2048;
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
