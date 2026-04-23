import { visit } from 'unist-util-visit';

/**
 * Remark plugin: replace `{{appctl_version}}` occurrences in markdown text,
 * inline code, and fenced code blocks with the workspace version read from
 * Cargo.toml at build time. Prevents docs from drifting out of sync with
 * released versions.
 */
export default function remarkAppctlVersion({ version }) {
  if (!version) {
    throw new Error(
      'remark-appctl-version requires { version } — pass the string from Cargo.toml.'
    );
  }
  const token = /\{\{\s*appctl_version\s*\}\}/g;
  const replace = (value) =>
    typeof value === 'string' ? value.replace(token, version) : value;

  return (tree) => {
    visit(tree, (node) => {
      if (node.type === 'text' || node.type === 'inlineCode' || node.type === 'code') {
        node.value = replace(node.value);
      }
    });
  };
}
