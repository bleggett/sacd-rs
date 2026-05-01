//! Integration test to verify bit-perfect extraction against C reference.
//!
//! This test extracts track 1 from the test ISO and compares the output
//! byte-for-byte with a known-good reference file extracted by the C tool.

use std::path::PathBuf;
use std::process::Command;

const TEST_ISO: &str = "Bacewicz_ Orchestral Works, Vol. 2 [CHSA 5345].iso";
const REFERENCE_FILE: &str = "reference_track01.dsf";

#[test]
#[ignore] // Requires test ISO and reference file
fn test_bit_perfect_extraction() {
    let test_iso_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(TEST_ISO);
    let reference_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(REFERENCE_FILE);

    // Skip test if files don't exist
    if !test_iso_path.exists() || !reference_path.exists() {
        eprintln!("Skipping test: required files not found");
        eprintln!("ISO: {}", test_iso_path.display());
        eprintln!("Reference: {}", reference_path.display());
        return;
    }

    // Extract with our tool
    let output_dir = tempfile::tempdir().unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_sacd-rs"))
        .args([
            "extract",
            "--iso",
            test_iso_path.to_str().unwrap(),
            output_dir.path().to_str().unwrap(),
            "-t",
            "1",
            "--stereo",
        ])
        .status()
        .expect("Failed to run sacd-rs");

    assert!(status.success(), "Extraction failed");

    // Find the extracted file
    let extracted_files: Vec<_> = std::fs::read_dir(output_dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("dsf"))
        .collect();

    assert_eq!(extracted_files.len(), 1, "Expected exactly one DSF file");
    let extracted_path = extracted_files[0].path();

    // Read both files
    let extracted_data = std::fs::read(&extracted_path).unwrap();
    let reference_data = std::fs::read(&reference_path).unwrap();

    // Compare file sizes
    assert_eq!(
        extracted_data.len(),
        reference_data.len(),
        "File sizes don't match: extracted={} bytes, reference={} bytes",
        extracted_data.len(),
        reference_data.len()
    );

    // Compare byte-by-byte
    for (i, (extracted_byte, reference_byte)) in
        extracted_data.iter().zip(reference_data.iter()).enumerate()
    {
        assert_eq!(
            extracted_byte, reference_byte,
            "Mismatch at byte offset {:#x}: extracted={:#04x}, reference={:#04x}",
            i, extracted_byte, reference_byte
        );
    }

    println!("✓ Bit-perfect match with reference file");
}

#[test]
fn test_dsd_silence_pattern() {
    // Verify our understanding of how to produce 0x99 (DSD silence)
    let predictions = [15i16, -15, 15, -15, 15, -15, 15, -15];
    let needed_residuals = [1u8, 1, 0, 0, 1, 1, 0, 0]; // Pattern for 0x99

    let mut byte = 0u8;
    for (i, (&pred, &res)) in predictions.iter().zip(needed_residuals.iter()).enumerate() {
        let bit_val = (((pred as u16) >> 15) ^ res as u16) & 1;
        byte |= (bit_val << (7 - i)) as u8;
    }

    // Before DSF bit reversal, check the pattern
    println!("Byte before reversal: {:#04x} (binary: {:08b})", byte, byte);

    // After reversal (DSF format), it should be 0x99
    let reversed = byte.reverse_bits();
    println!(
        "Byte after reversal: {:#04x} (binary: {:08b})",
        reversed, reversed
    );

    assert_eq!(reversed, 0x99, "Reversed byte should be DSD silence 0x99");
}
