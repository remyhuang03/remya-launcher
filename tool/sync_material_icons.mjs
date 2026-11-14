#!/usr/bin/env node
/**
 * Synchronize Material Icon Theme assets, normalized manifest, and static
 * icon seeds into Flutter assets.
 *
 * Usage:
 *   node tool/sync_material_icons.mjs
 *   node tool/sync_material_icons.mjs --check
 */

import { createHash } from 'node:crypto';
import fs from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  Cam16,
  Hct,
  argbFromRgb,
} from '@material/material-color-utilities';
import { Resvg } from '@resvg/resvg-js';
import { generateManifest } from 'material-icon-theme';

const require = createRequire(import.meta.url);
const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const OUTPUT_ROOT = path.join(REPO_ROOT, 'assets', 'material_icon_theme');
const ICONS_DIR = path.join(OUTPUT_ROOT, 'icons');
const OVERRIDES_PATH = path.join(
  REPO_ROOT,
  'tool',
  'material_icon_seed_overrides.json',
);
const PUBSPEC_PATH = path.join(REPO_ROOT, 'pubspec.yaml');

const MANIFEST_SCHEMA_VERSION = 1;
const SEEDS_SCHEMA_VERSION = 1;
const METADATA_SCHEMA_VERSION = 1;
const SEED_EXTRACTOR_VERSION = 1;

const RASTER_SIZE = 96;
const MIN_ALPHA = 0.2;
const MIN_ALPHA_BYTE = Math.round(MIN_ALPHA * 255);
const MIN_MEANINGFUL_CHROMA = 12;
const MIN_CANDIDATE_SHARE = 0.1;
const CLUSTER_DISTANCE_UCS = 14;
const BOUNDED_CHROMA_WEIGHT = 0.35;
const MAX_CHROMA_FOR_PREFERENCE = 60;

const MANIFEST_CONFIG = {
  activeIconPack: 'angular',
  folders: { theme: 'specific' },
  hidesExplorerArrows: false,
};

function compareStrings(a, b) {
  if (a < b) return -1;
  if (a > b) return 1;
  return 0;
}

function sortRecord(record) {
  const sorted = {};
  for (const key of Object.keys(record).sort(compareStrings)) {
    sorted[key] = record[key];
  }
  return sorted;
}

function sha256Hex(bufferOrString) {
  return createHash('sha256').update(bufferOrString).digest('hex');
}

function argbToHex(argb) {
  return `#${(argb & 0xffffff).toString(16).padStart(6, '0').toUpperCase()}`;
}

function normalizeMapping(rawMap, contextName, validIds) {
  if (!rawMap) return {};
  const out = {};
  for (const [rawKey, iconId] of Object.entries(rawMap)) {
    if (typeof iconId !== 'string' || iconId.length === 0) {
      throw new Error(`Invalid icon ID in ${contextName} for key "${rawKey}"`);
    }
    if (!validIds.has(iconId)) {
      throw new Error(
        `Unresolved icon ID "${iconId}" in ${contextName} for key "${rawKey}"`,
      );
    }
    const key = rawKey.toLowerCase();
    if (Object.hasOwn(out, key) && out[key] !== iconId) {
      throw new Error(
        `Lowercase collision in ${contextName} for "${key}": "${out[key]}" vs "${iconId}"`,
      );
    }
    out[key] = iconId;
  }
  return sortRecord(out);
}

function normalizeScope(rawScope, scopeName, validIds) {
  if (!rawScope) return undefined;
  const scope = {};
  const defaults = {};
  for (const field of [
    'file',
    'folder',
    'folderExpanded',
    'rootFolder',
    'rootFolderExpanded',
  ]) {
    if (rawScope[field] !== undefined) {
      const iconId = rawScope[field];
      if (!validIds.has(iconId)) {
        throw new Error(
          `Unresolved default icon "${iconId}" in ${scopeName}.${field}`,
        );
      }
      defaults[field] = iconId;
    }
  }
  if (Object.keys(defaults).length > 0) {
    scope.defaults = sortRecord(defaults);
  }

  const mapFields = [
    ['fileNames', 'filesByName'],
    ['fileExtensions', 'filesByExtension'],
    ['folderNames', 'foldersByName'],
    ['folderNamesExpanded', 'foldersByNameExpanded'],
    ['rootFolderNames', 'rootFoldersByName'],
    ['rootFolderNamesExpanded', 'rootFoldersByNameExpanded'],
  ];
  for (const [upstreamField, targetField] of mapFields) {
    const normalized = normalizeMapping(
      rawScope[upstreamField],
      `${scopeName}.${upstreamField}`,
      validIds,
    );
    if (Object.keys(normalized).length > 0) {
      scope[targetField] = normalized;
    }
  }
  return Object.keys(scope).length > 0 ? scope : undefined;
}

