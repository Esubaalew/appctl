import { readFileSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';

/**
 * Read the workspace version from the root Cargo.toml. Walks up from the
 * current working directory so the lookup works both when called from the
 * Astro config (project root) and when inlined into a prerendered module
 * (runs from a nested chunk directory).
 */
export function readWorkspaceVersion() {
  const candidates = [
    resolve(process.cwd(), '../Cargo.toml'),
    resolve(process.cwd(), 'Cargo.toml'),
    resolve(process.cwd(), '../../Cargo.toml'),
  ];
  const cargoToml = candidates.find((c) => existsSync(c));
  if (!cargoToml) {
    throw new Error(
      `Could not find Cargo.toml from ${process.cwd()}. Docs rely on the ` +
        'workspace Cargo.toml being one level above the docs-next/ directory.'
    );
  }
  const text = readFileSync(cargoToml, 'utf8');
  const match = text.match(/^\s*version\s*=\s*"([^"]+)"/m);
  if (!match) {
    throw new Error(
      `Could not find \`version = "..."\` in ${cargoToml}. ` +
        'Docs rely on the workspace version being declared there.'
    );
  }
  return match[1];
}

export const APPCTL_VERSION = readWorkspaceVersion();
