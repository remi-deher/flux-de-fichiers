"""Delete expired Docker Hub PR tags, never registry manifests by digest."""

import argparse
from datetime import datetime, timedelta, timezone
import json
import os
import re
import subprocess
import sys
from urllib.error import HTTPError
from urllib.parse import quote
from urllib.request import Request, urlopen

HUB_URL = "https://hub.docker.com"
IMAGE_NAME = "flux-de-fichiers"
PR_TAG = re.compile(r"pr-[0-9a-f]{12}")
TEST_TAG = re.compile(r"test-[0-9a-f]{12}")
HTTP_TIMEOUT_SECONDS = 30
DEFAULT_RETENTION_DAYS = 14


class Hub:
    def __init__(self, username, password):
        self.credentials = [password]
        self.token = None
        self.repository = f"{username}/{IMAGE_NAME}"
        self.tags_url = f"{HUB_URL}/v2/repositories/{quote(username, safe='')}/{IMAGE_NAME}/tags/"
        result = self.request("POST", f"{HUB_URL}/v2/users/login/", {
            "username": username, "password": password,
        })
        self.token = result["token"]
        self.credentials.append(self.token)

    def request(self, method, url, data=None):
        # Pagination must never forward the bearer token to another host/path.
        if url != f"{HUB_URL}/v2/users/login/" and not url.startswith(self.tags_url):
            raise RuntimeError("Unexpected Docker Hub API URL")
        headers = {"Content-Type": "application/json"}
        if self.token:
            headers["Authorization"] = f"JWT {self.token}"
        body = json.dumps(data).encode() if data is not None else None
        try:
            with urlopen(Request(url, data=body, headers=headers, method=method),
                         timeout=HTTP_TIMEOUT_SECONDS) as response:
                payload = response.read()
                return json.loads(payload) if payload else None
        except HTTPError as error:
            detail = error.read().decode(errors="replace")
            for credential in self.credentials:
                detail = detail.replace(credential, "[REDACTED]")
            advice = ""
            if error.code in (401, 403):
                advice = " Vérifier les droits ; si la suppression est refusée, demander un jeton dédié dans le secret GitHub DOCKERHUB_TOKEN."
            raise RuntimeError(f"Docker Hub {method}: HTTP {error.code}: {detail}.{advice}") from None

    def tags(self):
        result = []
        url = self.tags_url + "?page_size=100"
        while url:
            page = self.request("GET", url)
            result.extend(page["results"])
            url = page["next"]
        return result

    def tag(self, name):
        return self.request("GET", self.tags_url + quote(name, safe="") + "/")

    def delete(self, name):
        if not PR_TAG.fullmatch(name):
            raise ValueError("Refusing to delete a non-PR tag")
        self.request("DELETE", self.tags_url + name + "/")


def expired(tag, cutoff, open_tags):
    if not PR_TAG.fullmatch(tag["name"]) or tag["name"] in open_tags:
        return False
    updated = datetime.fromisoformat(tag["last_updated"].replace("Z", "+00:00"))
    return updated < cutoff


def digests(tag):
    # Include both the index and per-platform manifests (BuildKit attestations).
    return frozenset(filter(None, [tag.get("digest")] + [
        image.get("digest") for image in tag.get("images", [])
    ]))


def verify_test_image(repository, tag):
    reference = f"{repository}:{tag}"
    subprocess.run(["docker", "pull", "--platform", "linux/amd64", reference], check=True)
    # Exercise the release's promotion operation, without writing a release tag.
    subprocess.run(["docker", "buildx", "imagetools", "create", "--dry-run",
                    "--tag", f"{repository}:cleanup-verification", reference],
                   check=True, stdout=subprocess.DEVNULL)


def purge(hub, cutoff, open_tags, dry_run):
    tags = hub.tags()
    candidates = [tag for tag in tags if expired(tag, cutoff, open_tags)]
    print(f"{len(candidates)} tag(s) candidat(s) ; simulation={dry_run}")
    for candidate in candidates:
        name = candidate["name"]
        # Recheck age immediately before deletion in case the SHA was rebuilt.
        current = hub.tag(name)
        if not expired(current, cutoff, open_tags):
            continue
        protected = [tag for tag in hub.tags() if not PR_TAG.fullmatch(tag["name"])]
        shared = [tag for tag in protected if digests(tag) & digests(current)]
        if dry_run:
            print(f"Simulation : supprimer {name} ; {len(shared)} tag(s) de même digest conservé(s)")
            continue
        hub.delete(name)
        print(f"Supprimé : {name}")
        for tag in protected:
            if digests(hub.tag(tag["name"])) != digests(tag):
                raise RuntimeError(f"Digest modifié pour le tag protégé {tag['name']}")
        # A shared test tag must still download and support promotion.
        test_tag = next((tag for tag in shared if TEST_TAG.fullmatch(tag["name"])), None)
        if test_tag:
            verify_test_image(hub.repository, test_tag["name"])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--open-heads", required=True)
    args = parser.parse_args()
    days = int(os.environ.get("PR_IMAGE_RETENTION_DAYS", DEFAULT_RETENTION_DAYS))
    if days < 1:
        raise ValueError("PR_IMAGE_RETENTION_DAYS must be a positive integer")
    dry_run = os.environ.get("DRY_RUN", "true")
    if dry_run not in ("true", "false"):
        raise ValueError("DRY_RUN must be true or false")
    with open(args.open_heads, encoding="utf-8") as source:
        heads = source.read().splitlines()
    if any(not re.fullmatch(r"[0-9a-f]{40}", head) for head in heads):
        raise ValueError("Invalid open PR SHA")
    open_tags = {f"pr-{head[:12]}" for head in heads}
    cutoff = datetime.now(timezone.utc) - timedelta(days=days)
    hub = Hub(os.environ["DOCKERHUB_USERNAME"], os.environ["DOCKERHUB_TOKEN"])
    purge(hub, cutoff, open_tags, dry_run == "true")


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, ValueError, KeyError, OSError, subprocess.CalledProcessError) as error:
        print(f"Échec de la purge : {error}", file=sys.stderr)
        sys.exit(1)
