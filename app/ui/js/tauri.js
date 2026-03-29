// Tauri command invoker injected by the WebView runtime (v1/v2 compatible).
export const invoke = window.__TAURI__?.core?.invoke ?? window.__TAURI__?.invoke;
