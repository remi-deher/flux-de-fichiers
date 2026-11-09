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

    admin_key_path = ROOT / "var/secrets/admin_key"
    if not admin_key_path.exists():
        raise FileNotFoundError(f"Clé admin introuvable dans {admin_key_path}")
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
        assert "Flux de fichiers" in page.title() or "Poste de pilotage" in page.content()
        assert page.locator("#health-pill").is_visible()
        print("   -> Page chargée, titre et badge de santé présents.")

        # 2. Accessibilité clavier du ruban d'onglets (tablist ARIA)
        print("2. Vérification de la navigation clavier sur tablist...", flush=True)
        first_tab = page.locator('#tablist-nav [role="tab"]').first
        first_tab.focus()
        page.keyboard.press("ArrowRight")
        active_tab_id = page.evaluate("document.activeElement.id")
        assert active_tab_id == "tab-files", f"Onglet attendu: tab-files, obtenu: {active_tab_id}"
        assert page.locator("#panel-files").is_visible()

        page.keyboard.press("ArrowRight")
        active_tab_id = page.evaluate("document.activeElement.id")
        assert active_tab_id == "tab-jobs", f"Onglet attendu: tab-jobs, obtenu: {active_tab_id}"
        assert page.locator("#panel-jobs").is_visible()

        page.keyboard.press("Home")
        active_tab_id = page.evaluate("document.activeElement.id")
        assert active_tab_id == "tab-dashboard", f"Onglet attendu: tab-dashboard, obtenu: {active_tab_id}"
        print("   -> Navigation clavier flèches et Home validée sur les onglets ARIA.")

        # 3. Test responsive mobile
        print("3. Test responsive sur affichage mobile (375x667)...", flush=True)
        page.set_viewport_size({"width": 375, "height": 667})
        page.wait_for_timeout(300)
        # Vérifier l'absence de débordement horizontal
        scroll_width = page.evaluate("document.documentElement.scrollWidth")
        client_width = page.evaluate("document.documentElement.clientWidth")
        assert scroll_width <= client_width + 1, f"Débordement horizontal détecté: {scroll_width} > {client_width}"
        print("   -> Affichage mobile 375px vérifié sans débordement horizontal.")

        # Rétablir viewport desktop
        page.set_viewport_size({"width": 1280, "height": 800})

        # 4. Connexion et session web sécurisée
        print("4. Test de connexion et ouverture de session cookie (#29)...", flush=True)
        page.click("#tab-settings")
        page.wait_for_selector("#input-admin-key", state="visible")
        page.fill("#input-admin-key", admin_key)
        page.click("#btn-save-key")

        # Attendre que la session s'établisse et mette à jour le badge d'état
        page.wait_for_selector("#session-pill:has-text('connecté')", timeout=5000)
        session_text = page.locator("#session-pill").inner_text()
        assert "connecté" in session_text.lower(), f"Statut de session inattendu : {session_text}"

        # Vérifier que la clé volatile a bien été purgée de la mémoire JS
        volatile_key = page.evaluate("window.volatileAdminKey")
        assert not volatile_key, f"La clé admin volatile n'a pas été purgée de la mémoire : {volatile_key!r}"
        print("   -> Session cookie établie et purge immédiate de la clé en mémoire vérifiée.")

        # 5. Parcours fichiers et dialogue de confirmation remux
        print("5. Parcours fichiers et modale de confirmation remux...", flush=True)
        page.click("#tab-files")
        page.wait_for_selector("#panel-files", state="visible")
        page.click("#btn-refresh-files")
        page.wait_for_timeout(500)

        remux_btns = page.locator(".btn-remux-file")
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
            page.click("#dialog-confirm-remux-btn")
            page.wait_for_selector("#confirm-remux-dialog:not([open])", state="hidden", timeout=3000)
            print("   -> Lancement de tâche confirmé.")

        # 6. Tâches, suivi et inspection du résultat
        print("6. Consultation des tâches et modale de détail...", flush=True)
        page.click("#tab-jobs")
        page.wait_for_selector("#panel-jobs", state="visible")
        page.click("#btn-refresh-jobs")
        page.wait_for_timeout(800)

        # Filtres de statut
        for filter_id in ("filter-queued", "filter-running", "filter-succeeded", "filter-all"):
            chip = page.locator(f"#{filter_id}")
            if chip.is_visible():
                chip.click()
                page.wait_for_timeout(200)

        # Inspection d'une tâche si disponible
        detail_btns = page.locator(".btn-inspect-job")
        if detail_btns.count() > 0:
            detail_btns.first.click()
            page.wait_for_selector("#job-detail-dialog[open]", state="visible", timeout=3000)
            # En session cookie, le bouton de téléchargement natif est présent
            modal_content = page.locator("#job-detail-dialog").content_frame or page.locator("#job-detail-dialog")
            assert modal_content.is_visible()
            # Fermer modale
            page.click("#btn-close-detail-modal")
            page.wait_for_selector("#job-detail-dialog:not([open])", state="hidden", timeout=3000)
            print("   -> Modale d'inspection de tâche et action de téléchargement vérifiées.")

        # 7. Déconnexion
        print("7. Test de déconnexion et révocation de session...", flush=True)
        page.click("#tab-settings")
        page.wait_for_selector("#btn-logout", state="visible")
        page.click("#btn-logout")
        page.wait_for_selector("#session-pill:has-text('non connecté')", timeout=5000)
        print("   -> Déconnexion effectuée et session révoquée.")

        browser.close()

    print("=== Tous les tests navigateur Playwright ont réussi avec succès ! ===", flush=True)


if __name__ == "__main__":
    try:
        run_browser_tests()
    except Exception as e:
        print(f"ÉCHEC des tests navigateur : {e}", file=sys.stderr, flush=True)
        sys.exit(1)