function collectUsedIds(normalizedManifest) {
  const used = new Set();
  const visitScope = (scope) => {
    if (!scope) return;
    if (scope.defaults) {
      for (const id of Object.values(scope.defaults)) used.add(id);
    }
    for (const field of [
      'filesByName',
      'filesByExtension',
      'foldersByName',
      'foldersByNameExpanded',
      'rootFoldersByName',
      'rootFoldersByNameExpanded',
    ]) {
      if (scope[field]) {
        for (const id of Object.values(scope[field])) used.add(id);
      }
    }
  };
  visitScope(normalizedManifest);
  visitScope(normalizedManifest.light);
  visitScope(normalizedManifest.highContrast);
  return [...used].sort(compareStrings);
}

function safeAssetFilename(iconId, usedFilenames) {
  const safeBase = /^[a-z0-9][a-z0-9._-]*$/.test(iconId)
    ? iconId
    : `${iconId.toLowerCase().replace(/[^a-z0-9._-]+/g, '_')}-${sha256Hex(iconId).slice(0, 8)}`;
  const filename = `${safeBase}.svg`;
  const collisionKey = filename.toLowerCase();
  if (usedFilenames.has(collisionKey)) {
    const disambiguated = `${safeBase}-${sha256Hex(iconId).slice(0, 8)}.svg`;
    const disambiguatedKey = disambiguated.toLowerCase();
    if (usedFilenames.has(disambiguatedKey)) {
      throw new Error(`Asset filename collision for icon "${iconId}"`);
    }
    usedFilenames.add(disambiguatedKey);
    return disambiguated;
  }
  usedFilenames.add(collisionKey);
  return filename;
}

function resolveUpstreamSvg(pkgRootReal, rawIconPath, iconId) {
  if (typeof rawIconPath !== 'string' || rawIconPath.length === 0) {
    throw new Error(`Missing iconPath for "${iconId}"`);
  }
  const candidate = path.resolve(pkgRootReal, 'dist', rawIconPath);
  if (!candidate.startsWith(`${pkgRootReal}${path.sep}`)) {
    throw new Error(
      `Path traversal rejected for "${iconId}": "${rawIconPath}"`,
    );
  }
  if (!fs.existsSync(candidate)) {
    throw new Error(`Missing SVG asset for "${iconId}" at "${candidate}"`);
  }
  const realCandidate = fs.realpathSync(candidate);
  if (!realCandidate.startsWith(`${pkgRootReal}${path.sep}`)) {
    throw new Error(
      `Symlink traversal rejected for "${iconId}": "${realCandidate}"`,
    );
  }
  const content = fs.readFileSync(realCandidate);
  const text = content.toString('utf8');
  if (!/<svg[\s>]/i.test(text)) {
    throw new Error(`Invalid SVG content for "${iconId}"`);
  }
  if (/<script[\s>]/i.test(text)) {
    throw new Error(`Unsupported <script> element in SVG for "${iconId}"`);
  }
  return { bytes: content, sha256: sha256Hex(content) };
}

function fitModeForSvg(svgText) {
  const match = svgText.match(/viewBox=["']([^"']+)["']/i);
  if (match) {
    const parts = match[1].trim().split(/[\s,]+/).map(Number);
    if (parts.length === 4 && Number.isFinite(parts[2]) && Number.isFinite(parts[3])) {
      if (parts[3] > parts[2]) {
        return { mode: 'height', value: RASTER_SIZE };
      }
    }
  }
  return { mode: 'width', value: RASTER_SIZE };
}

