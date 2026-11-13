//! Binaire de génération de la page du client web (`frontend/dist/index.html`).

use std::{fs, path::Path};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("dist");
    fs::create_dir_all(&out_dir)?;
    fs::write(out_dir.join("index.html"), flux_frontend::render_page())?;
    println!("Page générée : {}", out_dir.join("index.html").display());
    Ok(())
}
