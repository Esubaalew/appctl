import { visit } from 'unist-util-visit';

/**
 * Remark plugin: prefix every absolute `/docs/...` link with the Astro `base`
 * so content links keep working when the site is served under a project-site
 * path (e.g. `/appctl/` on GitHub Pages).
 *
 * Runs on both Markdown links and MDX JSX `href` attributes.
 */
export default function remarkPrefixBase({ base = '/' } = {}) {
  const normalized = base.endsWith('/') ? base : `${base}/`;

  const rewrite = (url) => {
    if (typeof url !== 'string') return url;
    if (!url.startsWith('/docs/')) return url;
    // Don't double-prefix if base is already present.
    if (normalized !== '/' && url.startsWith(normalized)) return url;
    return `${normalized.replace(/\/$/, '')}${url}`;
  };

  return (tree) => {
    visit(tree, 'link', (node) => {
      node.url = rewrite(node.url);
    });
    visit(tree, 'definition', (node) => {
      node.url = rewrite(node.url);
    });
  };
}