function extractIconSeed(svgBytes, iconId) {
  let rendered;
  try {
    const resvg = new Resvg(svgBytes, {
      fitTo: fitModeForSvg(svgBytes.toString('utf8')),
      font: { loadSystemFonts: false },
    });
    rendered = resvg.render();
  } catch (error) {
    throw new Error(`SVG rasterization failed for "${iconId}": ${error.message}`);
  }

  const pixels = rendered.pixels;
  const colorWeights = new Map();
  const solidWeights = new Map();
  let totalVisibleWeight = 0;

  for (let i = 0; i < pixels.length; i += 4) {
    const a = pixels[i + 3];
    if (a < MIN_ALPHA_BYTE) continue;
    const weight = a / 255;
    totalVisibleWeight += weight;

    // resvg-js outputs premultiplied RGBA; unpremultiply before HCT conversion.
    const r = a === 255 ? pixels[i] : Math.min(255, Math.round((pixels[i] * 255) / a));
    const g = a === 255 ? pixels[i + 1] : Math.min(255, Math.round((pixels[i + 1] * 255) / a));
    const b = a === 255 ? pixels[i + 2] : Math.min(255, Math.round((pixels[i + 2] * 255) / a));
    const argb = argbFromRgb(r, g, b) >>> 0;

    colorWeights.set(argb, (colorWeights.get(argb) ?? 0) + weight);
    if (a >= 230) {
      solidWeights.set(argb, (solidWeights.get(argb) ?? 0) + weight);
    }
  }

  if (totalVisibleWeight === 0) return null;

  const uniqueColors = [...colorWeights.entries()]
    .map(([argb, weight]) => ({
      argb,
      weight,
      solidWeight: solidWeights.get(argb) ?? 0,
      hct: Hct.fromInt(argb),
      cam: Cam16.fromInt(argb),
    }))
    .sort((a, b) => b.weight - a.weight || a.argb - b.argb);

  const chromatic = uniqueColors.filter(
    (entry) => entry.hct.chroma >= MIN_MEANINGFUL_CHROMA,
  );
  if (chromatic.length === 0) return null;

  const clusters = [];
  for (const item of chromatic) {
    let bestCluster = null;
    let bestDistance = Infinity;
    for (const cluster of clusters) {
      const distance = Math.hypot(
        item.cam.jstar - cluster.jstar,
        item.cam.astar - cluster.astar,
        item.cam.bstar - cluster.bstar,
      );
      if (distance <= CLUSTER_DISTANCE_UCS && distance < bestDistance) {
        bestDistance = distance;
        bestCluster = cluster;
      }
    }
    if (!bestCluster) {
      clusters.push({
        jstar: item.cam.jstar,
        astar: item.cam.astar,
        bstar: item.cam.bstar,
        weight: item.weight,
        chromaSum: item.hct.chroma * item.weight,
        representative: item,
      });
    } else {
      const nextWeight = bestCluster.weight + item.weight;
      bestCluster.jstar =
        (bestCluster.jstar * bestCluster.weight + item.cam.jstar * item.weight) /
        nextWeight;
      bestCluster.astar =
        (bestCluster.astar * bestCluster.weight + item.cam.astar * item.weight) /
        nextWeight;
      bestCluster.bstar =
        (bestCluster.bstar * bestCluster.weight + item.cam.bstar * item.weight) /
        nextWeight;
      bestCluster.weight = nextWeight;
      bestCluster.chromaSum += item.hct.chroma * item.weight;
      if (
        item.solidWeight > bestCluster.representative.solidWeight ||
        (item.solidWeight === bestCluster.representative.solidWeight &&
          item.weight > bestCluster.representative.weight)
      ) {
        bestCluster.representative = item;
      }
    }
  }

  const candidates = clusters
    .map((cluster) => {
      const share = cluster.weight / totalVisibleWeight;
      const avgChroma = cluster.chromaSum / cluster.weight;
      const chromaBoost =
        1 +
        (BOUNDED_CHROMA_WEIGHT * Math.min(avgChroma, MAX_CHROMA_FOR_PREFERENCE)) /
          MAX_CHROMA_FOR_PREFERENCE;
      return {
        hex: argbToHex(cluster.representative.argb),
        share,
        score: share * chromaBoost,
      };
    })
    .filter((candidate) => candidate.share >= MIN_CANDIDATE_SHARE)
    .sort((a, b) => b.score - a.score || compareStrings(a.hex, b.hex));

  return candidates[0]?.hex ?? null;
}

