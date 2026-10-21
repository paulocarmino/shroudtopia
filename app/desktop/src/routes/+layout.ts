// Tauri has no Node.js server for SSR: adapter-static with an index.html fallback puts the app in SPA mode.
export const ssr = false;
export const prerender = true;
