"""Scénarios réels, bornés, sur la stack lancée par npm run dev.
N'utilise ni les NAS ni des médias utilisateur. Les trois agents sont des conteneurs
sur la même machine : ce test ne revendique pas une validation multi-hôte.
"""
from concurrent.futures import ThreadPoolExecutor
from http.cookiejar import CookieJar
import hashlib
import json
from pathlib import Path
import sqlite3
import subprocess
import time
import urllib.error
import urllib.request
import uuid
import xml.etree.ElementTree as ET

from dev import ROOT

PREFIX = f"scenario-{uuid.uuid4().hex[:10]}"
BASE_URL = "http://localhost:3000"
WEB_ORIGIN = "http://localhost:3000"


def get_key(name):
    cfg = ROOT / f"var/config/{name}"
    if cfg.exists():
        return cfg.read_text().strip()
    sec = ROOT / f"var/secrets/{name}"
    if sec.exists():
        return sec.read_text().strip()
    return ""


ADMIN = get_key("admin_key")
WORKER = get_key("worker_key")
RESULTS = []


def compose(*args, capture=True, **kwargs):
    result = subprocess.run(["docker", "compose", *args], cwd=ROOT,
                            capture_output=capture, **kwargs)
    if result.returncode and capture:
        print((result.stdout + result.stderr).decode(errors="replace")[-4000:], flush=True)
    result.check_returncode()
    return result.stdout


def request(method, path, payload=None, key=None, expected=200, headers=None, raw=False):
    resolved_key = key if key is not None else (get_key("admin_key") or ADMIN)
    body = payload if isinstance(payload, bytes) else json.dumps(payload).encode() if payload is not None else None
    h = {"Authorization": f"Bearer {resolved_key}", "Content-Type": "application/json"}
    h.update(headers or {})
    req = urllib.request.Request(BASE_URL + path, data=body, method=method, headers=h)
    try:
        with urllib.request.urlopen(req, timeout=90) as response:
            status, data = response.status, response.read()
    except urllib.error.HTTPError as error:
        status, data = error.code, error.read()
    assert status == expected, f"{method} {path}: {status} != {expected}: {data[:200]!r}"
    return data if raw else json.loads(data) if data else None


def web_opener():
    cookies = CookieJar()
    opener = urllib.request.build_opener(
        urllib.request.HTTPCookieProcessor(cookies),
    )
    return opener, cookies


def web_request(opener, method, path, payload=None, expected=200, headers=None):
    body = json.dumps(payload).encode() if payload is not None else None
    request_headers = {"Origin": WEB_ORIGIN}
    if payload is not None:
        request_headers["Content-Type"] = "application/json"
    request_headers.update(headers or {})
    req = urllib.request.Request(
        BASE_URL + path,
        data=body,
        method=method,
        headers=request_headers,
    )
    try:
        with opener.open(req, timeout=90) as response:
            status, data, response_headers = response.status, response.read(), response.headers
    except urllib.error.HTTPError as error:
        status, data, response_headers = error.code, error.read(), error.headers
    assert status == expected, f"{method} {path}: {status} != {expected}: {data[:200]!r}"
    return data, response_headers


def wait_for(predicate, timeout=90):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        value = predicate()
        if value:
            return value
        time.sleep(0.25)
    raise AssertionError("Délai du scénario dépassé")


def passed(name):
    RESULTS.append(name)
    print(f"OK : {name}", flush=True)


def fixture(*args, entrypoint="ffmpeg"):
    if entrypoint in ("mkvmerge", "mkvextract"):
        args = ("--output-charset", "UTF-8", *args)
    return compose("--profile", "test", "run", "--rm", "--no-deps", "--entrypoint", entrypoint, "fixtures", *args)


def sql(statement):
    db_path = ROOT / "var/config/flux.db"
    conn = sqlite3.connect(str(db_path), timeout=30)
    try:
        cursor = conn.cursor()
        cursor.execute(statement)
        rows = cursor.fetchall()
        conn.commit()
        if rows and len(rows) > 0 and len(rows[0]) > 0:
            return str(rows[0][0])
        return ""
    finally:
        conn.close()


def submit(name, target=None):
    return request("POST", "/api/v1/jobs", {"source_name": name, "target_worker_id": target}, expected=201)


def terminal(job):
    current = request("GET", f"/api/v1/jobs/{job['id']}")
    return current if current["status"] in ("succeeded", "failed") else None


def lease_headers(claim, worker):
    return {"x-worker-id": worker, "x-lease-token": claim["lease_token"]}


