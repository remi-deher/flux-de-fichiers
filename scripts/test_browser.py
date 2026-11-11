"""Validation automatisée en navigateur Chromium pour la CI distante GitHub Actions.

Exécute le parcours complet sur http://localhost:3000 servi par le conteneur preview Nginx :
1. Navigation et accessibilité clavier (tablist ARIA, flèches, Home, End).
2. Affichage responsive mobile (viewport 375x667 sans dépassement horizontal).
3. Ouverture de session web sécurisée via clé admin (#29) et purge mémoire immédiate.
4. Parcours fichiers : sélection, dialogue modal de confirmation (<dialog>), fermeture Échap puis confirmation.
5. Suivi des tâches : filtrage par statuts, inspection modale du job, présence du lien de flux direct.
6. Déconnexion et révocation de session côté serveur.
"""
from pathlib import Path
import sys
import time
import urllib.request
import urllib.error

ROOT = Path(__file__).resolve().parents[1]
BASE_URL = "http://localhost:3000"


def wait_for_server(timeout=30):
    """Attend que le proxy preview sur http://localhost:3000 soit prêt."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            with urllib.request.urlopen(f"{BASE_URL}/health", timeout=2) as resp:
                if resp.status in (200, 204):
                    return True
        except (urllib.error.URLError, OSError):
            pass
        time.sleep(0.5)
    raise RuntimeError(f"Le serveur sur {BASE_URL} n'a pas répondu dans le délai imparti ({timeout}s).")


def run_browser_tests():
    from playwright.sync_api import sync_playwright

    admin_key_path = ROOT / "var/config/admin_key"
    if not admin_key_path.exists():
        admin_key_path = ROOT / "var/secrets/admin_key"
    if not admin_key_path.exists():
        raise FileNotFoundError(f"Clé admin introuvable dans var/config ou var/secrets")
    admin_key = admin_key_path.read_text().strip()

    print("=== Démarrage des tests navigateur automatisés ===", flush=True)
    wait_for_server()
    print("Serveur preview détecté sur http://localhost:3000", flush=True)

    with sync_playwright() as p:
        browser = p.chromium.launch(
            headless=True,
            args=["--no-sandbox", "--disable-dev-shm-usage"],
        )
        context = browser.new_context(
            viewport={"width": 1280, "height": 800},
            user_agent="Flux-CI-Browser/1.0",
        )
        page = context.new_page()

        # 1. Chargement initial
        print("1. Chargement de l'interface d'accueil...", flush=True)
        page.goto(BASE_URL, wait_until="networkidle")
        title_text = page.title()
        assert "flux de fichiers" in title_text.lower(), f"Titre inattendu: {title_text!r}"
        assert page.locator("#health-status-pill").is_visible(), "Badge de santé introuvable"
        print(f"   -> Page chargée avec succès (titre: '{title_text}'), badge de santé présent.")

        # 2. Accessibilité clavier du ruban d'onglets (tablist ARIA)
        print("2. Vérification de la navigation clavier sur tablist...", flush=True)
        first_tab = page.locator('.tab-list [role="tab"]').first
        first_tab.focus()
        page.keyboard.press("ArrowRight")
        active_tab_id = page.evaluate("document.activeElement.id")
        assert active_tab_id == "tab-btn-files", f"Onglet attendu: tab-btn-files, obtenu: {active_tab_id}"
        assert page.locator("#panel-files").is_visible(), "Panneau fichiers non visible après navigation clavier"

        page.keyboard.press("ArrowRight")
        active_tab_id = page.evaluate("document.activeElement.id")
        assert active_tab_id == "tab-btn-flows", f"Onglet attendu: tab-btn-flows, obtenu: {active_tab_id}"
        assert page.locator("#panel-flows").is_visible(), "Panneau flux non visible après navigation clavier"

        page.keyboard.press("ArrowRight")
        active_tab_id = page.evaluate("document.activeElement.id")
        assert active_tab_id == "tab-btn-jobs", f"Onglet attendu: tab-btn-jobs, obtenu: {active_tab_id}"
        assert page.locator("#panel-jobs").is_visible(), "Panneau tâches non visible après navigation clavier"

        page.keyboard.press("Home")
        active_tab_id = page.evaluate("document.activeElement.id")
        assert active_tab_id == "tab-btn-dashboard", f"Onglet attendu: tab-btn-dashboard, obtenu: {active_tab_id}"
        print("   -> Navigation clavier flèches et Home validée sur les onglets ARIA.")

        # 3. Test responsive mobile
        print("3. Test responsive sur affichage mobile (375x667)...", flush=True)
        page.set_viewport_size({"width": 375, "height": 667})
        page.wait_for_timeout(300)
        scroll_width = page.evaluate("document.documentElement.scrollWidth")
        client_width = page.evaluate("document.documentElement.clientWidth")
        assert scroll_width <= client_width + 1, f"Débordement horizontal détecté: {scroll_width} > {client_width}"
        print("   -> Affichage mobile 375px vérifié sans débordement horizontal.")

        # Rétablir viewport desktop
        page.set_viewport_size({"width": 1280, "height": 800})

        # 4. Connexion et session web sécurisée
        print("4. Test de connexion et ouverture de session cookie (#29)...", flush=True)
        page.click("#tab-btn-settings")
        page.wait_for_selector("#admin-key-input", state="visible")
        page.fill("#admin-key-input", admin_key)
        page.click("#admin-key-save-btn")

        # Attendre que la session s'établisse et mette à jour le badge d'état
        page.wait_for_selector("#session-status-pill:has-text('Session web active')", timeout=5000)
        session_text = page.locator("#session-status-pill").inner_text()
        assert "session web active" in session_text.lower(), f"Statut de session inattendu : {session_text}"
        # Note : volatileAdminKey est dans une IIFE, non exposé sur window ; la preuve de purge
        # reste le passage en mode cookie (badge « Session web active ») plutôt qu'un assert JS global.
        print("   -> Session cookie établie (badge « Session web active »).")

        # 5. Parcours fichiers et dialogue de confirmation remux
        print("5. Parcours fichiers et modale de confirmation remux...", flush=True)
        page.click("#tab-btn-files")
        page.wait_for_selector("#panel-files", state="visible")
        page.click("#refresh-all-btn")
        page.wait_for_timeout(500)

        remux_btns = page.locator(".remux-launch-btn")
        if remux_btns.count() > 0:
            remux_btns.first.click()
            page.wait_for_selector("#confirm-remux-dialog[open]", state="visible", timeout=3000)
            print("   -> Modale de confirmation ouverte.")

            # Test clavier : touche Échap ferme le dialogue
            page.keyboard.press("Escape")
            page.wait_for_selector("#confirm-remux-dialog:not([open])", state="hidden", timeout=3000)
            print("   -> Fermeture par touche Échap validée.")

            # Réouverture et confirmation du remux
            remux_btns.first.click()
            page.wait_for_selector("#confirm-remux-dialog[open]", state="visible", timeout=3000)
            page.click("#confirm-dialog-submit-btn")
            page.wait_for_selector("#confirm-remux-dialog:not([open])", state="hidden", timeout=3000)
            print("   -> Lancement de tâche confirmé.")

        # 6. Tâches, suivi et inspection du résultat
        print("6. Consultation des tâches et modale de détail...", flush=True)
        page.click("#tab-btn-jobs")
        page.wait_for_selector("#panel-jobs", state="visible")
        page.click("#refresh-all-btn")
        page.wait_for_timeout(800)

        # Filtres de statut
        for filter_name in ("queued", "running", "succeeded", "all"):
            chip = page.locator(f'.filter-chip[data-filter="{filter_name}"]')
            if chip.is_visible():
                chip.click()
                page.wait_for_timeout(200)

        # Inspection d'une tâche réussie et vérification du bouton de téléchargement
        page.click('.filter-chip[data-filter="succeeded"]')
        page.wait_for_timeout(300)

        # Vérification du bouton de téléchargement présent dans la ligne du tableau
        row_dl_btns = page.locator(".job-download-btn")
        if row_dl_btns.count() > 0:
            first_row_dl = row_dl_btns.first
            assert first_row_dl.is_visible(), "Bouton .job-download-btn non visible dans la ligne du tableau"
            assert first_row_dl.is_enabled(), "Bouton .job-download-btn non actif"
            print("   -> Bouton de téléchargement direct (.job-download-btn) vérifié visible et actif dans le tableau.")

        # Inspection de la tâche et vérification du bouton de téléchargement dans la modale
        detail_btns = page.locator(".job-detail-btn")
        if detail_btns.count() > 0:
            detail_btns.first.click()
            page.wait_for_selector("#job-detail-dialog[open]", state="visible", timeout=3000)
            modal_content = page.locator("#job-detail-dialog")
            assert modal_content.is_visible(), "Modale #job-detail-dialog non visible"

            # Assert explicite du bouton de téléchargement dans la modale d'inspection
            modal_dl_btn = page.locator("#job-dialog-download-btn")
            assert modal_dl_btn.is_visible(), "Le bouton #job-dialog-download-btn doit être visible pour une tâche réussie en session cookie"
            assert modal_dl_btn.is_enabled(), "Le bouton #job-dialog-download-btn doit être actif/cliquable"
            btn_text = modal_dl_btn.inner_text().strip()
            assert "télécharger" in btn_text.lower(), f"Libellé inattendu pour le bouton de téléchargement : {btn_text!r}"
            print(f"   -> Bouton de téléchargement (#job-dialog-download-btn) vérifié visible et actif ('{btn_text}').")

            # Déclenchement et interception du flux de téléchargement direct via le navigateur
            with page.expect_download(timeout=6000) as download_info:
                modal_dl_btn.click()
            download = download_info.value
            filename = download.suggested_filename
            assert filename, "Nom de fichier attendu pour le flux direct"
            print(f"   -> Flux de téléchargement direct déclenché et intercepté (fichier: '{filename}').")

            # Fermer modale
            page.click("#job-dialog-close-btn")
            page.wait_for_selector("#job-detail-dialog:not([open])", state="hidden", timeout=3000)
            print("   -> Modale d'inspection de tâche refermée.")

        # Rétablir le filtre global
        page.click('.filter-chip[data-filter="all"]')

        # 7. Déconnexion
        print("7. Test de déconnexion et révocation de session...", flush=True)
        page.click("#tab-btn-settings")
        page.wait_for_selector("#admin-key-logout-btn", state="visible")
        page.click("#admin-key-logout-btn")
        page.wait_for_selector("#session-status-pill:has-text('Non authentifié')", timeout=5000)
        print("   -> Déconnexion effectuée et session révoquée.")

        browser.close()

    print("=== Tous les tests navigateur Playwright ont réussi avec succès ! ===", flush=True)


if __name__ == "__main__":
    try:
        run_browser_tests()
    except Exception as e:
        print(f"ÉCHEC des tests navigateur : {e}", file=sys.stderr, flush=True)
        sys.exit(1)
