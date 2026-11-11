"""Prépare le répertoire /config local puis exécute Compose au premier plan."""
import os
from pathlib import Path
import secrets
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def prepare():
    os.chdir(ROOT)
    config_dir = ROOT / "var/config"
    config_dir.mkdir(parents=True, exist_ok=True)
    try:
        config_dir.chmod(0o777)
    except OSError:
        pass

    secrets_dir = ROOT / "var/secrets"
    secrets_dir.mkdir(parents=True, exist_ok=True)

    (ROOT / "var/input").mkdir(parents=True, exist_ok=True)

    for name in ("admin_key", "worker_key"):
        cfg_path = config_dir / name
        sec_path = secrets_dir / name
        if not cfg_path.exists():
            val = secrets.token_hex(32)
            cfg_path.write_text(val)
        else:
            val = cfg_path.read_text().strip()
        try:
            cfg_path.chmod(0o644)
        except OSError:
            pass
        if not sec_path.exists():
            sec_path.write_text(val)

    dist_file = ROOT / "frontend/dist/index.html"
    frontend_sources = list((ROOT / "frontend/src").glob("**/*.rs")) + [ROOT / "frontend/Cargo.toml"]
    needs_build = not dist_file.exists() or any(
        src.exists() and src.stat().st_mtime > dist_file.stat().st_mtime for src in frontend_sources
    )
    if needs_build:
        (ROOT / "frontend/dist").mkdir(parents=True, exist_ok=True)
        try:
            print("Génération de l'interface Leptos...", flush=True)
            subprocess.run(["cargo", "run", "--manifest-path", str(ROOT / "frontend/Cargo.toml")], check=True)
        except (FileNotFoundError, subprocess.CalledProcessError):
            print("Cargo absent ou échec local ; la génération du frontend s'effectuera dans le conteneur Docker.", flush=True)

    print("Configuration /config et volumes prêts ; démarrage de Docker Compose sur http://localhost:3000", flush=True)


if __name__ == "__main__":
    prepare()
    os.execvp("docker", ["docker", "compose", "up", "--build", "--remove-orphans", "--scale", "worker=3"])
