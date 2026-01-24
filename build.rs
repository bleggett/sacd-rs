use std::io::Result;
fn main() -> Result<()> {
    prost_build::compile_protos(&["src/protos/sacd_ripper.proto"], &["src/"])?;
    Ok(())
}
