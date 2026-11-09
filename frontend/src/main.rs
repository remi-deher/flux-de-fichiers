//! Binaire de génération de la page statique de prévisualisation.

use std::{fs, path::Path};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("dist");
    fs::create_dir_all(&out_dir)?;
    let html = flux_frontend::render_full_page();
    let out_file = out_dir.join("index.html");
    fs::write(&out_file, html)?;
    println!("Interface générée avec succès : {}", out_file.display());
    Ok(())
}
