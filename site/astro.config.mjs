// The BandEar website: a static build (dist/) that GitHub Pages serves at the domain root.
// See README.md.
import { defineConfig } from "astro/config";

export default defineConfig({
  // Canonical, hreflang, Open Graph, sitemap and robots.txt URLs are built from this.
  site: "https://bandear.com",
  build: {
    // /es/ is es/index.html, like every folder GitHub Pages serves.
    format: "directory",
    // One small stylesheet: inlining it saves a render-blocking request.
    inlineStylesheets: "always",
  },
  devToolbar: { enabled: false },
  vite: {
    build: {
      // The minified bundles drop license comments; this file carries the notices of the
      // libraries they include (three.js, MIT).
      license: { fileName: "third-party-licenses.md" },
      // three.js is one lazy chunk of about 580 kB (145 kB gzipped), loaded only for the 3D scenes.
      chunkSizeWarningLimit: 650,
    },
  },
});
