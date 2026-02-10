use std::io::Result;
fn main() -> Result<()> {
    // Set build date
    let now = chrono::Utc::now();
    println!("cargo:rustc-env=BUILD_DATE={}", now.format("%Y-%m-%d"));

    prost_build::compile_protos(&["src/protos/sacd_ripper.proto"], &["src/"])?;
    Ok(())
}
