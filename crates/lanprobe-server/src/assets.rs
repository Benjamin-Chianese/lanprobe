//! Assets frontend embarqués via rust-embed.
//!
//! Le build Svelte est dans `../../build` (adapter-static, fallback SPA).
//! Le dossier doit exister au moment de `cargo build` — le workflow CI
//! lance `npm run build` avant `cargo build -p lanprobe-server`.

use axum::{
    body::Body,
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/../../build"]
struct Assets;

/// Petit patch injecté dans index.html : remplace le runtime Tauri par
/// un shim HTTP+WS qui parle au serveur. Le `@tauri-apps/api` appelle
/// `window.__TAURI_INTERNALS__.invoke` et `transformCallback`, on les
/// redirige donc vers `/api/invoke/...` et une WebSocket `/ws`.
const TAURI_SHIM: &str = r#"<script>
(function(){
  const callbacks = {};
  let nextId = 1;
  const listeners = {};
  let ws = null;
  let wsReady = null;

  function connectWs() {
    if (wsReady) return wsReady;
    wsReady = new Promise((resolve) => {
      const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
      ws = new WebSocket(proto + '//' + location.host + '/ws');
      ws.onmessage = (ev) => {
        try {
          const msg = JSON.parse(ev.data);
          const set = listeners[msg.event];
          if (set) {
            for (const { handler } of Object.values(set)) {
              handler({ event: msg.event, id: 0, payload: msg.payload });
            }
          }
        } catch (e) { console.error('ws parse', e); }
      };
      ws.onopen = () => resolve();
      ws.onclose = () => { ws = null; wsReady = null; };
    });
    return wsReady;
  }

  window.__TAURI_INTERNALS__ = {
    transformCallback: function(cb, once) {
      const id = nextId++;
      callbacks[id] = { cb, once };
      return id;
    },
    invoke: async function(cmd, args, options) {
      args = args || {};
      if (cmd === 'plugin:event|listen') {
        await connectWs();
        const { event, handler } = args;
        const id = nextId++;
        listeners[event] = listeners[event] || {};
        listeners[event][id] = { handler: (payload) => {
          const cb = callbacks[handler];
          if (cb) cb.cb(payload);
        }};
        return id;
      }
      if (cmd === 'plugin:event|unlisten') {
        const { event, eventId } = args;
        if (listeners[event]) delete listeners[event][eventId];
        return null;
      }
      const r = await fetch('/api/invoke/' + encodeURIComponent(cmd), {
        method: 'POST',
        headers: {'Content-Type': 'application/json'},
        credentials: 'same-origin',
        body: JSON.stringify(args)
      });
      const text = await r.text();
      if (!r.ok) throw text;
      return text ? JSON.parse(text) : null;
    }
  };
  window.__TAURI_METADATA__ = { __currentWindow: { label: 'main' }, __windows: [{ label: 'main' }] };
  window.__LANPROBE_WEB__ = true;
  window.__LANPROBE_HEADLESS__ = true;
})();
</script>"#;

pub async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let candidate = if path.is_empty() { "index.html" } else { path };

    if let Some(file) = Assets::get(candidate) {
        return build_response(candidate, file.data.into_owned());
    }

    // SPA fallback : toute route inconnue retombe sur index.html pour que
    // SvelteKit/router gère côté client.
    if let Some(index) = Assets::get("index.html") {
        return build_response("index.html", index.data.into_owned());
    }

    (StatusCode::NOT_FOUND, "asset not found").into_response()
}

fn build_response(path: &str, data: Vec<u8>) -> Response {
    let mime = mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string();

    if path == "index.html" {
        if let Ok(html) = String::from_utf8(data.clone()) {
            let patched = html.replacen("<head>", &format!("<head>{TAURI_SHIM}"), 1);
            return Response::builder()
                .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                .body(Body::from(patched))
                .unwrap();
        }
    }

    Response::builder()
        .header(header::CONTENT_TYPE, mime)
        .body(Body::from(data))
        .unwrap()
}
