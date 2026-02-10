use crate::sacd_net_reader::SacdNetReader;
use anyhow::{Context, Result};
use log::warn;

use crate::scarletbook::consts;
use crate::scarletbook::types::{AreaToc, MasterToc, MasterText};

pub struct ScarletBookReader {
    reader: SacdNetReader,
    master_toc: MasterToc,
    master_text: Option<MasterText>,
    stereo_toc: Option<AreaToc>,
    stereo_area_index: i32,
    mch_area_index: i32,
    total_sectors: Option<u32>,
}

pub fn new(mut reader: SacdNetReader) -> Result<ScarletBookReader> {
    let master_toc = read_master_toc(&mut reader)?;

    let master_text = read_master_text(&mut reader);

    let stereo_toc = read_stereo_toc(&master_toc, &mut reader);

    let sbreader = ScarletBookReader {
        reader,
        master_toc,
        master_text,
        stereo_toc,
        stereo_area_index: -1,
        mch_area_index: -1,
        total_sectors: None,
    };

    Ok(sbreader)
}

impl ScarletBookReader {
    pub fn get_master_toc(&self) -> MasterToc {
        self.master_toc.clone()
    }

    pub fn get_stereo_toc(&self) -> Option<AreaToc> {
        self.stereo_toc.clone()
    }

    /// Print disc and track information to stdout
    pub fn print_disc_info(&mut self) {
        self.print_disc_info_section();
        self.print_album_info_section();
        self.print_area_count();

        if let Some(ref stereo_toc) = self.stereo_toc {
            self.print_area_toc(stereo_toc, 0);
        }

        self.print_disc_size();
    }

    fn print_disc_info_section(&self) {
        let mtoc = &self.master_toc;

        println!("Disc Information:");
        println!("    Version: {:2}.{:02}", mtoc.version.major, mtoc.version.minor);
        println!("    Creation date: {:04}-{:02}-{:02}",
            mtoc.disc_date_year, mtoc.disc_date_month, mtoc.disc_date_day);

        let disc_catalog = mtoc.disc_catalog();
        if !disc_catalog.is_empty() {
            println!("    Disc Catalog Number: {}", disc_catalog);
        }

        // Print locales
        for locale in &mtoc.locales {
            let lang_code = String::from_utf8_lossy(&locale.language_code);
            let lang_code = lang_code.trim_end_matches('\0');
            if !lang_code.is_empty() {
                let charset_name = match locale.character_set & 0x07 {
                    0 => "US-ASCII",
                    1 => "ISO646-JP",
                    2 => "ISO-8859-1",
                    3 => "SHIFT_JISX0213",
                    4 => "KSC5601.1987-0",
                    5 => "GB2312.1980-0",
                    6 => "BIG5",
                    7 => "ISO-8859-1",
                    _ => "Unknown",
                };
                println!("    Locale: {}, Code character set:[{}], {}",
                    lang_code, locale.character_set, charset_name);
            }
        }

        // Print disc text from master text
        if let Some(ref mt) = self.master_text {
            if let Some(ref title) = mt.disc_title {
                println!("    Title: {}", title);
            }
            if let Some(ref artist) = mt.disc_artist {
                println!("    Artist: {}", artist);
            }
            if let Some(ref publisher) = mt.disc_publisher {
                println!("    Publisher: {}", publisher);
            }
            if let Some(ref copyright) = mt.disc_copyright {
                println!("    Copyright: {}", copyright);
            }
        }
    }

    fn print_album_info_section(&self) {
        let mtoc = &self.master_toc;

        println!("\nAlbum Information:");

        let album_catalog = String::from_utf8_lossy(&mtoc.album_catalog_number)
            .trim_end_matches('\0')
            .trim()
            .to_string();
        if !album_catalog.is_empty() {
            println!("    Album Catalog Number: {}", album_catalog);
        }

        println!("    Sequence Number: {}", mtoc.album_sequence_number);
        println!("    Set Size: {}", mtoc.album_set_size);

        // Print album text from master text
        if let Some(ref mt) = self.master_text {
            if let Some(ref title) = mt.album_title {
                println!("    Title: {}", title);
            }
            if let Some(ref artist) = mt.album_artist {
                println!("    Artist: {}", artist);
            }
            if let Some(ref publisher) = mt.album_publisher {
                println!("    Publisher: {}", publisher);
            }
            if let Some(ref copyright) = mt.album_copyright {
                println!("    Copyright: {}", copyright);
            }
        }
    }

    fn print_area_count(&self) {
        let area_count = if self.stereo_toc.is_some() { 1 } else { 0 };
        println!("\nArea count: {}", area_count);
    }

