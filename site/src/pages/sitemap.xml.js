// sitemap.xml: one entry per language version, each listing all versions (hreflang) as Google
// documents for sitemaps. No <lastmod>: the build date is not when the content changed.
import { LANGUAGES, PATHS } from "../i18n.js";

export function GET({ site }) {
  const url = (lang) => new URL(PATHS[lang], site).href;
  const alternates = [
    ...LANGUAGES.map((lang) => `    <xhtml:link rel="alternate" hreflang="${lang}" href="${url(lang)}"/>`),
    `    <xhtml:link rel="alternate" hreflang="x-default" href="${url("en")}"/>`,
  ].join("\n");
  const entries = LANGUAGES.map((lang) => `  <url>\n    <loc>${url(lang)}</loc>\n${alternates}\n  </url>`).join("\n");
  const body = `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9" xmlns:xhtml="http://www.w3.org/1999/xhtml">
${entries}
</urlset>
`;
  return new Response(body, { headers: { "Content-Type": "application/xml; charset=utf-8" } });
}
