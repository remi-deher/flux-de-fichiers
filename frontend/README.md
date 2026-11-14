# Page et thème de l'interface

Toute l'interface est le client Leptos compilé en WASM, dans [`client/`](../client)
([ADR 0014](../docs/adr/0014-client-leptos-wasm.md)). Ce crate ne fait que deux choses :

- `src/theme/` : les styles typés (jetons clair/sombre, composants), consolidés par
  `theme::get_all_styles()` ;
- `src/lib.rs` : la page unique `dist/index.html`, avec ces styles et le chargement du module
  `/pkg/flux_client.js`.

Le central sert `dist/index.html` pour toute route d'interface et `dist/pkg/` pour le WASM.

## Construire

L'image le fait (`Dockerfile`) ; à la main :

```sh
cargo run -p flux-frontend
cargo build --release -p flux-client --target wasm32-unknown-unknown
wasm-bindgen --target web --no-typescript --out-dir frontend/dist/pkg \
  target/wasm32-unknown-unknown/release/flux_client.wasm
```

`wasm-bindgen-cli` doit avoir la version exacte du crate `wasm-bindgen` verrouillé
(`WASM_BINDGEN_VERSION` du `Dockerfile`).
