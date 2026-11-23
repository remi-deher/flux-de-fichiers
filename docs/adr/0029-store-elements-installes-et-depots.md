# 0029 — Store, éléments installés et dépôts

Statut : accepté par l'utilisateur (22 nov. 2026). Modifie les ADR 0010 (§1 à 3 et 5), 0014 et 0025 (import de script).

## Contexte

Un dépôt synchronisé alimentait directement le moteur. Une synchronisation changeait donc les opérations des
tâches en file ; désactiver ou retirer un dépôt cassait les flux qui s'en servaient. « Installer » ne
téléchargeait que des outils, et le bouton d'un plugin ne s'affichait que pour des dépendances obligatoires,
qu'aucun plugin officiel ne déclare. Un pack vérifiait la présence de ses plugins et flux sans rien en faire.

## Décision

1. **Vitrine et moteur séparés.** Les dépôts synchronisés (`/config/plugins/<dépôt>`) ne servent qu'au Store. Le
   moteur ne lit que `PLUGINS_DIR` s'il est donné explicitement, la collection locale (`/config/plugins/local`) et
   les copies figées (`/config/installed`). Un dossier de plugins détecté automatiquement à côté du projet est
   une source de vitrine, pas du moteur.
2. **Copie figée.** Installer copie les fichiers de l'élément dans `/config/installed/<id>.<empreinte>/`, un
   mini-dépôt immuable avec son index. La table `installed_items` (migration 0050) retient type, version,
   empreinte, dépôt et URL d'origine, sans clé étrangère : elle survit au dépôt. La base fait autorité ; un
   dossier non référencé est un reste, balayé sous verrou après un délai de garde. Le dossier est écrit à part,
   synchronisé sur disque, puis renommé avant la validation en base.
3. **Chemins.** Identifiants, chemins de manifeste et chemins déclarés sont contrôlés avant toute copie (nom
   sûr, chemin relatif sûr, pas de lien symbolique, tailles et nombre de fichiers bornés).
4. **Par type.** Plugin, outil, préréglage et ressource : copie figée lue par le moteur. Flux : nouveau flux
   modifiable, plugins requis installés. Script : copie intégrale et modifiable dans la collection locale,
   identifiants de bloc et d'opération conservés. Pack : chacun de ses composants est installé ; le pack n'a pas
   d'état propre, le sien se déduit de celui de ses composants.
5. **Plan puis application.** Le plan montre éléments, dépendances (déclarées, et déduites des blocs, des
   opérations et des `requires`), outils recommandés cochés et modifiables, blocages et avertissements.
   L'application recalcule le plan ; s'il a changé, elle est refusée. Un plan accepte plusieurs cibles
   (« tout mettre à jour »).
6. **Atomicité.** Les éléments d'un plan sont tous installés ou aucun. Les outils se téléchargent ensuite, un par
   un, sans rien défaire en cas d'échec ; un outil décoché garde son contrat figé et peut être téléchargé plus
   tard, ou par la première tâche qui l'exige. Une dépendance `optional` présente dans le dépôt est cochée.
7. **Versions.** `^x.y.z`, les versions des composants de pack et celles exigées par les opérations sont des
   minimums de même version majeure. Une mise à jour qui rompt un élément installé dépendant est bloquée.
   `compatibility.appMin` reste informatif.
8. **Mises à jour** manuelles et signalées : version plus récente dans le dépôt d'origine, ou même version et
   contenu différent. Un flux n'est jamais touché : une mise à jour crée un nouveau flux. Un script n'est remplacé
   que s'il est resté identique à sa copie d'origine, et seulement à la demande. Un bloc retiré est annoncé ; il
   bloque la mise à jour si une tâche en attente ou en cours l'utilise.
9. **Désinstallation** refusée, en nommant l'usage, si un flux, une tâche non terminée (flux d'échec et
   notifications en attente compris), un modèle local ou un script de la collection utilise l'élément, ou s'il porte la dernière
   sonde primaire ou la dernière opération par défaut du moteur. Les réglages sont conservés et restent effaçables. Un
   script est retiré avec sa copie ; un flux installé appartient à l'utilisateur et se supprime comme un autre.
   Retirer un outil libère son binaire ; son contrat reste tant qu'un élément installé le déclare.
10. **Origine.** L'origine d'un élément et de ses réglages est le couple (identifiant, URL) du dépôt. Le Store
    montre les homonymes ; un seul est installable, et prendre celui d'une autre origine exige d'effacer
    explicitement les réglages. Types de blocs, alias et opérations sont uniques dans le moteur ; un conflit
    hérité n'empêche pas une mise à jour qui n'en ajoute pas. Un espace exposé partagé reste permis (ADR 0025).
11. **Accès entre plugins.** Un plugin qui lit les réglages d'un autre, ou qui prend la place de la sonde primaire ou de l'opération par
    défaut, ne s'installe qu'avec un consentement explicite affiché dans le plan.
12. **Outils.** Versions et purge viennent des contrats figés. Aucune purge avant la fin de la reprise, avec une
    liste vide, ni tant qu'une tâche tourne. Les workers distants ne changent pas : tout vient du bail.
13. **Reprise au démarrage**, sans réseau, dépôt par dépôt, depuis les copies déjà sur disque : `essentiel`, le
    socle (sonde primaire, opération par défaut, sondes de capacités), les éléments utilisés par les flux actuels,
    les tâches non terminées et les modèles locaux, les plugins configurés, et les contrats de leurs outils. Un
    dépôt reste lu par le moteur tant que ses éléments voulus ne sont pas tous figés au complet ; aucune fenêtre
    ne laisse un flux sans ses blocs. Un élément défectueux n'arrête pas la reprise des autres. Une installation neuve reçoit `essentiel`
    et le socle (aujourd'hui `video`).
14. **Copie figée manquante.** Une tâche qui en dépend rend son bail au lieu d'échouer, et l'élément est signalé
    « à réparer ».
15. **Droits.** Lire : `view_plugins`. Installer, retirer : `manage_plugins`, plus `manage_flows` si le plan crée
    un flux.
16. **Interface.** Trois onglets : Store, Installés, Dépôts ; les outils sont dans les deux premiers. Le parcours
    d'installation est le même partout : plan, installation, réglages et test, utilisation.

## Conséquences

- Retirer ou désactiver un dépôt ne change plus le moteur.
- Une mise à jour s'applique à l'étape suivante d'une tâche, comme avant, mais seulement à la demande.
- La migration 0050 interdit de relancer une version antérieure de Forgearr sur la même base. Une copie de la base
  est écrite dans `/config/backups` avant cette migration ; un échec de copie arrête le démarrage.
- Les routes d'installation antérieures et `catalog-scripts` disparaissent avec l'ancienne interface.
- Limite connue : l'épinglage d'une archive d'outil ignore le dépôt ; le même outil à la même version, avec une
  autre archive, reste refusé.
