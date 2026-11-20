# Site: Astro migration and SEO

Objective: move the website in `site/` (https://bandear.com) from plain HTML and ES modules to an
Astro static build, and improve its SEO with measured evidence.
Branch: `feat/site-astro-seo` (worktree `bandear-worktrees/site-astro`). Delivery: one PR, merged
only after the user's word (merging deploys the live site). Test runner: `node --test`.

## Specs

- S1. Astro project, static output, that reproduces the current site exactly: every section, the
  English/Spanish copy and the language toggle (`site/js/i18n.js`); `404.html`; the footer
  "A project by" / "Un proyecto de" with the Brizons logo.
- S2. The download buttons still read the latest GitHub release and match files by suffix
  (`_x64.zip`, `_aarch64.dmg`, `_x86_64.dmg`, `_amd64.tar.gz`, `_android.apk`, repo
  `brizons/bandear`); every link falls back to the releases page.
- S3. The three.js scenes with their static fallback.
- S4. SEO improvements that are worth it and backed by evidence: real per-language pages
  (`/` and `/es/`) with `hreflang` instead of a client-side language swap; canonical URLs, Open
  Graph and Twitter tags; `sitemap.xml` and `robots.txt`; JSON-LD (SoftwareApplication, and
  Organization for Brizons); a sensible title, description and headings hierarchy; image sizes,
  alt text and performance. Explain each change and measure before and after (Lighthouse).
- S5. The existing site tests (`node --test site/tests/`, 54 tests) are ported or kept, so the same
  rules stay covered.
- S6. `.github/workflows/pages.yml` builds Astro and deploys to GitHub Pages with the custom domain
  `bandear.com` still working.
- S7. The site README is updated.
- S8. Verify locally first (a preview build, desktop and 375 px mobile, no horizontal scroll), open
  a PR, and ask the user before merging.

## Tasks

| ID | Specs | Route | Status | Commit |
|----|-------|-------|--------|--------|
| T1 | S1, S3 | inline | pending | |
| T2 | S1, S2, S3 | inline | pending | |
| T3 | S4 | inline | pending | |
| T4 | S4 | inline | pending | |
| T5 | S5 | inline | pending | |
| T6 | S6, S7 | inline | pending | |
| T7 | S8 | inline | pending | |

- T1: move the static files under `site/public/`, the scripts under `site/src/scripts/`, add the
  Astro project (package.json, config).
- T2: page components rendered at build time per language; client scripts (downloads, scenes,
  language switch) bundled by Astro; three.js from npm at the vendored version.
- T3: head tags (canonical, hreflang, Open Graph, Twitter), JSON-LD, `sitemap.xml`, `robots.txt`.
- T4: responsive WebP/AVIF screenshots, WOFF2 fonts, inlined CSS; Lighthouse before and after.
- T5: tests on the source and the built pages.
- T6: Pages workflow (build, test, deploy; build check on PRs) and READMEs.
- T7: preview at desktop and 375 px, PR.

Route evidence: one understood change on one page; no Writer trigger reason (no parallel units),
so every task runs inline.

## Log

- L1 (2026-11-19, user, relayed by the parent session): "que migres la web a astro y mejores el ceo"
  (SEO).
- L2 (2026-11-19): baseline 54 tests pass on 2ff491a. Astro 7.4.11 chosen (latest is 7.4.13 from
  2026-11-18; a week-old patch as a supply-chain cooldown). three 0.186.1 from npm, the vendored
  version.
- L3 (2026-11-19): evidence for SEO choices, Google Search Central:
  - Avoid automatic redirects between language versions based on a guess, and link the versions
    for the user to choose
    (https://developers.google.com/search/docs/specialty/international/managing-multi-regional-sites).
  - Use `x-default` for the fallback page, and reciprocal hreflang links
    (https://developers.google.com/search/docs/specialty/international/localized-versions).
  - Software app rich results need `offers.price` plus `aggregateRating` or `review`
    (https://developers.google.com/search/docs/appearance/structured-data/software-app). BandEar
    has no ratings, so the JSON-LD describes the app without a rich-result claim.
  - Site names come from WebSite structured data on the home page
    (https://developers.google.com/search/docs/appearance/site-names).
- L4 (2026-11-19): decision pending the user's confirmation at the PR: the current site follows a
  Spanish browser on the first visit. Kept on `/` only (the x-default page), only on entry to the
  site (no same-origin referrer), and a saved toggle choice still wins. Google advises against
  guess-based redirects; dropping it is a one-line change if the user prefers.