    fn print_area_toc(&self, area_toc: &AreaToc, area_idx: usize) {
        println!("    Area Information [{}]:\n", area_idx);
        println!("    Version: {:2}.{:02}", area_toc.version.major, area_toc.version.minor);
        println!("    Track Count: {}", area_toc.track_count);
        println!("    Total play time: {:02}:{:02}:{:02} [mins:secs:frames]",
            area_toc.total_playtime.minutes,
            area_toc.total_playtime.seconds,
            area_toc.total_playtime.frames);
        println!("    Speaker config: {} Channel", area_toc.channel_count);

        println!("    Track list [{}]:", area_idx);
        for (i, track_text) in area_toc.track_texts.iter().enumerate() {
            if let Some(ref title) = track_text.title {
                println!("        Title[{}]: {}", i, title);
            }
            if let Some(ref performer) = track_text.performer {
                println!("        Performer[{}]: {}", i, performer);
            }
            if let Some(ref composer) = track_text.composer {
                println!("        Composer[{}]: {}", i, composer);
            }

            // Print track start time and duration from Area_Tracklist_Time (SACDTRL2)
            if i < area_toc.track_times_start.len() && i < area_toc.track_times_duration.len() {
                let start = &area_toc.track_times_start[i];
                let duration = &area_toc.track_times_duration[i];
                println!("        Track_Start_Time_Code: {:02}:{:02}:{:02} [mins:secs:frames]",
                    start.minutes, start.seconds, start.frames);
                println!("        Duration: {:02}:{:02}:{:02} [mins:secs:frames]",
                    duration.minutes, duration.seconds, duration.frames);
            }

            println!();
        }

        // Print ISRC information from Area_ISRC_Genre (SACD_IGL)
        for (i, isrc) in area_toc.track_isrc.iter().enumerate() {
            if isrc.is_valid() {
                let country = String::from_utf8_lossy(&isrc.country_code);
                let owner = String::from_utf8_lossy(&isrc.owner_code);
                let year = String::from_utf8_lossy(&isrc.recording_year);
                let designation = String::from_utf8_lossy(&isrc.designation_code);

                println!("    ISRC Track [{}]:", i);
                println!("      Country: {}, Owner: {}, Year: {}, Designation: {}",
                    country, owner, year, designation);
            }
        }
    }

    fn get_total_sectors(&mut self) -> Result<u32> {
        if let Some(sectors) = self.total_sectors {
            Ok(sectors)
        } else {
            let sectors = self.reader.get_total_sectors()?;
            self.total_sectors = Some(sectors);
            Ok(sectors)
        }
    }

    fn print_disc_size(&mut self) {
        if let Ok(total_sectors) = self.get_total_sectors() {
            const SACD_LSN_SIZE: u64 = 2048;
            let total_bytes = total_sectors as u64 * SACD_LSN_SIZE;
            // C code uses 1000^3 for "gigabyte" (not 1024^3)
            let gb = total_bytes as f64 / (1000.0 * 1000.0 * 1000.0);

            println!("\nThe size of sacd is ok (sectors={}). Size is: {} bytes, {:.3} GB (gigabyte)",
                total_sectors, total_bytes, gb);
        }
    }
}

fn read_master_toc(reader: &mut SacdNetReader) -> Result<MasterToc> {
    let res = reader
        .read_data(consts::START_OF_MASTER_TOC, consts::MASTER_TOC_LEN)
        .context("couldn't read master TOC bytes")?;
    Ok(MasterToc::from_bytes(&res).context("couldn't parse master TOC bytes")?)
}

fn read_master_text(reader: &mut SacdNetReader) -> Option<MasterText> {
    // Master text is at sector 511 (START_OF_MASTER_TOC + 1)
    // Read 1 sector (2048 bytes)
    let master_text_sector = consts::START_OF_MASTER_TOC + 1;
    reader
        .read_data(master_text_sector, 1)
        .and_then(|data| MasterText::from_bytes(&data))
        .ok()
}

fn read_stereo_toc(master_toc: &MasterToc, reader: &mut SacdNetReader) -> Option<AreaToc> {
    // Look for stereo TOC 1
    let stereo_toc1 = if master_toc.area_1_toc_1_start > 0 {
        reader
            .read_data(
                master_toc.area_1_toc_1_start,
                master_toc.area_1_toc_size as u32,
            )
            .and_then(|tocdata| AreaToc::from_bytes(&tocdata))
            .ok()
    } else {
        warn!("Couldn't read Stereo TOC 1");
        None
    };

    // see if stereo TOC 2 matches TOC 1
    let stereo_toc2 = if master_toc.area_1_toc_2_start > 0 {
        reader
            .read_data(
                master_toc.area_1_toc_2_start,
                master_toc.area_1_toc_size as u32,
            )
            .and_then(|tocdata| AreaToc::from_bytes(&tocdata))
            .ok()
    } else {
        warn!("Couldn't read Stereo TOC 2");
        None
    };

    match (stereo_toc1, stereo_toc2) {
        (Some(toc1), Some(toc2)) => {
            if toc1 == toc2 {
                // Both exist and are equal - use TOC 1
                Some(toc1)
            } else {
                // By spec, TOC 1 and TOC 2 should be identical/redundant.
                warn!("Stereo TOC 1 and TOC 2 differ, using backup TOC 2");
                Some(toc2)
            }
        }
        (Some(toc1), None) => {
            // Only TOC 1 exists
            Some(toc1)
        }
        (None, Some(toc2)) => {
            // Only TOC 2 exists
            Some(toc2)
        }
        (None, None) => {
            warn!("No stereo TOC found");
            None
        }
    }
}

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
