// Reads the built website (site/dist/) for the tests that check what visitors and crawlers get.
// Build it first: `npm run build` in site/ (the Pages workflow builds before it tests).

import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

export const SITE = fileURLToPath(new URL("..", import.meta.url));
export const DIST = join(SITE, "dist");

// The home page of each language, by language.
export const HOME_PAGES = Object.freeze({ en: "index.html", es: "es/index.html" });

export function built(path) {
  const file = join(DIST, path);
  if (!existsSync(file)) throw new Error(`${file} is missing: run \`npm run build\` in site/ before the tests`);
  return readFileSync(file, "utf8");
}

// Every .astro source file under src/, as { path, source }.
export function astroSources(dir = join(SITE, "src")) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return astroSources(path);
    return entry.name.endsWith(".astro") ? [{ path, source: readFileSync(path, "utf8") }] : [];
  });
}

// Text as Astro escapes it inside elements and attributes.
export const escapeHtml = (text) =>
  text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;").replace(/'/g, "&#39;");

// The attributes of every tag named `tag` in `html`, as plain objects (values unescaped).
export function tags(html, tag) {
  return [...html.matchAll(new RegExp(`<${tag}\\b([^>]*)>`, "g"))].map(([, attrs]) => {
    const found = {};
    for (const [, name, , value] of attrs.matchAll(/([\w:-]+)(="([^"]*)")?/g)) {
      found[name] = (value ?? "").replace(/&quot;/g, '"').replace(/&#39;/g, "'").replace(/&amp;/g, "&");
    }
    return found;
  });
}
