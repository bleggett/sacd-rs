mod sacd_net_reader;

pub mod sacd_ripper {
    include!(concat!(env!("OUT_DIR"), "/libsacd.sacd_ripper.rs"));
}
