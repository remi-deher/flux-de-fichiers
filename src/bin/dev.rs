//! Outil de développement local et préparation de l'environnement en Rust pur.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.to_path_buf()
}

pub fn prepare(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let config_dir = root.join("var/config");
    fs::create_dir_all(&config_dir)?;

    let input_dir = root.join("var/input");
    fs::create_dir_all(&input_dir)?;

    let dist_file = root.join("frontend/dist/index.html");
    let needs_build = if !dist_file.exists() {
        true
    } else {
        let dist_mtime = fs::metadata(&dist_file)?
            .modified()
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        let mut out_of_date = false;
        if let Ok(entries) = fs::read_dir(root.join("frontend/src")) {
            for entry in entries.flatten() {
                if let Ok(meta) = entry.metadata()
                    && let Ok(mtime) = meta.modified()
                    && mtime > dist_mtime
                {
                    out_of_date = true;
                    break;
                }
            }
        }
        out_of_date
    };

    if needs_build {
        println!("Génération de l'interface Leptos en Rust...");
        let status = Command::new("cargo")
            .args(["run", "--manifest-path", "frontend/Cargo.toml"])
            .current_dir(root)
            .status();
        match status {
            Ok(s) if s.success() => println!("Interface Leptos générée avec succès."),
            _ => println!(
                "Note: compilation locale du frontend ignorée ou échouée (effectuée dans Docker)."
            ),
        }
    }

    println!("Configuration /config et volumes prêts ; environnement opérationnel.");
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = repo_root();
    prepare(&root)?;

    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--prepare-only") {
        return Ok(());
    }

    println!("Démarrage de Docker Compose sur http://localhost:3000 ...");
    let mut child = Command::new("docker")
        .args([
            "compose",
            "up",
            "--build",
            "--remove-orphans",
            "--scale",
            "worker=3",
        ])
        .current_dir(&root)
        .spawn()?;

    let status = child.wait()?;
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    Ok(())
}
