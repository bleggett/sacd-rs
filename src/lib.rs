use std::net::IpAddr;

mod sacd_net_reader;
mod scarletbook;
use std::path::Path;

pub mod sacd_ripper {
    include!(concat!(env!("OUT_DIR"), "/libsacd.sacd_ripper.rs"));
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;
    use indicatif::{ProgressBar, ProgressStyle};

    use super::*;

    fn init() {
        let _ = env_logger::builder().is_test(true).try_init();
    }

    // #[test]
    // fn test_open_network() {
    //     init();
    //     let handle =
    //         sacd_net_reader::open_network_reader(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 130)), 2002)
    //             .expect("should init");
    // }

    // #[test]
    // fn test_read() {
    //     init();
    //     let mut handle =
    //         sacd_net_reader::open_network_reader(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 130)), 2002)
    //             .expect("should init");
    //     let res = handle.read_data(510, 10).expect("should read");
    //     println!("{:?}", res);
    //     assert_eq!(res.len(), 20480);
    // }

    // #[test]
    // fn test_read_master_toc() {
    //     init();
    //     let handle =
    //         sacd_net_reader::open_network_reader(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 130)), 2002)
    //             .expect("should init");
    //     let sbreader = scarletbook::reader::new(handle).expect("should create sbreader");
    //     let master_toc = sbreader.get_master_toc();
    //     println!("DISC CATALOG: {}", master_toc.disc_catalog())
    // }

    // #[test]
    // fn test_read_stereo_area_toc() {
    //     init();
    //     let handle =
    //         sacd_net_reader::open_network_reader(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 130)), 2002)
    //             .expect("should init");
    //     let sbreader = scarletbook::reader::new(handle).expect("should create sbreader");
    //     let stereo_toc = sbreader.get_stereo_toc().expect("stereo toc not present");
    //     println!("stereo toc: {:#?}", stereo_toc)
    // }

    #[test]
    fn test_dump_iso() {
        init();
        let mut handle =
            sacd_net_reader::open_network_reader(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 130)), 2002)
                .expect("should init");

        let pb = ProgressBar::new(0);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} sectors ({percent}%)")
                .unwrap()
                .progress_chars("#>-")
        );

        handle
            .dump_iso(
                Path::new("/var/home/bleggett/TEST.iso"),
                scarletbook::consts::SACD_LSN_SIZE,
                Some(|current, total| {
                    if pb.length().unwrap_or(0) == 0 {
                        pb.set_length(total as u64);
                    }
                    pb.set_position(current as u64);
                }),
            )
            .expect("write success");
        pb.finish_with_message("Complete!");
    }
}