function loadOverrides(validIds) {
  if (!fs.existsSync(OVERRIDES_PATH)) {
    return {};
  }
  const parsed = JSON.parse(fs.readFileSync(OVERRIDES_PATH, 'utf8'));
  if (!parsed || typeof parsed !== 'object' || typeof parsed.overrides !== 'object') {
    throw new Error(`Invalid override file format at ${OVERRIDES_PATH}`);
  }
  const overrides = {};
  for (const [iconId, entry] of Object.entries(parsed.overrides)) {
    if (!validIds.has(iconId)) {
      throw new Error(
        `Seed override references unknown or unused icon ID "${iconId}"`,
      );
    }
    const seed =
      entry !== null && typeof entry === 'object' ? entry.seed : entry;
    if (seed !== null && (typeof seed !== 'string' || !/^#[0-9A-F]{6}$/.test(seed))) {
      throw new Error(
        `Invalid override seed for "${iconId}": expected uppercase #RRGGBB or null`,
      );
    }
    overrides[iconId] = seed;
  }
  return overrides;
}

function readPackageMeta(pkgName) {
  let dir = path.dirname(require.resolve(pkgName));
  while (true) {
    const candidate = path.join(dir, 'package.json');
    if (fs.existsSync(candidate)) {
      const pkg = JSON.parse(fs.readFileSync(candidate, 'utf8'));
      if (pkg.name === pkgName) {
        return {
          rootReal: fs.realpathSync(dir),
          pkg,
        };
      }
    }
    const parent = path.dirname(dir);
    if (parent === dir) {
      throw new Error(`Could not locate package.json for "${pkgName}"`);
    }
    dir = parent;
  }
}

function ensurePubspecAssets({ checkOnly }) {
  const requiredEntries = [
    '    - assets/material_icon_theme/manifest.json',
    '    - assets/material_icon_theme/seeds.json',
    '    - assets/material_icon_theme/metadata.json',
    '    - assets/material_icon_theme/LICENSE',
    '    - assets/material_icon_theme/icons/',
  ];
  const current = fs.readFileSync(PUBSPEC_PATH, 'utf8');
  const lines = current.split(/\r?\n/);
  const missing = requiredEntries.filter((entry) => !lines.includes(entry));
  if (missing.length === 0) return false;
  if (checkOnly) {
    throw new Error(
      `pubspec.yaml is missing Material Icon Theme asset entries:\n${missing.join('\n')}`,
    );
  }
  const licenseIndex = lines.indexOf('    - assets/licenses/');
  if (licenseIndex === -1) {
    throw new Error('Could not locate "    - assets/licenses/" in pubspec.yaml');
  }
  const updatedLines = [
    ...lines.slice(0, licenseIndex + 1),
    ...missing,
    ...lines.slice(licenseIndex + 1),
  ];
  fs.writeFileSync(PUBSPEC_PATH, `${updatedLines.join('\n')}\n`.replace(/\n\n$/, '\n'));
  return true;
}

function buildGeneratedOutputs() {
  const upstream = readPackageMeta('material-icon-theme');
  const resvgPkg = readPackageMeta('@resvg/resvg-js');
  const colorPkg = readPackageMeta('@material/material-color-utilities');

  const rawManifest = generateManifest(MANIFEST_CONFIG);
  if (!rawManifest || typeof rawManifest.iconDefinitions !== 'object') {
    throw new Error('generateManifest() returned an invalid manifest');
  }
  const definedIds = new Set(Object.keys(rawManifest.iconDefinitions));
  const baseNormalized = normalizeScope(rawManifest, 'base', definedIds);
  const lightNormalized = normalizeScope(rawManifest.light, 'light', definedIds);
  const hcNormalized = normalizeScope(
    rawManifest.highContrast,
    'highContrast',
    definedIds,
  );

  const normalizedManifest = {
    schemaVersion: MANIFEST_SCHEMA_VERSION,
    defaults: baseNormalized.defaults ?? {},
    filesByName: baseNormalized.filesByName ?? {},
    filesByExtension: baseNormalized.filesByExtension ?? {},
    foldersByName: baseNormalized.foldersByName ?? {},
    foldersByNameExpanded: baseNormalized.foldersByNameExpanded ?? {},
    rootFoldersByName: baseNormalized.rootFoldersByName ?? {},
    rootFoldersByNameExpanded: baseNormalized.rootFoldersByNameExpanded ?? {},
    ...(lightNormalized ? { light: lightNormalized } : {}),
    ...(hcNormalized ? { highContrast: hcNormalized } : {}),
    assets: {},
  };

  for (const requiredDefault of ['file', 'folder', 'folderExpanded']) {
    if (!normalizedManifest.defaults[requiredDefault]) {
      throw new Error(`Missing required default icon "${requiredDefault}"`);
    }
  }

  const usedIds = collectUsedIds(normalizedManifest);
  const usedIdSet = new Set(usedIds);
  const overrides = loadOverrides(usedIdSet);

  const usedFilenames = new Set();
  const svgOutputs = new Map();
  const assetsMap = {};
  const seedsMap = {};
  const metadataIcons = {};

  for (const iconId of usedIds) {
    const def = rawManifest.iconDefinitions[iconId];
    const { bytes, sha256 } = resolveUpstreamSvg(
      upstream.rootReal,
      def?.iconPath,
      iconId,
    );
    const filename = safeAssetFilename(iconId, usedFilenames);
    const assetPath = `assets/material_icon_theme/icons/${filename}`;
    const extractedSeed = extractIconSeed(bytes, iconId);
    const overridden = Object.hasOwn(overrides, iconId);
    const finalSeed = overridden ? overrides[iconId] : extractedSeed;

    svgOutputs.set(filename, bytes);
    assetsMap[iconId] = assetPath;
    seedsMap[iconId] = finalSeed;
    metadataIcons[iconId] = {
      asset: assetPath,
      sha256,
      seed: finalSeed,
      ...(overridden ? { overridden: true } : {}),
    };
  }

  normalizedManifest.assets = sortRecord(assetsMap);

  const seedsJson = {
    schemaVersion: SEEDS_SCHEMA_VERSION,
    sourcePackage: upstream.pkg.name,
    sourceVersion: upstream.pkg.version,
    seeds: sortRecord(seedsMap),
  };

  const metadataJson = {
    schemaVersion: METADATA_SCHEMA_VERSION,
    sourcePackage: upstream.pkg.name,
    sourceVersion: upstream.pkg.version,
    manifestConfig: MANIFEST_CONFIG,
    seedExtractor: {
      version: SEED_EXTRACTOR_VERSION,
      renderer: `${resvgPkg.pkg.name}@${resvgPkg.pkg.version}`,
      colorUtilities: `${colorPkg.pkg.name}@${colorPkg.pkg.version}`,
      rasterSize: RASTER_SIZE,
      minAlpha: MIN_ALPHA,
      minMeaningfulChroma: MIN_MEANINGFUL_CHROMA,
      minCandidateShare: MIN_CANDIDATE_SHARE,
      clusterDistanceUcs: CLUSTER_DISTANCE_UCS,
    },
    counts: {
      icons: usedIds.length,
      nonNullSeeds: Object.values(seedsMap).filter((value) => value !== null).length,
      nullSeeds: Object.values(seedsMap).filter((value) => value === null).length,
      overriddenSeeds: Object.keys(overrides).length,
      filesByName: Object.keys(normalizedManifest.filesByName).length,
      filesByExtension: Object.keys(normalizedManifest.filesByExtension).length,
      foldersByName: Object.keys(normalizedManifest.foldersByName).length,
      foldersByNameExpanded: Object.keys(normalizedManifest.foldersByNameExpanded).length,
      rootFoldersByName: Object.keys(normalizedManifest.rootFoldersByName).length,
      rootFoldersByNameExpanded: Object.keys(normalizedManifest.rootFoldersByNameExpanded).length,
    },
    icons: sortRecord(metadataIcons),
  };

  const licensePath = path.join(upstream.rootReal, 'LICENSE');
  if (!fs.existsSync(licensePath)) {
    throw new Error(`Missing upstream LICENSE at ${licensePath}`);
  }
  const licenseBytes = fs.readFileSync(licensePath);

  return {
    manifestText: `${JSON.stringify(normalizedManifest, null, 2)}\n`,
    seedsText: `${JSON.stringify(seedsJson, null, 2)}\n`,
    metadataText: `${JSON.stringify(metadataJson, null, 2)}\n`,
    licenseBytes,
    svgOutputs,
    counts: metadataJson.counts,
    sourceVersion: upstream.pkg.version,
  };
}

function checkOutputs(generated) {
  const expectedFiles = new Map([
    ['manifest.json', Buffer.from(generated.manifestText, 'utf8')],
    ['seeds.json', Buffer.from(generated.seedsText, 'utf8')],
    ['metadata.json', Buffer.from(generated.metadataText, 'utf8')],
    ['LICENSE', generated.licenseBytes],
  ]);
  for (const [rel, expectedBytes] of expectedFiles) {
    const target = path.join(OUTPUT_ROOT, rel);
    if (!fs.existsSync(target)) {
      throw new Error(`Missing generated file: ${target}`);
    }
    const actual = fs.readFileSync(target);
    if (!actual.equals(expectedBytes)) {
      throw new Error(`Generated file out of date: ${target}`);
    }
  }
  if (!fs.existsSync(ICONS_DIR)) {
    throw new Error(`Missing generated icons directory: ${ICONS_DIR}`);
  }
  const actualIcons = fs.readdirSync(ICONS_DIR).sort(compareStrings);
  const expectedIcons = [...generated.svgOutputs.keys()].sort(compareStrings);
  if (actualIcons.length !== expectedIcons.length) {
    throw new Error(
      `Icon count mismatch: expected ${expectedIcons.length}, found ${actualIcons.length}`,
    );
  }
  for (let i = 0; i < expectedIcons.length; i++) {
    if (actualIcons[i] !== expectedIcons[i]) {
      throw new Error(
        `Icon filename mismatch at index ${i}: expected ${expectedIcons[i]}, found ${actualIcons[i]}`,
      );
    }
    const actual = fs.readFileSync(path.join(ICONS_DIR, actualIcons[i]));
    const expected = generated.svgOutputs.get(expectedIcons[i]);
    if (!actual.equals(expected)) {
      throw new Error(`SVG content out of date: ${actualIcons[i]}`);
    }
  }
  ensurePubspecAssets({ checkOnly: true });
}

function writeOutputs(generated) {
  fs.mkdirSync(ICONS_DIR, { recursive: true });
  for (const entry of fs.readdirSync(ICONS_DIR)) {
    if (!generated.svgOutputs.has(entry)) {
      fs.unlinkSync(path.join(ICONS_DIR, entry));
    }
  }
  for (const [filename, bytes] of generated.svgOutputs) {
    fs.writeFileSync(path.join(ICONS_DIR, filename), bytes);
  }
  fs.writeFileSync(path.join(OUTPUT_ROOT, 'manifest.json'), generated.manifestText);
  fs.writeFileSync(path.join(OUTPUT_ROOT, 'seeds.json'), generated.seedsText);
  fs.writeFileSync(path.join(OUTPUT_ROOT, 'metadata.json'), generated.metadataText);
  fs.writeFileSync(path.join(OUTPUT_ROOT, 'LICENSE'), generated.licenseBytes);
  ensurePubspecAssets({ checkOnly: false });
}

const args = new Set(process.argv.slice(2));
const checkOnly = args.has('--check');
if (args.size > 1 || (args.size === 1 && !checkOnly)) {
  throw new Error('Usage: node tool/sync_material_icons.mjs [--check]');
}

const generated = buildGeneratedOutputs();
if (checkOnly) {
  checkOutputs(generated);
  console.log(
    `Verified Material Icon Theme ${generated.sourceVersion} (${generated.counts.icons} icons).`,
  );
} else {
  writeOutputs(generated);
  console.log(
    `Synced Material Icon Theme ${generated.sourceVersion}: ${generated.counts.icons} icons (${generated.counts.nonNullSeeds} chromatic seeds, ${generated.counts.nullSeeds} neutral, ${generated.counts.overriddenSeeds} overrides).`,
  );
}
