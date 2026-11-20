# BandEar website

The website published at **https://bandear.com/**, built with [Astro](https://astro.build/) as
static files: one page per language (`/` in English, `/es/` in Spanish), no server code and no
external requests except the GitHub API call that finds the latest release.

| Path | What it is |
|------|------------|
| `src/pages/` | `index.astro` (English, also the `x-default` page), `es/index.astro`, `404.astro`, `sitemap.xml.js` and `robots.txt.js`. |
| `src/components/` | The home page's sections (`Home.astro` puts them together) and its structured data. |
| `src/layouts/Page.astro` | The document shell: title, description, canonical and `hreflang` links, Open Graph and Twitter cards, the scripts that run before the first paint. |
| `src/i18n.js` | English and Spanish copy, used at build time. |
| `src/inline-scripts.js` | The WebGL probe and the language switch the pages inline in `<head>`. |
| `src/scripts/` | Browser scripts: the download buttons (`downloads.js`, `links.js`), the decorative three.js scenes (`main.js`, `scene-*.js`, `model-mixer.js`, `three-setup.js`). |
| `src/styles/style.css` | The stylesheet (inlined into each page by the build). |
| `src/assets/screenshots/` | App screenshots; the build turns them into responsive WebP sizes. |
| `public/` | Files served as they are: brand marks, the social card (`og.png`) and the self-hosted fonts (WOFF2). |
| `dev/` | A preview server that behaves like GitHub Pages, and the social-card source. Not deployed. |
| `tests/` | Node tests for the scripts and the built pages. Not deployed. |

## Build and preview

Node 24 (any version Astro accepts: 22.23.4 or newer). From `site/`:

```sh
npm ci            # install the locked dependencies
npm run dev       # live preview at http://localhost:4321/
npm run build     # the static site in dist/
npm run preview   # serve dist/ like GitHub Pages at http://127.0.0.1:5300/
npm test          # node tests; most of them read dist/, so build first
```

`npm run preview` mirrors GitHub Pages: `404.html` for missing paths and a redirect from a folder
to its trailing slash (`/es` → `/es/`). The build assumes the site is served at the domain root.

## Languages

`src/i18n.js` holds one dictionary per language. Components translate with `t("key")` from
`translator(lang)`, which fails the build if a key is missing in that language; text is escaped,
and keys holding markup (`<em>`, `<a>`…) are inserted with `set:html`. The tests fail if the two
languages define different keys, if a component names a key the dictionary lacks, or if a built
page lacks any of its own language's strings.

The language switch in the header is a link to the other language's page. An inline script
(`src/inline-scripts.js`) saves the choice in `localStorage` and keeps the current section. On
arrival from another site, a saved choice wins; without one, `/` sends a Spanish browser (any
`es*` language) to `/es/`. Moving around the site never redirects, and crawlers, which arrive
with no saved choice and an English browser, see each page at its own address.

The few strings the scripts write at run time (the download button, the 3D diagram's labels;
`CLIENT_KEYS`) travel inside each page as JSON, so the browser never downloads the dictionary.

## Downloads

`src/scripts/downloads.js` asks `api.github.com/repos/brizons/bandear/releases/latest` (cached for
30 minutes; the anonymous limit is 60 requests per hour), matches the downloads by file-name
suffix (`_x64.zip`, `_aarch64.dmg`, `_x86_64.dmg`, `_amd64.tar.gz`, `_android.apk`, the names
`apps/desktop/app/package.sh` and the release workflow give them) and offers the visitor's system
first. Keep those suffixes and the release files' names in step. Every download link in the
built pages opens the latest release page, so an API failure only loses the direct links; until
the API answers, the cards name the files of `FALLBACK_VERSION` (`src/scripts/links.js`), which
is worth bumping with each release.

## 3D scenes

The decorative three.js scenes (three.js comes from npm, pinned in `package.json`, and is loaded
only when a scene is about to show) start only when the browser offers a hardware-accelerated
WebGL 2: the probe in `<head>` asks for a context with `failIfMajorPerformanceCaveat`, so a
software renderer (no GPU) keeps the static design instead of drawing on the CPU. Each figure
keeps its static design until its scene has drawn a first frame, and goes back to it for good if
the scene fails. The build writes the licenses of the bundled libraries to
`third-party-licenses.md`.

## Search engines and link previews

- Each page has its own title and description (kept under about 155 characters, the width of a
  search result snippet), a canonical URL and `hreflang` links to both languages plus
  `x-default` (`/`), which the sitemap repeats.
- Open Graph and Twitter cards per language, with `public/assets/og.png`.
- JSON-LD: `WebSite` (Google takes the site name from it), `Organization` for Brizons and
  `SoftwareApplication` for BandEar (free, MIT). There are no ratings or reviews, so it makes no
  claim to a software-app rich result.
- `sitemap.xml` lists both pages; `robots.txt` allows everything and points at it; `404.html` is
  `noindex`.

## Deployment

`.github/workflows/pages.yml` installs the locked dependencies, builds `dist/` and runs the tests
on every pull request and every push to `main` that touches `site/**`. On `main` (or by hand from
the Actions tab) it publishes `dist/` to GitHub Pages.

One-time setup: **Settings → Pages → Build and deployment → Source: GitHub Actions**.

## Custom domain

The site is served at `bandear.com` (registered at Hostinger). An Actions-deployed Pages site
ignores a `CNAME` file: the domain lives in **Settings → Pages → Custom domain**.

- DNS at the registrar:
  - apex: four `A` records to `185.199.108.153`, `185.199.109.153`, `185.199.110.153`,
    `185.199.111.153`, and four `AAAA` records to `2606:50c0:8000::153`, `2606:50c0:8001::153`,
    `2606:50c0:8002::153`, `2606:50c0:8003::153`;
  - `www`: a `CNAME` record to `brizons.github.io`.
- Tick **Enforce HTTPS** once GitHub has issued the certificate.
- If the domain ever changes, update `site` in `astro.config.mjs` (canonical, `hreflang`, Open
  Graph, sitemap and robots URLs follow it) and the root `README.md` link.
- The download links stay on github.com. Only if the repository itself is renamed or moved,
  update `src/scripts/links.js` (the tests check the built pages against it).

## Social card

`public/assets/og.png` (1200×630) and `public/assets/brand/apple-touch-icon.png` (180×180) are
screenshots of `dev/og-card.html`; the comment at the top of that file explains how to
regenerate them.