def packet_signature(name):
    # Hash concatenated encoded payloads per stream, with stream copy: a remux may
    # change packet boundaries while retaining exactly the same elementary bytes.
    return fixture("-v", "error", "-i", f"/fixtures/{name}", "-map", "0:v", "-map", "0:a", "-map", "0:s", "-c", "copy", "-f", "streamhash", "-hash", "sha256", "-").decode().splitlines()


def create_fixture():
    directory = ROOT / "var/input"
    source = f"{PREFIX}.mkv"
    (directory / f"{PREFIX}.srt").write_text("1\n00:00:00,000 --> 00:00:01,500\nSous-titre témoin\n")
    (directory / f"{PREFIX}.txt").write_text("Pièce jointe de contrôle\n")
    (directory / f"{PREFIX}.meta").write_text(";FFMETADATA1\ntitle=Scénario remux\n[CHAPTER]\nTIMEBASE=1/1000\nSTART=0\nEND=2000\ntitle=Chapitre témoin\n")
    fixture("-v", "error", "-y", "-f", "lavfi", "-i", "testsrc=size=160x90:rate=10", "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000",
            "-i", f"/fixtures/{PREFIX}.srt", "-f", "ffmetadata", "-i", f"/fixtures/{PREFIX}.meta",
            "-map", "0:v", "-map", "1:a", "-map", "2:s", "-map_metadata", "3", "-map_chapters", "3",
            "-c:v", "ffv1", "-c:a", "pcm_s16le", "-c:s", "srt", "-t", "2", "-metadata:s:a:0", "language=fra",
            "-metadata:s:s:0", "language=eng", "-disposition:s:0", "forced", "-attach", f"/fixtures/{PREFIX}.txt",
            "-metadata:s:t:0", "mimetype=text/plain", f"/fixtures/{source}")
    return directory, source


