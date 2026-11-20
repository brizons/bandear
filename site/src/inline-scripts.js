// The scripts the pages inline in <head>: they run before the first paint, so they cannot wait
// for a module. Kept here as source strings so node tests can run them against stubbed browser
// globals (tests/inline-scripts.test.mjs) and the components inline exactly what was tested.

import { DEFAULT_LANGUAGE, PATHS, STORAGE_KEY, pickLanguage } from "./i18n.js";

// Decides before the first paint whether the decorative 3D layers replace the static visuals, so
// nothing shifts later. three.js needs WebGL 2, and a software renderer (no hardware GPU, a
// "major performance caveat") would draw the scenes on the CPU: the static design stays then.
// If the page script never runs (blocked, or it failed to load), the static design comes back
// at `load`: scripts/main.js sets data-booted on <html>.
export const WEBGL_PROBE = `(function () {
  try {
    var gl = document.createElement("canvas").getContext("webgl2", { failIfMajorPerformanceCaveat: true });
    if (!gl) return;
    var root = document.documentElement;
    root.classList.add("webgl");
    var lose = gl.getExtension("WEBGL_lose_context");
    if (lose) lose.loseContext();
    window.addEventListener("load", function () {
      if (!root.dataset.booted) root.classList.remove("webgl");
    });
  } catch (error) {
    /* Static design. */
  }
})();`;

// pickLanguage as a function expression, between markers the tests look for.
export const PICK_LANGUAGE = `/* BEGIN pickLanguage */ var pickLanguage = ${pickLanguage.toString()}; /* END pickLanguage */`;

// The language switch and the arrival redirect, for the page in <html lang>:
// - the switch (a[data-lang-switch]) saves the visitor's choice and keeps the current section;
// - on arrival from another site, a saved choice wins; without one, the default-language page
//   (also x-default) follows the browser's first language. Moving around the site, the switch
//   included, never redirects, so the switch works even where storage is blocked. Crawlers
//   arrive with no saved choice and an English browser, so every page stays reachable for them.
export const LANGUAGE_SWITCH = `(function () {
  var STORAGE_KEY = ${JSON.stringify(STORAGE_KEY)};
  var DEFAULT_LANGUAGE = ${JSON.stringify(DEFAULT_LANGUAGE)};
  var PATHS = ${JSON.stringify(PATHS)};
  ${PICK_LANGUAGE}
  var lang = document.documentElement.lang;
  var stored = null;
  try {
    stored = localStorage.getItem(STORAGE_KEY);
  } catch (error) {
    /* Blocked storage: no saved choice. */
  }
  document.addEventListener("click", function (event) {
    var link = event.target && event.target.closest ? event.target.closest("a[data-lang-switch]") : null;
    if (!link) return;
    try {
      localStorage.setItem(STORAGE_KEY, link.getAttribute("hreflang"));
    } catch (error) {
      /* Blocked storage: the choice lasts while the visitor stays on the site. */
    }
    link.hash = location.hash;
  });
  var fromSite = false;
  try {
    fromSite = new URL(document.referrer).origin === location.origin;
  } catch (error) {
    /* No referrer: an arrival. */
  }
  if (fromSite) return;
  var languages = navigator.languages && navigator.languages.length ? navigator.languages : [navigator.language];
  var wanted = pickLanguage(stored, lang === DEFAULT_LANGUAGE ? languages : [lang]);
  if (wanted !== lang && PATHS[wanted]) location.replace(PATHS[wanted] + location.search + location.hash);
})();`;
