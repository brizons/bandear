// Tests for the website copy: both languages cover the same keys, the components only name keys
// the dictionary has, each built page carries its own language's copy, and every page picks a
// visitor's language the same way.
// Run: npm run build && npm test (in site/)

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import {
  CLIENT_KEYS,
  DICTIONARY,
  LANGUAGES,
  clientStrings,
  keysInSource,
  pickLanguage,
  translate,
  translator,
} from "../src/i18n.js";
import { HOME_PAGES, astroSources, built, escapeHtml } from "./built.mjs";

const sceneFlow = readFileSync(new URL("../src/scripts/scene-flow.js", import.meta.url), "utf8");
const componentKeys = new Set(astroSources().flatMap(({ source }) => [...keysInSource(source)]));

test("English and Spanish define exactly the same keys", () => {
  const en = Object.keys(DICTIONARY.en).sort();
  for (const lang of LANGUAGES) {
    assert.deepEqual(Object.keys(DICTIONARY[lang]).sort(), en, `${lang} keys differ from en`);
  }
});

test("no string is empty in either language", () => {
  for (const lang of LANGUAGES) {
    for (const [key, value] of Object.entries(DICTIONARY[lang])) {
      assert.ok(typeof value === "string" && value.trim().length > 0, `${lang}.${key} is empty`);
    }
  }
});

test("every key the components name exists in both languages", () => {
  assert.ok(componentKeys.size > 80, `expected the page to be translated, found ${componentKeys.size} keys`);
  for (const key of componentKeys) {
    for (const lang of LANGUAGES) assert.ok(key in DICTIONARY[lang], `${key} is missing from ${lang}`);
  }
});

test("the 3D scene's labels are strings the page embeds", () => {
  const block = sceneFlow.match(/const LABELS = \[([^\]]+)\]/);
  assert.ok(block, "LABELS not found in scene-flow.js");
  const keys = [...block[1].matchAll(/"([^"]+)"/g)].map((match) => match[1]);
  assert.equal(keys.length, 5);
  for (const key of keys) {
    assert.ok(CLIENT_KEYS.includes(key), `${key} is not in CLIENT_KEYS`);
    assert.ok(key in DICTIONARY.en && key in DICTIONARY.es, `${key} is missing`);
  }
});

test("each built page carries its own language's copy", () => {
  for (const [lang, path] of Object.entries(HOME_PAGES)) {
    const html = built(path);
    assert.match(html, new RegExp(`<html lang="${lang}"`), `${path} is not marked ${lang}`);
    for (const key of componentKeys) {
      const value = DICTIONARY[lang][key];
      // Plain strings are escaped; markup strings (set:html) appear as written.
      assert.ok(html.includes(escapeHtml(value)) || html.includes(value), `${path} lacks ${lang}.${key}`);
    }
  }
});

test("each built page embeds the run-time strings in its language", () => {
  for (const [lang, path] of Object.entries(HOME_PAGES)) {
    const json = built(path).match(/<script type="application\/json" id="strings">([^<]*)<\/script>/);
    assert.ok(json, `${path} has no strings block`);
    assert.deepEqual(JSON.parse(json[1]), clientStrings(lang), path);
  }
});

test("pickLanguage prefers a stored choice, then a Spanish browser", () => {
  assert.equal(pickLanguage("es", ["en-US"]), "es");
  assert.equal(pickLanguage("en", ["es-MX"]), "en");
  assert.equal(pickLanguage(null, ["es-419", "en"]), "es");
  assert.equal(pickLanguage(null, ["ES"]), "es");
  assert.equal(pickLanguage(null, ["en-GB", "es"]), "en");
  assert.equal(pickLanguage("fr", ["de-DE"]), "en");
  assert.equal(pickLanguage(null, []), "en");
  assert.equal(pickLanguage(null, [undefined]), "en");
  assert.equal(pickLanguage(null, undefined), "en");
});

test("every built page picks the language with pickLanguage itself", () => {
  const cases = [
    ["es", ["en-US"]],
    ["en", ["es-MX"]],
    [null, ["es-419", "en"]],
    [null, ["ES"]],
    [null, ["en-GB", "es"]],
    [null, ["", "es-AR"]],
    [null, [undefined, "es"]],
    ["fr", ["de-DE"]],
    [null, []],
    [null, [undefined]],
  ];
  for (const path of [...Object.values(HOME_PAGES), "404.html"]) {
    const source = built(path).match(/\/\* BEGIN pickLanguage \*\/([\s\S]*?)\/\* END pickLanguage \*\//);
    assert.ok(source, `pickLanguage not found in ${path}`);
    const inline = new Function(`${source[1]}; return pickLanguage;`)();
    for (const [stored, languages] of cases) {
      assert.equal(inline(stored, languages), pickLanguage(stored, languages), `${path}: ${stored} ${JSON.stringify(languages)}`);
    }
  }
});

test("translate fills placeholders and falls back to English, then the key", () => {
  assert.equal(translate("es", "dl.hero.for", { os: "Linux" }), "Descargar para Linux");
  assert.equal(translate("en", "dl.hero.for", { os: "Linux" }), "Download for Linux");
  assert.equal(translate("en", "dl.hero.for"), "Download for {os}");
  assert.equal(translate("xx", "nav.faq"), "FAQ");
  assert.equal(translate("es", "no.such.key"), "no.such.key");
});

test("the components' translator fails on a missing key or language instead of falling back", () => {
  assert.equal(translator("es")("nav.faq"), "Preguntas");
  assert.equal(translator("en")("dl.hero.for", { os: "Linux" }), "Download for Linux");
  assert.throws(() => translator("es")("no.such.key"), /no\.such\.key/);
  assert.throws(() => translator("xx"), /unknown language/);
});

test("keysInSource finds t() calls in component source", () => {
  assert.deepEqual([...keysInSource(`<p>{t("a.b")}</p><img alt={t('c')} /><p>{tt("x")}</p>`)], ["a.b", "c"]);
  assert.deepEqual([...keysInSource("")], []);
});
