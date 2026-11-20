// Tests for what search engines and link previews read from the built pages: one page per
// language with reciprocal hreflang links, canonical and social tags, structured data, a heading
// outline without gaps, image sizes and alt texts, sitemap.xml and robots.txt.
// Run: npm run build && npm test (in site/)

import { test } from "node:test";
import assert from "node:assert/strict";

import { DICTIONARY, LANGUAGES, OG_LOCALES, PATHS } from "../src/i18n.js";
import { HOME_PAGES, built, tags } from "./built.mjs";

const SITE = "https://bandear.com";
const url = (lang) => `${SITE}${PATHS[lang]}`;
const meta = (html, key, value) => tags(html, "meta").filter((m) => m[key] === value).map((m) => m.content);
const links = (html, rel) => tags(html, "link").filter((l) => l.rel === rel);

test("each page has its own title, description and canonical URL", () => {
  for (const [lang, path] of Object.entries(HOME_PAGES)) {
    const html = built(path);
    const title = html.match(/<title>([^<]*)<\/title>/)[1].replace(/&#39;/g, "'");
    assert.equal(title, DICTIONARY[lang]["meta.title"], path);
    assert.deepEqual(meta(html, "name", "description"), [DICTIONARY[lang]["meta.description"]], path);
    assert.deepEqual(links(html, "canonical").map((l) => l.href), [url(lang)], path);
  }
});

test("descriptions fit a search result snippet", () => {
  for (const lang of LANGUAGES) {
    const length = DICTIONARY[lang]["meta.description"].length;
    assert.ok(length >= 70 && length <= 160, `${lang} description is ${length} characters`);
  }
});

test("every page links every language version and x-default, the same way", () => {
  const expected = [...LANGUAGES.map((lang) => [lang, url(lang)]), ["x-default", url("en")]];
  for (const path of Object.values(HOME_PAGES)) {
    const alternates = links(built(path), "alternate").map((l) => [l.hreflang, l.href]);
    assert.deepEqual(alternates, expected, path);
  }
});

test("the language switch links the other language's page", () => {
  for (const [lang, path] of Object.entries(HOME_PAGES)) {
    const switches = tags(built(path), "a").filter((a) => "data-lang-switch" in a);
    assert.equal(switches.length, 1, path);
    const other = LANGUAGES.find((candidate) => candidate !== lang);
    assert.equal(switches[0].href, PATHS[other], path);
    assert.equal(switches[0].hreflang, other, path);
    assert.equal(switches[0].lang, other, path);
  }
});

test("Open Graph and Twitter cards describe each page in its language", () => {
  for (const [lang, path] of Object.entries(HOME_PAGES)) {
    const html = built(path);
    const og = (property) => meta(html, "property", property);
    const twitter = (name) => meta(html, "name", name);
    assert.deepEqual(og("og:url"), [url(lang)], path);
    assert.deepEqual(og("og:title"), [DICTIONARY[lang]["meta.title"]], path);
    assert.deepEqual(og("og:description"), [DICTIONARY[lang]["meta.description"]], path);
    assert.deepEqual(og("og:image"), [`${SITE}/assets/og.png`], path);
    assert.deepEqual(og("og:image:alt"), [DICTIONARY[lang]["meta.ogImageAlt"]], path);
    assert.deepEqual(og("og:locale"), [OG_LOCALES[lang]], path);
    assert.deepEqual(
      og("og:locale:alternate"),
      LANGUAGES.filter((other) => other !== lang).map((other) => OG_LOCALES[other]),
      path,
    );
    assert.deepEqual(twitter("twitter:card"), ["summary_large_image"], path);
    assert.deepEqual(twitter("twitter:title"), [DICTIONARY[lang]["meta.title"]], path);
    assert.deepEqual(twitter("twitter:description"), [DICTIONARY[lang]["meta.description"]], path);
    assert.deepEqual(twitter("twitter:image"), [`${SITE}/assets/og.png`], path);
  }
});

test("structured data names the site, Brizons and BandEar as a free app", () => {
  for (const [lang, path] of Object.entries(HOME_PAGES)) {
    const blocks = [...built(path).matchAll(/<script type="application\/ld\+json">([^<]*)<\/script>/g)];
    assert.equal(blocks.length, 1, path);
    const graph = JSON.parse(blocks[0][1])["@graph"];
    const byType = Object.fromEntries(graph.map((node) => [node["@type"], node]));
    assert.equal(byType.WebSite.name, "BandEar", path);
    assert.equal(byType.WebSite.url, `${SITE}/`, path);
    assert.equal(byType.Organization.name, "Brizons", path);
    assert.equal(byType.Organization.url, "https://brizons.com/", path);
    const app = byType.SoftwareApplication;
    assert.equal(app.name, "BandEar", path);
    assert.equal(app.url, url(lang), path);
    assert.equal(app.description, DICTIONARY[lang]["meta.description"], path);
    assert.deepEqual(app.offers, { "@type": "Offer", price: "0", priceCurrency: "USD" }, path);
    assert.deepEqual(app.publisher, { "@id": byType.Organization["@id"] }, path);
    assert.deepEqual(byType.WebSite.publisher, { "@id": byType.Organization["@id"] }, path);
    // No invented ratings: BandEar has none.
    assert.equal(app.aggregateRating, undefined, path);
    assert.equal(app.review, undefined, path);
  }
});

test("each page has one h1 and its headings never skip a level", () => {
  for (const path of Object.values(HOME_PAGES)) {
    const levels = [...built(path).matchAll(/<h([1-6])\b/g)].map((match) => Number(match[1]));
    assert.equal(levels.filter((level) => level === 1).length, 1, `${path}: h1 count`);
    assert.equal(levels[0], 1, `${path}: the first heading is not the h1`);
    for (let i = 1; i < levels.length; i++) {
      assert.ok(levels[i] <= levels[i - 1] + 1, `${path}: h${levels[i - 1]} is followed by h${levels[i]}`);
    }
  }
});

test("every image has an alt text and its size, so nothing shifts while it loads", () => {
  for (const path of Object.values(HOME_PAGES)) {
    const images = tags(built(path), "img");
    assert.ok(images.length >= 9, `${path}: ${images.length} images`);
    for (const image of images) {
      assert.ok("alt" in image, `${path}: ${image.src} has no alt`);
      assert.match(image.width ?? "", /^\d+$/, `${path}: ${image.src} has no width`);
      assert.match(image.height ?? "", /^\d+$/, `${path}: ${image.src} has no height`);
    }
    // The screenshots come in responsive WebP sizes; only the hero one loads eagerly.
    const shots = images.filter((image) => image.src.startsWith("/_astro/"));
    assert.equal(shots.length, 5, path);
    for (const shot of shots) {
      assert.match(shot.src, /\.webp$/, path);
      assert.match(shot.srcset ?? "", /\.webp 480w, .*\.webp 1440w$/, path);
      assert.ok(shot.sizes, path);
      assert.ok(shot.alt.length > 20, `${path}: ${shot.src} alt is too short`);
    }
    assert.deepEqual(
      shots.map((shot) => shot.loading),
      ["eager", "lazy", "lazy", "lazy", "lazy"],
      path,
    );
    assert.equal(shots[0].fetchpriority, "high", path);
  }
});

test("sitemap.xml lists every language page with its alternates, and nothing else", () => {
  const xml = built("sitemap.xml");
  const locs = [...xml.matchAll(/<loc>([^<]+)<\/loc>/g)].map((match) => match[1]);
  assert.deepEqual(locs, LANGUAGES.map(url));
  const alternates = [...xml.matchAll(/<xhtml:link rel="alternate" hreflang="([^"]+)" href="([^"]+)"\/>/g)];
  assert.equal(alternates.length, (LANGUAGES.length + 1) * LANGUAGES.length);
  assert.match(xml, /^<\?xml version="1\.0" encoding="UTF-8"\?>/);
  assert.match(xml, /xmlns:xhtml="http:\/\/www\.w3\.org\/1999\/xhtml"/);
});

test("robots.txt allows crawling and points at the sitemap", () => {
  const robots = built("robots.txt");
  assert.match(robots, /^User-agent: \*$/m);
  assert.match(robots, /^Allow: \/$/m);
  assert.doesNotMatch(robots, /^Disallow: \/\s*$/m);
  assert.match(robots, new RegExp(`^Sitemap: ${SITE}/sitemap\\.xml$`, "m"));
});

test("the 404 page is kept out of search results", () => {
  const html = built("404.html");
  assert.deepEqual(meta(html, "name", "robots"), ["noindex"]);
  assert.doesNotMatch(built("sitemap.xml"), /404/);
});
