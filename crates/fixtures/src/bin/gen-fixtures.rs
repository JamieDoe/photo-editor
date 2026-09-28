//! Regenerates the committed synthetic fixtures, and optionally large benchmark files.
//!
//! Usage: cargo run -p fixtures --bin gen-fixtures -- [--large]

use std::path::PathBuf;

fn main() -> std::io::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    let synthetic = root.join("synthetic");
    std::fs::create_dir_all(&synthetic)?;

    write(
        &synthetic.join("chart.dng"),
        &fixtures::chart_dng(1200, 800),
    )?;
    write(
        &synthetic.join("chart.jpg"),
        &fixtures::chart_jpeg(1200, 800, 92),
    )?;

    if std::env::args().any(|a| a == "--large") {
        // ~24 MP; large, so written to the git-ignored local directory.
        let local = root.join("local");
        std::fs::create_dir_all(&local)?;
        write(
            &local.join("synthetic-24mp.dng"),
            &fixtures::chart_dng(6000, 4000),
        )?;
        write(
            &local.join("synthetic-24mp.jpg"),
            &fixtures::chart_jpeg(6000, 4000, 92),
        )?;
    }
    Ok(())
}

fn write(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::write(path, bytes)?;
    println!("wrote {} ({} bytes)", path.display(), bytes.len());
    Ok(())
}
