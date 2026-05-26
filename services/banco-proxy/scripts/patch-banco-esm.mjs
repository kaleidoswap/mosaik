#!/usr/bin/env node
/**
 * Patch @arkade-os/banco's ESM dist for Node's strict ESM resolver.
 *
 * Upstream's tsc-built ESM emits extensionless relative imports
 * (`from "./offer"` instead of `from "./offer.js"`), which crash Node
 * at module-load with ERR_MODULE_NOT_FOUND. Five sites today, all in
 * `dist/esm/{index,maker,taker}.js`.
 *
 * Runs as a `postinstall` hook so every fresh `pnpm install` self-heals.
 * Idempotent — re-running on a patched tree is a no-op.
 *
 * Drop this script the day upstream publishes a release with a working
 * ESM build (track in services/banco-proxy/README.md §"Upstream caveat").
 */

import { readFileSync, writeFileSync, readdirSync, statSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

// Resolve banco's dist next to OUR node_modules — works for both flat (npm)
// and pnpm's virtual-store symlink layouts. We walk our own node_modules tree
// so the script doesn't care which package manager installed it.
const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, '..');

// Find every banco/dist/esm under node_modules (pnpm hoists multiple copies).
const targets = [];
function walk(dir) {
  let entries;
  try {
    entries = readdirSync(dir);
  } catch {
    return;
  }
  for (const name of entries) {
    const full = join(dir, name);
    let s;
    try {
      s = statSync(full);
    } catch {
      continue;
    }
    if (!s.isDirectory()) continue;
    // The match path: .../node_modules/@arkade-os/banco/dist/esm
    if (full.endsWith('/@arkade-os/banco/dist/esm')) {
      targets.push(full);
      continue;
    }
    // Avoid descending forever — only follow node_modules / .pnpm dirs.
    if (
      name === 'node_modules' ||
      name === '.pnpm' ||
      name.startsWith('@') ||
      full.includes('/node_modules/')
    ) {
      walk(full);
    }
  }
}
walk(join(root, 'node_modules'));

if (targets.length === 0) {
  // Not installed yet (e.g. running script before `pnpm install`). Silent exit.
  process.exit(0);
}

// Rewrite extensionless relative imports/exports. We only touch:
//   from "./xyz"   →   from "./xyz.js"
//   from "../xyz"  →   from "../xyz.js"
// We do NOT touch absolute imports or already-extensioned paths.
const importRe = /(from\s+["'])(\.{1,2}\/[^"']+?)(["'])/g;

let totalEdits = 0;
for (const dir of targets) {
  for (const file of readdirSync(dir)) {
    if (!file.endsWith('.js')) continue;
    const path = join(dir, file);
    const before = readFileSync(path, 'utf8');
    const after = before.replace(importRe, (_, pre, spec, post) => {
      // Leave already-extensioned specifiers alone.
      if (/\.(js|mjs|cjs|json)$/.test(spec)) return `${pre}${spec}${post}`;
      return `${pre}${spec}.js${post}`;
    });
    if (after !== before) {
      writeFileSync(path, after);
      totalEdits++;
    }
  }
}

if (totalEdits > 0) {
  console.log(
    `patch-banco-esm: rewrote extensionless imports in ${totalEdits} file(s) across ${targets.length} banco dist tree(s)`,
  );
}
