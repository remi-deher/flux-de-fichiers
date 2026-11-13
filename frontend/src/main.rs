//! Binaire de génération des pages statiques : interface historique et page du client WASM.

use std::{fs, path::Path};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("dist");
    let app_dir = out_dir.join(flux_frontend::APP_DIST_DIR);
    fs::create_dir_all(&app_dir)?;
    fs::write(
        out_dir.join("index.html"),
        flux_frontend::render_full_page(),
    )?;
    fs::write(app_dir.join("index.html"), flux_frontend::render_app_page())?;
    println!("Interface générée avec succès : {}", out_dir.display());
    Ok(())
}
