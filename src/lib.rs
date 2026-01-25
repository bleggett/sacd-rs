use std::net::{IpAddr, SocketAddr, TcpStream};

mod sacd_net_reader;
mod scarletbook;

pub mod sacd_ripper {
    include!(concat!(env!("OUT_DIR"), "/libsacd.sacd_ripper.rs"));
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use super::*;

    fn init() {
        let _ = env_logger::builder().is_test(true).try_init();
    }

    #[test]
    fn test_open_network() {
        init();
        let handle =
            sacd_net_reader::open_network_reader(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 130)), 2002)
                .expect("should init");
    }

    #[test]
    fn test_read() {
        init();
        let mut handle =
            sacd_net_reader::open_network_reader(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 130)), 2002)
                .expect("should init");
        let res = handle.read_data(510, 10).expect("should read");
        println!("{:?}", res);
        assert_eq!(res.len(), 20480);
    }

    #[test]
    fn test_read_master_toc() {
        init();
        let handle =
            sacd_net_reader::open_network_reader(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 130)), 2002)
                .expect("should init");
        let sbreader = scarletbook::reader::new(handle).expect("should create sbreader");
        let master_toc = sbreader.get_master_toc();
        println!("DISC CATALOG: {}", master_toc.disc_catalog())
    }
}
