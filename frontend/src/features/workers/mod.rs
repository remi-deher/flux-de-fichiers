//! Registre des nœuds. Une seule nature, donc pas de sous-navigation.

use crate::shared::ui::{Button, ButtonVariant, Card, RegistreFact, RegistreTable, SectionHeader};
use leptos::prelude::*;

#[component]
pub fn WorkersPanel() -> impl IntoView {
    view! {
        <div id=crate::core::panel::WORKERS.dom_id class="tab-panel" role="tabpanel" aria-labelledby=crate::core::panel::WORKERS.button_id hidden>
            <div class="registre">
                <RegistreTable
                    aria_label="Nœuds qui exécutent"
                    headers=vec!["Nœud", "Rôle", "Présence", "État", "Action"]
                    body_id="workers-table-body"
                    empty="Chargement des nœuds…"
                />
                <RegistreFact id="workers-fact" action_id="workers-fact-action" action="Raccorder" />
                <div id="worker-connect-panel" hidden>
                    <Card aria_labelledby="worker-connect-title">
                        <SectionHeader
                            id="worker-connect-title"
                            title="Raccordement du nœud"
                            subtitle="Clé d'agent et commande pour adjoindre un worker distant en HTTP."
                        />

                        <div class="detail-grid" style="margin-bottom: 1.5rem;">
                            <div class="detail-term">"Rôle de l'instance"</div>
                            <div class="detail-val"><code>"Coordinateur central"</code>" (seule autorité)"</div>

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
                                <Button
                                    id="btn-toggle-worker-key"
                                    variant=ButtonVariant::Secondary
                                    aria_label="Afficher ou masquer la clé worker"
                                    disabled=true
                                >
                                    "Afficher"
                                </Button>
                                <Button
                                    id="btn-copy-worker-key"
                                    variant=ButtonVariant::Primary
                                    aria_label="Copier la clé worker dans le presse-papier"
                                    disabled=true
                                >
                                    "Copier la clé"
                                </Button>
                            </div>
                            <p id="worker-key-hint" class="form-help">
                                "Clé secrète requise par les conteneurs worker pour s'enregistrer et réclamer des baux."
                            </p>
                        </div>

                        <div class="form-group">
                            <label for="docker-worker-cmd" class="form-label">
                                "Commande Docker de raccordement immédiat (HTTP)"
                            </label>
                            <pre class="code-block-wrapper"><code id="docker-worker-cmd">"docker run -d --name flux-worker -e COORDINATOR_URL=http://localhost:3000 -e WORKER_KEY=<WORKER_KEY> -v /chemin/data:/data flux-de-fichiers worker"</code></pre>
                            <div style="margin-top: 0.75rem;">
                                <Button
                                    id="btn-copy-worker-cmd"
                                    variant=ButtonVariant::Secondary
                                    aria_label="Copier la commande Docker pour worker dans le presse-papier"
                                >
                                    "Copier la commande"
                                </Button>
                            </div>
                        </div>
                    </Card>
                </div>
            </div>
        </div>
    }
}
