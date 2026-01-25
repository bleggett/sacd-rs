use crate::sacd_net_reader::SacdNetReader;
use anyhow::{Context, Result};
use log::{debug, info, warn};
use std::fs::File;
use std::io::Write;
use std::path::Path;

use crate::scarletbook::consts;
use crate::scarletbook::types::{AreaToc, MasterToc};

pub struct ScarletBookReader {
    reader: SacdNetReader,
    master_toc: MasterToc,
    stereo_toc: Option<AreaToc>,
    stereo_area_index: i32,
    mch_area_index: i32,
}

pub fn new(mut reader: SacdNetReader) -> Result<ScarletBookReader> {
    let master_toc = read_master_toc(&mut reader)?;

    let stereo_toc = read_stereo_toc(&master_toc, &mut reader);

    let sbreader = ScarletBookReader {
        reader,
        master_toc,
        stereo_toc,
        stereo_area_index: -1,
        mch_area_index: -1,
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
}

fn read_master_toc(reader: &mut SacdNetReader) -> Result<MasterToc> {
    let res = reader
        .read_data(consts::START_OF_MASTER_TOC, consts::MASTER_TOC_LEN)
        .context("couldn't read master TOC bytes")?;
    Ok(MasterToc::from_bytes(&res).context("couldn't parse master TOC bytes")?)
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
