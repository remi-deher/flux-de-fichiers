"""Prépare les secrets locaux puis exécute Compose au premier plan."""
import os
from pathlib import Path
import secrets
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def prepare():
    os.chdir(ROOT)
    directory = ROOT / "var/secrets"
    directory.mkdir(parents=True, exist_ok=True, mode=0o700)
    directory.chmod(0o700)
    (ROOT / "var/input").mkdir(parents=True, exist_ok=True)
    for name in ("db_password", "admin_key", "worker_key"):
        path = directory / name
        if not path.exists():
            path.write_text(secrets.token_hex(32))
        # Compose bind-mount secrets must be readable by the non-root container.
        # Their host directory is 0700; keys never leave this ignored directory.
        path.chmod(0o644)
    def openssl(*args):
        subprocess.run(["openssl", *map(str, args)], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    if not (directory / "ca.crt").exists():
        openssl("req", "-x509", "-newkey", "rsa:3072", "-nodes", "-days", "365", "-subj", "/CN=Flux local development CA",
                "-addext", "basicConstraints=critical,CA:TRUE", "-addext", "subjectKeyIdentifier=hash", "-addext", "keyUsage=critical,keyCertSign,cRLSign", "-keyout", directory / "ca.key", "-out", directory / "ca.crt")
    for name, names in (("server", "DNS:coordinator,DNS:localhost,IP:127.0.0.1"), ("db", "DNS:db")):
        if not (directory / f"{name}.crt").exists():
            ext = directory / f"{name}.ext"
            ext.write_text(f"subjectAltName={names}\nextendedKeyUsage=serverAuth\nbasicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nsubjectKeyIdentifier=hash\nauthorityKeyIdentifier=keyid,issuer\n")
            openssl("req", "-newkey", "rsa:3072", "-nodes", "-subj", f"/CN={name}",
                    "-keyout", directory / f"{name}.key", "-out", directory / f"{name}.csr")
            openssl("x509", "-req", "-days", "365", "-in", directory / f"{name}.csr", "-CA", directory / "ca.crt",
                    "-CAkey", directory / "ca.key", "-CAcreateserial", "-extfile", ext, "-out", directory / f"{name}.crt")
        (directory / f"{name}.key").chmod(0o644)
    password = (directory / "db_password").read_text().strip()
    path = directory / "database_url"
    path.write_text(f"postgresql://flux:{password}@db/flux?sslmode=verify-full&sslrootcert=/run/secrets/ca_cert")
    path.chmod(0o644)
    dist_file = ROOT / "frontend/dist/index.html"
    frontend_sources = list((ROOT / "frontend/src").glob("**/*.rs")) + [ROOT / "frontend/Cargo.toml"]
    needs_build = not dist_file.exists() or any(
        src.exists() and src.stat().st_mtime > dist_file.stat().st_mtime for src in frontend_sources
    )
    if needs_build:
        (ROOT / "frontend/dist").mkdir(parents=True, exist_ok=True)
        print("Génération de l'interface Leptos...", flush=True)
        subprocess.run(["cargo", "run", "--manifest-path", str(ROOT / "frontend/Cargo.toml")], check=True)
    print("Secrets locaux et frontend prêts ; démarrage de Docker Compose sur http://localhost:3000", flush=True)


if __name__ == "__main__":
    prepare()
    os.execvp("docker", ["docker", "compose", "up", "--build", "--remove-orphans", "--scale", "worker=3"])
