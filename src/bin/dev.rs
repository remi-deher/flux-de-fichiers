//! Outil de développement local et préparation de l'environnement en Rust pur.

use std::fmt::Write as _;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

fn generate_token_hex() -> String {
    let mut bytes = [0u8; 32];
    if let Ok(mut f) = fs::File::open("/dev/urandom")
        && f.read_exact(&mut bytes).is_ok()
    {
        let mut s = String::with_capacity(64);
        for b in bytes {
            let _ = write!(s, "{b:02x}");
        }
        return s;
    }
    let u1 = uuid::Uuid::new_v4().simple();
    let u2 = uuid::Uuid::new_v4().simple();
    format!("{u1}{u2}")
}

fn repo_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.to_path_buf()
}

pub fn prepare(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let config_dir = root.join("var/config");
    fs::create_dir_all(&config_dir)?;

    let secrets_dir = root.join("var/secrets");
    fs::create_dir_all(&secrets_dir)?;

    let input_dir = root.join("var/input");
    fs::create_dir_all(&input_dir)?;

    for name in ["admin_key", "worker_key"] {
        let cfg_path = config_dir.join(name);
        let sec_path = secrets_dir.join(name);

        let val = if cfg_path.exists() {
            fs::read_to_string(&cfg_path)?.trim().to_string()
        } else {
            let generated = generate_token_hex();
            fs::write(&cfg_path, &generated)?;
            generated
        };

        if !sec_path.exists() {
            fs::write(&sec_path, &val)?;
        }
    }

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
