#!/usr/bin/env node
// Build `public/og.png` (1200x630) from `src/assets/og-template.svg`.
// Deterministic: same SVG input produces byte-identical PNG (sharp is stable).
import { readFile, writeFile, mkdir, stat } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import sharp from 'sharp';

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, '..');
const svgPath = resolve(repoRoot, 'src/assets/og-template.svg');
const outPath = resolve(repoRoot, 'public/og.png');

async function main() {
  try {
    await stat(svgPath);
  } catch {
    console.warn(`[og] ${svgPath} not found, skipping.`);
    return;
  }
  const svg = await readFile(svgPath);
  await mkdir(dirname(outPath), { recursive: true });
  await sharp(svg, { density: 144 })
    .resize(1200, 630, { fit: 'cover' })
    .png({ compressionLevel: 9 })
    .toFile(outPath);
  console.log(`[og] wrote ${outPath}`);
}

main().catch((err) => {
  console.error('[og] failed:', err);
  process.exitCode = 1;
});
