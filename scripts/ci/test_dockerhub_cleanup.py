"""Safety tests: no Docker Hub credentials or network required."""

from datetime import datetime, timezone
import io
import unittest
from unittest.mock import patch
from urllib.error import HTTPError

from dockerhub_cleanup import Hub, expired, purge

CUTOFF = datetime(2026, 10, 25, tzinfo=timezone.utc)
PR_NAME = "pr-0123456789ab"
TEST_NAME = "test-abcdef012345"


def tag(name, updated="2026-10-01T00:00:00Z", digest="sha256:shared"):
    return {"name": name, "last_updated": updated, "digest": digest,
            "images": [{"digest": "sha256:amd64"}]}


class FakeHub:
    repository = "example/flux-de-fichiers"

    def __init__(self, tags):
        self.data = {item["name"]: item for item in tags}
        self.deleted = []

    def tags(self):
        return list(self.data.values())

    def tag(self, name):
        return self.data[name]

    def delete(self, name):
        self.deleted.append(name)
        del self.data[name]


class CleanupTests(unittest.TestCase):
    def test_exact_allowlist_and_age(self):
        for name in ("test", TEST_NAME, "1.2.3", "1.2", "latest", "pr-other",
                     "pr-0123456789abcdef", "pr-0123456789ab/extra"):
            with self.subTest(name=name):
                self.assertFalse(expired(tag(name), CUTOFF, set()))
        self.assertTrue(expired(tag(PR_NAME), CUTOFF, set()))
        self.assertFalse(expired(tag(PR_NAME, "2026-10-25T00:00:00Z"), CUTOFF, set()))
        self.assertFalse(expired(tag(PR_NAME, "2026-11-01T00:00:00Z"), CUTOFF, set()))
        self.assertFalse(expired(tag(PR_NAME), CUTOFF, {PR_NAME}))

    def test_dry_run_does_not_delete(self):
        hub = FakeHub([tag(PR_NAME)])
        purge(hub, CUTOFF, set(), True)
        self.assertEqual(hub.deleted, [])

    @patch("dockerhub_cleanup.verify_test_image")
    def test_shared_digest_survives_and_is_verified(self, verify):
        protected = [tag(name) for name in ("test", TEST_NAME, "1.2.3", "1.2", "latest")]
        hub = FakeHub([tag(PR_NAME), *protected])
        purge(hub, CUTOFF, set(), False)
        self.assertEqual(hub.deleted, [PR_NAME])
        self.assertEqual(hub.tags(), protected)
        verify.assert_called_once_with(hub.repository, TEST_NAME)

    def test_rebuilt_candidate_is_preserved(self):
        hub = FakeHub([tag(PR_NAME)])
        with patch.object(hub, "tag", return_value=tag(PR_NAME, "2026-11-01T00:00:00Z")):
            purge(hub, CUTOFF, set(), False)
        self.assertEqual(hub.deleted, [])

    def test_delete_rejects_protected_tag_before_api_call(self):
        hub = object.__new__(Hub)
        with patch.object(hub, "request") as request:
            for name in ("test", TEST_NAME, "1.2.3", "1.2", "latest"):
                with self.assertRaises(ValueError):
                    hub.delete(name)
            request.assert_not_called()

    def test_delete_uses_tag_endpoint_not_manifest_digest(self):
        hub = object.__new__(Hub)
        hub.tags_url = "https://hub.docker.com/v2/repositories/example/flux-de-fichiers/tags/"
        with patch.object(hub, "request") as request:
            hub.delete(PR_NAME)
        request.assert_called_once_with("DELETE", hub.tags_url + PR_NAME + "/")

    def test_pagination(self):
        hub = object.__new__(Hub)
        hub.tags_url = "https://hub.docker.com/v2/repositories/example/flux-de-fichiers/tags/"
        next_url = hub.tags_url + "?page=2"
        with patch.object(hub, "request", side_effect=[
            {"results": [tag(PR_NAME)], "next": next_url},
            {"results": [tag(TEST_NAME)], "next": None},
        ]) as request:
            self.assertEqual(len(hub.tags()), 2)
            self.assertEqual(request.call_args.args, ("GET", next_url))

    def test_denied_delete_reports_error_without_secret(self):
        hub = object.__new__(Hub)
        hub.tags_url = "https://hub.docker.com/v2/repositories/example/flux-de-fichiers/tags/"
        hub.token = "dummy-test-token"
        hub.credentials = [hub.token]
        error = HTTPError(hub.tags_url, 403, "Forbidden", {},
                          io.BytesIO(b'{"detail":"permission denied dummy-test-token"}'))
        with patch("dockerhub_cleanup.urlopen", side_effect=error):
            with self.assertRaisesRegex(RuntimeError, "HTTP 403.*permission denied.*jeton dédié") as result:
                hub.delete(PR_NAME)
        self.assertNotIn(hub.token, str(result.exception))

    def test_foreign_pagination_host_is_rejected(self):
        hub = object.__new__(Hub)
        hub.tags_url = "https://hub.docker.com/v2/repositories/example/flux-de-fichiers/tags/"
        with self.assertRaisesRegex(RuntimeError, "Unexpected"):
            hub.request("GET", "https://other.invalid/tags/")

    def test_changed_protected_digest_fails(self):
        hub = FakeHub([tag(PR_NAME), tag(TEST_NAME)])
        original_delete = hub.delete

        def changed(name):
            original_delete(name)
            hub.data[TEST_NAME] = {"name": TEST_NAME, "digest": "sha256:changed"}

        with patch.object(hub, "delete", side_effect=changed):
            with self.assertRaisesRegex(RuntimeError, "Digest modifié"):
                purge(hub, CUTOFF, set(), False)

    @patch("dockerhub_cleanup.subprocess.run")
    def test_download_and_promotion_are_checked(self, run):
        from dockerhub_cleanup import verify_test_image
        verify_test_image("example/flux-de-fichiers", TEST_NAME)
        self.assertEqual(run.call_args_list[0].args[0][:2], ["docker", "pull"])
        self.assertIn("--dry-run", run.call_args_list[1].args[0])
        self.assertTrue(all(call.kwargs["check"] for call in run.call_args_list))


if __name__ == "__main__":
    unittest.main()
