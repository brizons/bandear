// Tests for the scripts the pages inline in <head>, run against stubbed browser globals: the
// WebGL probe (only WebGL 2 without a major performance caveat turns the 3D layers on, and a page
// script that never runs brings the static design back) and the language switch and arrival redirect.
// Run: npm run build && npm test (in site/)

import { test } from "node:test";
import assert from "node:assert/strict";

import { LANGUAGE_SWITCH, WEBGL_PROBE } from "../src/inline-scripts.js";
import { HOME_PAGES, built } from "./built.mjs";

const ORIGIN = "https://bandear.com";

// Runs LANGUAGE_SWITCH on the page of `lang`; returns where it redirected (or null), the saved
// choice and a click() that follows the language switch.
function runLanguageSwitch({ lang, stored = null, languages = ["en-US"], referrer = "", search = "", hash = "", storage = true }) {
  const saved = { value: stored };
  let redirect = null;
  let clickListener = null;
  const localStorage = {
    getItem: () => {
      if (!storage) throw new Error("blocked");
      return saved.value;
    },
    setItem: (key, value) => {
      if (!storage) throw new Error("blocked");
      saved.value = value;
    },
  };
  const document = {
    documentElement: { lang },
    referrer,
    addEventListener: (type, listener) => {
      if (type === "click") clickListener = listener;
    },
  };
  const location = { origin: ORIGIN, search, hash, replace: (to) => (redirect = to) };
  const navigator = { languages, language: languages[0] };
  new Function("document", "location", "navigator", "localStorage", "URL", LANGUAGE_SWITCH)(
    document,
    location,
    navigator,
    localStorage,
    URL,
  );
  const click = (hreflang) => {
    const link = { hash: "", getAttribute: (name) => (name === "hreflang" ? hreflang : null) };
    const target = { closest: (selector) => (selector === "a[data-lang-switch]" ? link : null) };
    clickListener({ target });
    return link;
  };
  return { redirect, saved: () => saved.value, click };
}

test("a Spanish browser arriving at / is sent to /es/, keeping the query and section", () => {
  assert.equal(runLanguageSwitch({ lang: "en", languages: ["es-AR", "en"] }).redirect, "/es/");
  assert.equal(
    runLanguageSwitch({ lang: "en", languages: ["es"], search: "?ref=x", hash: "#faq" }).redirect,
    "/es/?ref=x#faq",
  );
});

test("an English browser, or a crawler, stays on the page it asked for", () => {
  assert.equal(runLanguageSwitch({ lang: "en", languages: ["en-US"] }).redirect, null);
  assert.equal(runLanguageSwitch({ lang: "es", languages: ["en-US"] }).redirect, null);
  assert.equal(runLanguageSwitch({ lang: "es", languages: [] }).redirect, null);
});

test("a saved choice wins on arrival, on either page", () => {
  assert.equal(runLanguageSwitch({ lang: "en", stored: "es", languages: ["en-US"] }).redirect, "/es/");
  assert.equal(runLanguageSwitch({ lang: "en", stored: "en", languages: ["es-MX"] }).redirect, null);
  assert.equal(runLanguageSwitch({ lang: "es", stored: "en", languages: ["es-MX"] }).redirect, "/");
  assert.equal(runLanguageSwitch({ lang: "es", stored: "es", languages: ["en-US"] }).redirect, null);
});

test("moving around the site never redirects, so the switch works without storage", () => {
  const referrer = `${ORIGIN}/es/#faq`;
  assert.equal(runLanguageSwitch({ lang: "en", languages: ["es"], referrer }).redirect, null);
  assert.equal(runLanguageSwitch({ lang: "en", languages: ["es"], referrer, storage: false }).redirect, null);
  assert.equal(runLanguageSwitch({ lang: "es", stored: "en", referrer: `${ORIGIN}/` }).redirect, null);
  // Another site is an arrival.
  assert.equal(runLanguageSwitch({ lang: "en", languages: ["es"], referrer: "https://www.google.com/" }).redirect, "/es/");
});

test("blocked storage still follows the browser on arrival", () => {
  assert.equal(runLanguageSwitch({ lang: "en", languages: ["es-ES"], storage: false }).redirect, "/es/");
});

test("the switch saves the chosen language and keeps the current section", () => {
  const page = runLanguageSwitch({ lang: "en", hash: "#downloads" });
  const link = page.click("es");
  assert.equal(page.saved(), "es");
  assert.equal(link.hash, "#downloads");
  // Blocked storage: following the link still works.
  assert.equal(runLanguageSwitch({ lang: "es", storage: false }).click("en").hash, "");
});

// Runs WEBGL_PROBE with a canvas whose WebGL 2 context is `context` (null when unavailable);
// returns the <html> classes and the load listener.
function runProbe(context) {
  const classes = new Set();
  const requested = [];
  let onLoad = null;
  const root = { classList: { add: (c) => classes.add(c), remove: (c) => classes.delete(c) }, dataset: {} };
  const document = {
    documentElement: root,
    createElement: () => ({
      getContext: (type, options) => {
        requested.push([type, options]);
        return context;
      },
    }),
  };
  const window = { addEventListener: (type, listener) => type === "load" && (onLoad = listener) };
  new Function("document", "window", WEBGL_PROBE)(document, window);
  return { classes, requested, root, load: () => onLoad?.() };
}

test("only WebGL 2 without a major performance caveat turns the 3D layers on", () => {
  const lost = [];
  const context = { getExtension: () => ({ loseContext: () => lost.push(true) }) };
  const on = runProbe(context);
  assert.deepEqual(on.requested, [["webgl2", { failIfMajorPerformanceCaveat: true }]]);
  assert.ok(on.classes.has("webgl"));
  assert.equal(lost.length, 1, "the probe context is released");

  // No WebGL 2, or only a slow software renderer the browser reports: no context.
  assert.equal(runProbe(null).classes.has("webgl"), false);
});

test("the static design comes back at load if the page script never ran", () => {
  const context = { getExtension: () => null };
  const failed = runProbe(context);
  failed.load();
  assert.equal(failed.classes.has("webgl"), false);

  const booted = runProbe(context);
  booted.root.dataset.booted = "true";
  booted.load();
  assert.ok(booted.classes.has("webgl"));
});

test("the built pages inline exactly the tested scripts", () => {
  // The build reprints pickLanguage's source (its behavior is checked in i18n.test.mjs), so the
  // block between its markers is left out of the comparison.
  const PICK = /\/\* BEGIN pickLanguage \*\/[\s\S]*?\/\* END pickLanguage \*\//;
  const languageSwitch = LANGUAGE_SWITCH.replace(PICK, "PICK");
  for (const path of Object.values(HOME_PAGES)) {
    const html = built(path);
    const body = html.indexOf("<body");
    assert.ok(html.indexOf(`<script>${WEBGL_PROBE}</script>`) < body, `${path}: WebGL probe in <head>`);
    const inline = html.replace(PICK, "PICK");
    const at = inline.indexOf(`<script>${languageSwitch}</script>`);
    assert.ok(at >= 0 && at < inline.indexOf("<body"), `${path}: language switch in <head>`);
  }
});
