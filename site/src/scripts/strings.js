// The strings the page scripts write at run time (the download button, the 3D diagram's labels).
// Each page is built in one language and embeds just these strings for it as JSON in
// <script type="application/json" id="strings"> (see components/Strings.astro), so the browser
// never downloads the whole dictionary.

let strings = null;

// `text` with its {name} placeholders filled from `vars`; unknown placeholders stay as they are.
export function fill(text, vars = {}) {
  return text.replace(/\{(\w+)\}/g, (match, name) => (name in vars ? String(vars[name]) : match));
}

function load() {
  if (strings) return strings;
  try {
    strings = JSON.parse(document.getElementById("strings")?.textContent ?? "{}") ?? {};
  } catch {
    strings = {};
  }
  return strings;
}

// The page's string for `key` (the key itself if the page lacks it), placeholders filled.
export function t(key, vars) {
  return fill(load()[key] ?? key, vars);
}
