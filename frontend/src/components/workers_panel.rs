//! Panneau de supervision des serveurs et agents de remuxage.

use leptos::prelude::*;

#[component]
pub fn WorkersPanel() -> impl IntoView {
    view! {
        <div id="panel-workers" class="tab-panel" role="tabpanel" aria-labelledby="tab-btn-workers" hidden>
            <section class="section-panel" aria-labelledby="workers-section-title">
                <div class="section-panel-header">
                    <div>
                        <h2 id="workers-section-title" class="section-title">"Serveurs et agents de traitement"</h2>
                        <p class="section-subtitle">
                            "Agents de calcul enregistrés via l'API. Chaque agent traite les baux sur MKVToolNix sans connexion directe à la base de données."
                        </p>
                    </div>
                </div>

                <div class="table-wrapper">
                    <table class="data-table" aria-label="Liste des agents et serveurs de traitement">
                        <thead>
                            <tr>
                                <th scope="col">"Identifiant de l'agent"</th>
                                <th scope="col">"Nom d'hôte / Rôle"</th>
                                <th scope="col">"Dernière activité"</th>
                                <th scope="col">"Statut"</th>
                            </tr>
                        </thead>
                        <tbody id="workers-table-body">
                            <tr>
                                <td colspan="4" class="empty-state-text">
                                    "Chargement des serveurs et agents…"
                                </td>
                            </tr>
                        </tbody>
                    </table>
                </div>

                <div class="workers-info-grid">
                    <div class="metric-card">
                        <div class="metric-label">"Architecture distribuée"</div>
                        <p class="card-caption">
                            "Mode centralisé de 1 à N serveurs. Le coordinateur central gère les métadonnées et distribue les tâches avec des baux de 120 s renouvelés toutes les 20 s."
                        </p>
                    </div>
                    <div class="metric-card">
                        <div class="metric-label">"Haute disponibilité"</div>
                        <p class="card-caption">
                            "La haute disponibilité reste optionnelle et nécessite un cadrage distinct ; aucun seuil de nœuds ne l'active automatiquement."
                        </p>
                    </div>
                </div>
            </section>

            <section class="section-panel" aria-labelledby="worker-connect-title" style="margin-top: 1.5rem;">
                <div class="section-panel-header">
                    <div>
                        <h2 id="worker-connect-title" class="section-title">"Raccordement d'un agent distant (*arr)"</h2>
                        <p class="section-subtitle">
                            "Configuration auto-amorcée du coordinateur (/config) et commande Docker pour adjoindre un worker distant en HTTP."
                        </p>
                    </div>
                </div>

                <div class="detail-grid" style="margin-bottom: 1.5rem;">
                    <div class="detail-term">"Répertoire de configuration"</div>
                    <div class="detail-val"><code>"/config"</code>" (SQLite flux.db, clés 256 bits auto-amorcées)"</div>

                    <div class="detail-term">"Worker local intégré"</div>
                    <div class="detail-val" id="config-local-worker">"Vérification…"</div>

                    <div class="detail-term">"Version du coordinateur"</div>
                    <div class="detail-val" id="config-version">"Vérification…"</div>

                    <div class="detail-term">"Taille max par fichier"</div>
                    <div class="detail-val" id="config-max-file-bytes">"Vérification…"</div>
                </div>

                <div class="form-group" style="margin-bottom: 1.5rem;">
                    <label for="worker-key-display" class="form-label">
                        "Clé d'authentification worker (WORKER_KEY)"
                    </label>
                    <div class="copy-input-row">
                        <input
                            type="password"
                            id="worker-key-display"
                            class="form-input"
                            readonly
                            aria-label="Clé secrète d'agent worker"
                            value=""
                            placeholder="Ouvrez une session admin pour révéler la clé"
                            style="flex: 1; font-family: var(--font-mono); font-size: 0.875rem;"
                        />
                        <button
                            type="button"
                            id="btn-toggle-worker-key"
                            class="btn btn-secondary"
                            aria-label="Afficher ou masquer la clé worker"
                            disabled
                        >
                            "Afficher"
                        </button>
                        <button
                            type="button"
                            id="btn-copy-worker-key"
                            class="btn btn-primary"
                            aria-label="Copier la clé worker dans le presse-papier"
                            disabled
                        >
                            "Copier la clé"
                        </button>
                    </div>
                    <p id="worker-key-hint" class="form-help">
                        "🔒 Clé secrète requise par les conteneurs worker pour s'enregistrer et réclamer des baux de remuxage."
                    </p>
                </div>

                <div class="form-group">
                    <label for="docker-worker-cmd" class="form-label">
                        "Commande Docker de raccordement immédiat (HTTP)"
                    </label>
                    <pre class="code-block-wrapper"><code id="docker-worker-cmd">"docker run -d --name flux-worker -e COORDINATOR_URL=http://localhost:3000 -e WORKER_KEY=<WORKER_KEY> -v /chemin/data:/data flux-de-fichiers worker"</code></pre>
                    <div style="margin-top: 0.75rem;">
                        <button
                            type="button"
                            id="btn-copy-worker-cmd"
                            class="btn btn-secondary"
                            aria-label="Copier la commande Docker pour worker dans le presse-papier"
                        >
                            "Copier la commande"
                        </button>
                    </div>
                </div>
            </section>
        </div>
    }
}