def run():
    wait_for(lambda: request("GET", "/health", expected=204) is None)
    admin_key = get_key("admin_key") or ADMIN
    worker_key = get_key("worker_key") or WORKER
    containers = compose("ps", "-q", "worker").decode().split()
    assert len(containers) == 3, "Lancer npm run dev pour disposer de trois agents"
    # Register list may include workers from previous runs. Use container hostnames.
    names = subprocess.check_output(["docker", "inspect", "--format", "{{.Config.Hostname}}", *containers], text=True).split()
    def current_workers():
        rows = [w for w in request("GET", "/api/v1/workers", key=admin_key) if w["name"] in names]
        return rows if len(rows) == 3 else None
    workers = wait_for(current_workers)
    print("Construction du conteneur d'outils de test…", flush=True)
    compose("--profile", "test", "build", "fixtures")
    directory, source = create_fixture()
    original_hash = hashlib.sha256((directory / source).read_bytes()).hexdigest()
    request("GET", "/api/v1/jobs", key="wrong", expected=401)
    request("POST", "/api/v1/jobs", {"source_name": source}, key=worker_key, expected=401)
    request("POST", "/api/v1/claims", {"worker_id": workers[0]["id"]}, key=admin_key, expected=401)
    request("POST", "/api/v1/jobs", {"source_name": "../secret.mkv"}, key=admin_key, expected=400)
    passed("Authentification, séparation des rôles et rejet de traversée de chemin")

    # Vérification SQLite WAL et endpoint d'administration /api/v1/config
    journal_mode = sql("PRAGMA journal_mode;")
    assert journal_mode.lower() == "wal", f"Journal mode attendu: wal, obtenu: {journal_mode}"
    sys_cfg = request("GET", "/api/v1/config", key=admin_key)
    assert sys_cfg["worker_key"] == worker_key, "Clé worker incohérente dans /api/v1/config"
    assert "version" in sys_cfg and "max_file_bytes" in sys_cfg, "Champs obligatoires manquants dans /api/v1/config"
    passed("Architecture *arr : port unique HTTP 3000, SQLite WAL et endpoint /api/v1/config")

    web, cookies = web_opener()
    login_body, login_headers = web_request(
        web,
        "POST",
        "/api/v1/auth/session",
        {"admin_key": admin_key},
    )
    assert login_headers.get("Cache-Control") == "no-store"
    assert {cookie.name for cookie in cookies} >= {"flux_session", "flux_csrf"}
    session = json.loads(login_body)
    csrf = session["csrf_token"]
    restored_body, _ = web_request(web, "GET", "/api/v1/auth/session")
    assert json.loads(restored_body)["csrf_token"] == csrf
    web_request(
        web,
        "POST",
        "/api/v1/jobs",
        {"source_name": source, "target_worker_id": None},
        expected=403,
        headers={"X-CSRF-Token": "invalid-token"},
    )
    session_job_body, _ = web_request(
        web,
        "POST",
        "/api/v1/jobs",
        {"source_name": source, "target_worker_id": None},
        expected=201,
        headers={"X-CSRF-Token": csrf},
    )
    session_job = json.loads(session_job_body)
    completed_session_job = wait_for(lambda: terminal(session_job))
    assert completed_session_job["status"] == "succeeded", completed_session_job
    download_request = urllib.request.Request(
        f"{BASE_URL}/api/v1/jobs/{session_job['id']}/output",
        headers={"Origin": WEB_ORIGIN},
    )
    streamed_hash = hashlib.sha256()
    streamed_size = 0
    with web.open(download_request, timeout=90) as response:
        assert response.status == 200
        assert response.headers.get("Cache-Control") == "no-store"
        assert response.headers.get("Content-Disposition", "").startswith("attachment;")
        while True:
            chunk = response.read(64 * 1024)
            if not chunk:
                break
            streamed_hash.update(chunk)
            streamed_size += len(chunk)
        assert streamed_size == int(response.headers["Content-Length"])
    assert streamed_hash.hexdigest() == completed_session_job["output_sha256"]
    web_request(
        web,
        "POST",
        "/api/v1/auth/logout",
        headers={"X-CSRF-Token": csrf},
        expected=204,
    )
    web_request(web, "GET", "/api/v1/files", expected=401)
    passed("Session web : origine/CSRF, révocation et téléchargement authentifié streamé sans Blob")

    # One active worker on the same host as coordinator; pause the other two.
    paused = containers[1:]
    subprocess.run(["docker", "pause", *paused], check=True, stdout=subprocess.DEVNULL)
    try:
        job = submit(source)
        one = wait_for(lambda: terminal(job))
        assert one["status"] == "succeeded" and one["attempts"] == 1, one
        data = request("GET", f"/api/v1/jobs/{one['id']}/output", raw=True)
        assert hashlib.sha256(data).hexdigest() == one["output_sha256"]
        output = f"{PREFIX}-output.mkv"
        (directory / output).write_bytes(data)
        assert packet_signature(source) == packet_signature(output), "Paquets audio/vidéo/sous-titres modifiés"
        before = json.loads(fixture("-J", f"/fixtures/{source}", entrypoint="mkvmerge"))
        after = json.loads(fixture("-J", f"/fixtures/{output}", entrypoint="mkvmerge"))
        for field in ("chapters", "global_tags", "track_tags"):
            assert before[field] == after[field], field
        def chapters(name):
            root = ET.fromstring(fixture(f"/fixtures/{name}", "chapters", entrypoint="mkvextract"))
            def canonical(element):
                legacy = element.findtext("ChapterLanguage")
                return (element.tag, (element.text or "").strip(), tuple(
                    canonical(child) for child in element
                    if child.tag not in ("ChapterUID", "EditionUID")
                    and not (child.tag == "ChapLanguageIETF" and child.text == legacy)))
            return canonical(root)
        assert chapters(source) == chapters(output)
        assert fixture(f"/fixtures/{source}", "tags", entrypoint="mkvextract") == fixture(f"/fixtures/{output}", "tags", entrypoint="mkvextract")
        for name in (source, output):
            fixture(f"/fixtures/{name}", "attachments", f"1:/fixtures/{name}.attachment", entrypoint="mkvextract")
        assert (directory / f"{source}.attachment").read_bytes() == (directory / f"{output}.attachment").read_bytes()
        assert hashlib.sha256((directory / source).read_bytes()).hexdigest() == original_hash
        passed("Un agent : remux réel, paquets identiques, chapitres/tags/pièce jointe et source conservés")
    finally:
        subprocess.run(["docker", "unpause", *paused], check=True, stdout=subprocess.DEVNULL)

    # A real targeted job per worker, submitted concurrently.
    with ThreadPoolExecutor(max_workers=3) as pool:
        jobs = list(pool.map(lambda w: submit(source, w["id"]), workers))
    completed = [wait_for(lambda job=job: terminal(job)) for job in jobs]
    assert all(j["status"] == "succeeded" and j["attempts"] == 1 for j in completed), completed
    assert {j["worker_id"] for j in completed} == {w["id"] for w in workers}
    # Untargeted work under contention must not be claimed twice.
    with ThreadPoolExecutor(max_workers=6) as pool:
        jobs = list(pool.map(lambda _: submit(source), range(9)))
    completed = [wait_for(lambda job=job: terminal(job)) for job in jobs]
    assert all(j["status"] == "succeeded" and j["attempts"] == 1 for j in completed)
    passed("Trois agents : affectation ciblée et neuf tâches concurrentes sans double attribution")

    # Hold a lease as an agent that disappears. Force only this test lease's clock in SQL.
    ghost = request("POST", "/api/v1/workers", {"name": PREFIX + "-interrupted"}, key=worker_key)
    interrupted = submit(source, ghost["id"])
    old = request("POST", "/api/v1/claims", {"worker_id": ghost["id"]}, key=worker_key)
    assert old["job_id"] == interrupted["id"]
    request("POST", f"/api/v1/jobs/{interrupted['id']}/heartbeat", {"worker_id": ghost["id"], "lease_token": old["lease_token"]}, key=worker_key, expected=204)
    remaining_secs = float(sql(f"SELECT (strftime('%s', leased_until) - strftime('%s', 'now')) FROM jobs WHERE id='{interrupted['id']}'"))
    assert remaining_secs > 100, f"Bail restant insuffisant: {remaining_secs}"
    sql(f"UPDATE jobs SET leased_until=datetime('now', '-1 second'), target_worker_id=NULL WHERE id='{interrupted['id']}'")
    request("POST", f"/api/v1/jobs/{interrupted['id']}/heartbeat", {"worker_id": ghost["id"], "lease_token": old["lease_token"]}, key=worker_key, expected=409)
    request("PUT", f"/api/v1/jobs/{interrupted['id']}/result", data, key=worker_key, headers=lease_headers(old, ghost["id"]), expected=409)
    recovered = wait_for(lambda: terminal(interrupted))
    assert recovered["status"] == "succeeded" and recovered["attempts"] == 2
    passed("Agent disparu : reprise après expiration, ancien bail refusé en heartbeat et publication")

    # Max attempts, invalid source, mutation after submission, oversized/invalid result.
    corrupt = f"{PREFIX}-corrupt.mkv"
    (directory / corrupt).write_bytes(b"not matroska")
    bad_job = submit(corrupt)
    bad = wait_for(lambda: terminal(bad_job))
    assert bad["status"] == "failed" and bad["error_code"] == "remux_failed", bad
    reserved = request("POST", "/api/v1/workers", {"name": PREFIX + "-reserved"}, key=worker_key)
    changed = f"{PREFIX}-changed.mkv"
    (directory / changed).write_bytes((directory / source).read_bytes())
    mutation = submit(changed, reserved["id"])
    (directory / changed).write_bytes(b"changed")
    sql(f"UPDATE jobs SET target_worker_id=NULL WHERE id='{mutation['id']}'")
    failed = wait_for(lambda: terminal(mutation))
    assert failed["status"] == "failed" and failed["error_code"] == "source_integrity"
    invalid = submit(source, reserved["id"])
    claim = request("POST", "/api/v1/claims", {"worker_id": reserved["id"]}, key=worker_key)
    request("PUT", f"/api/v1/jobs/{invalid['id']}/result", b"invalid", key=worker_key, headers=lease_headers(claim, reserved["id"]), expected=400)
    request("GET", f"/api/v1/jobs/{invalid['id']}/output", expected=404)
    sql(f"UPDATE jobs SET attempts=3,leased_until=datetime('now', '-1 second') WHERE id='{invalid['id']}'")
    exhausted = wait_for(lambda: terminal(invalid))
    assert exhausted["status"] == "failed" and exhausted["error_code"] == "lease_expired"
    passed("Échecs contrôlés : source invalide/modifiée, résultat invalide non publié, limite de reprises")

    # Coordinator restart: migration state, SQLite database and completed outputs survive it.
    compose("restart", "coordinator")
    wait_for(lambda: request("GET", "/health", expected=204) is None)
    def restored():
        try:
            return request("GET", f"/api/v1/jobs/{one['id']}")["status"] == "succeeded"
        except (AssertionError, OSError):
            return False
    wait_for(restored)
    assert request("GET", f"/api/v1/jobs/{one['id']}/output", raw=True) == data
    passed("Redémarrage coordinateur : tâches et résultat persistants dans SQLite, intégrité préservée")
    print(f"{len(RESULTS)} groupes de scénarios réussis. Rapport : var/scenario-report.json", flush=True)


def main():
    started = time.monotonic()
    status = "failed"
    try:
        run()
        status = "passed"
    except Exception:
        print("ÉCHEC : consulter les journaux Docker et les journaux JSON locaux.", flush=True)
        raise
    finally:
        (ROOT / "var/scenario-report.json").write_text(json.dumps({
            "prefix": PREFIX,
            "status": status,
            "duration_seconds": round(time.monotonic() - started, 3),
            "passed": RESULTS,
        }, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
