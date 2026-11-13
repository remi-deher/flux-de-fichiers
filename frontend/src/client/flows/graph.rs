//! Modèle du graphe édité : fonctions pures, sans DOM.
//!
//! L'éditeur ne lit et n'écrit le flux qu'à travers ce modèle : la définition enregistrée est
//! relue telle quelle (blocs, liaisons, réglages et clés inconnues), puis renvoyée sans perte.

pub const SCRIPT: &str = r###"
  // --- Modèle du graphe édité (fonctions pures) ---

  // Définition stockée → graphe éditable. Les clés inconnues sont gardées dans `extra`.
  function parseFlowGraph(definition) {
    let raw = definition;
    if (typeof raw === 'string') {
      try {
        raw = JSON.parse(raw);
      } catch {
        raw = null;
      }
    }
    const source = raw && typeof raw === 'object' && !Array.isArray(raw) ? raw : {};
    const { nodes, connections, ...extra } = source;
    return {
      nodes: Array.isArray(nodes) ? nodes.map(node => ({ ...node, config: node.config ?? {} })) : [],
      connections: Array.isArray(connections) ? connections.map(edge => ({ ...edge })) : [],
      extra,
    };
  }

  // Graphe éditable → définition JSON, sans rien perdre de ce qui a été lu.
  function serializeFlowGraph(graph) {
    return JSON.stringify({ ...graph.extra, nodes: graph.nodes, connections: graph.connections });
  }

  function graphNode(graph, nodeId) {
    return graph.nodes.find(node => node.id === nodeId) || null;
  }

  function outgoingEdges(graph, nodeId) {
    return graph.connections.filter(edge => edge.from === nodeId);
  }

  function incomingEdges(graph, nodeId) {
    return graph.connections.filter(edge => edge.to === nodeId);
  }

  // Ports de sortie d'un bloc : ceux de ses liaisons, ou une sortie simple par défaut.
  function outputPorts(graph, node) {
    const ports = [...new Set(outgoingEdges(graph, node.id).map(edge => edge.port ?? null))];
    if (ports.length > 0) return ports;
    return node.type === FLOW_OUTPUT_BLOCK ? [] : [null];
  }

  function nextNodeId(graph) {
    const used = new Set(graph.nodes.map(node => node.id));
    let index = graph.nodes.length + 1;
    while (used.has(`node-${index}`)) index += 1;
    return `node-${index}`;
  }

  // Ajoute un bloc et le relie au dernier bloc sans suite (hors sortie), comme le tracé l'affiche.
  function appendGraphNode(graph, type, name, config = {}) {
    const node = { id: nextNodeId(graph), type, name, config };
    const tail = [...graph.nodes].reverse().find(candidate =>
      candidate.type !== FLOW_OUTPUT_BLOCK && outgoingEdges(graph, candidate.id).length === 0);
    graph.nodes.push(node);
    if (tail) graph.connections.push({ from: tail.id, to: node.id });
    return node;
  }

  // Retire un bloc et ses liaisons ; un maillon simple (une entrée, une sortie) est raccordé.
  function removeGraphNode(graph, nodeId) {
    const incoming = incomingEdges(graph, nodeId);
    const outgoing = outgoingEdges(graph, nodeId);
    graph.nodes = graph.nodes.filter(node => node.id !== nodeId);
    graph.connections = graph.connections.filter(edge => edge.from !== nodeId && edge.to !== nodeId);
    if (incoming.length === 1 && outgoing.length === 1 && outgoing[0].port == null) {
      const bridge = { from: incoming[0].from, to: outgoing[0].to };
      if (incoming[0].port != null) bridge.port = incoming[0].port;
      graph.connections.push(bridge);
    }
  }

  // Corps de `PUT /api/v1/flows/{id}` : les métadonnées du flux enregistré sont conservées.
  function flowSaveRequest(flow, graph) {
    return {
      name: flow.name,
      description: flow.description ?? null,
      file_mask: flow.file_mask || DEFAULT_FILE_MASK,
      is_default: Boolean(flow.is_default),
      definition: serializeFlowGraph(graph),
    };
  }

  // Identifiant d'un nouveau flux, dérivé de son nom.
  function newFlowId(name, suffix) {
    const slug = String(name || '')
      .normalize('NFD').replace(/[̀-ͯ]/g, '')
      .toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '')
      .slice(0, FLOW_ID_SLUG_MAX);
    return `flow.${slug || 'nouveau'}-${suffix}`;
  }

  // Réglages saisis en JSON : un objet, ou une erreur lisible.
  function parseNodeConfig(text) {
    if (String(text).trim() === '') return { ok: true, value: {} };
    try {
      const value = JSON.parse(text);
      if (value && typeof value === 'object' && !Array.isArray(value)) return { ok: true, value };
      return { ok: false, error: 'Les réglages doivent être un objet JSON ({ … }).' };
    } catch (error) {
      return { ok: false, error: `JSON invalide : ${error.message}` };
    }
  }
"###;

/// Constantes partagées par le modèle, émises avant ses fonctions.
pub fn constants() -> String {
    format!(
        "\n  const FLOW_OUTPUT_BLOCK = {output:?};\n  const DEFAULT_FILE_MASK = {mask:?};\n  const FLOW_ID_SLUG_MAX = {slug};\n",
        output = FLOW_OUTPUT_BLOCK,
        mask = DEFAULT_FILE_MASK,
        slug = FLOW_ID_SLUG_MAX,
    )
}

/// Bloc terminal : il n'a pas de sortie.
pub const FLOW_OUTPUT_BLOCK: &str = "file-output";
/// Masque appliqué à un flux créé depuis l'éditeur.
pub const DEFAULT_FILE_MASK: &str = "*.mkv";
/// Longueur maximale de la partie lisible d'un identifiant de flux.
pub const FLOW_ID_SLUG_MAX: usize = 40;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_functions_and_constants_are_emitted() {
        for name in [
            "function parseFlowGraph",
            "function serializeFlowGraph",
            "function appendGraphNode",
            "function removeGraphNode",
            "function flowSaveRequest",
            "function newFlowId",
            "function parseNodeConfig",
            "function outputPorts",
        ] {
            assert!(SCRIPT.contains(name), "{name}");
        }
        let constants = constants();
        assert!(constants.contains("\"file-output\""));
        assert!(constants.contains("\"*.mkv\""));
        assert!(constants.contains("40"));
    }

    #[test]
    fn save_never_resets_connections_or_metadata() {
        assert!(!SCRIPT.contains("connections: []"));
        assert!(SCRIPT.contains("is_default: Boolean(flow.is_default)"));
        assert!(SCRIPT.contains("description: flow.description"));
    }
}
