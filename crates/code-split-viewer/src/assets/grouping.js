// grouping.js — the grouping ladder for the map's relative zoom (level-of-detail).
//
// Two orthogonal navigation axes (see docs/code-split-viewer/REFACTOR-split-plan.md):
//   • window.zoom  — relative LOD on the OVERVIEW. 0 = the default crate tier;
//                    +1 descends one directory level (crate/folder groups),
//                    -1 ascends to the crate's parent folder (workspace subfolders).
//   • focus (window.drillGroup) — click a group to drill into just its files.
//
// For Rust (group=crate, node=file) the tier sequence is, coarse → fine:
//   …workspace-subfolders… ▸ crate ▸ …dirs under the crate… ▸ file
// Every tier is DERIVED from the file id path plus the crate grouping attribute,
// so no extra backend data is needed. zoom 0 reproduces the legacy crate grouping
// exactly (so the default view is byte-for-byte unchanged).

const ZOOM_MIN = -2, ZOOM_MAX = 4;
function clampZoom(z) { return Math.max(ZOOM_MIN, Math.min(ZOOM_MAX, (z | 0))); }
window.clampZoom = clampZoom;

// Strip the leading `{token}/` root marker from an id/path.
function relPathOf(id) { return String(id || '').replace(/^\{[^}]+\}\//, ''); }

// Per-level memoised crate-root directories: the common directory prefix of all
// files sharing a crate value. Locating where a crate sits in the tree lets zoom
// ascend above it (workspace subfolders) or descend below it (dirs under it).
const _crateRootCache = new Map();   // level -> Map<crateValue, string[] dirSegs>
function crateRoots(level) {
  if (_crateRootCache.has(level)) return _crateRootCache.get(level);
  const gk = levelUi(level).grouping?.key;
  const byCrate = new Map();
  if (gk) {
    for (const n of (unionGraph(level).nodes || [])) {
      if (isExternalNode(n, level)) continue;
      const crate = n[gk];
      if (crate == null || crate === '') continue;
      const dirs = relPathOf(n.id).split('/').slice(0, -1);   // drop the filename
      const arr  = byCrate.get(String(crate));
      if (arr) arr.push(dirs); else byCrate.set(String(crate), [dirs]);
    }
  }
  const roots = new Map();
  for (const [crate, list] of byCrate) {
    let prefix = list[0].slice();
    for (let k = 1; k < list.length; k++) {
      const segs = list[k];
      let i = 0;
      while (i < prefix.length && i < segs.length && prefix[i] === segs[i]) i++;
      prefix = prefix.slice(0, i);
    }
    roots.set(crate, prefix);
  }
  _crateRootCache.set(level, roots);
  return roots;
}
// Snapshot swaps change the node set → drop the memoised roots.
function clearGroupingCache() { _crateRootCache.clear(); }
window.clearGroupingCache = clearGroupingCache;

// Group key for a node at a given zoom. zoom 0 → the crate value (matches the
// legacy makeGroupOf). zoom>0 appends directory segments under the crate; zoom<0
// collapses crates into their ancestor (workspace) folders.
function groupKeyAtZoom(level, n, zoom) {
  if (isExternalNode(n, level))
    return (nodeKindSpec(level, n.kind).plural || 'external').toLowerCase();

  const z    = zoom | 0;
  const gk   = levelUi(level).grouping?.key;
  const crate = gk ? n[gk] : null;
  const dirs  = relPathOf(n.id).split('/').slice(0, -1);

  // No crate attribute: fall back to plain directory tiers (zoom 0 = full dir,
  // matching the legacy dirGrouper).
  if (crate == null || crate === '') {
    const depth = dirs.length + z;
    const keep  = dirs.slice(0, Math.max(0, depth));
    return keep.length ? keep.join('/') : '_root';
  }

  if (z >= 0) {
    const root       = crateRoots(level).get(String(crate)) || [];
    const underCrate = dirs.slice(root.length);
    return [String(crate), ...underCrate.slice(0, z)].join('/');
  }
  // z < 0: ascend ABOVE the crate. -1 groups crates by their top-level workspace
  // folder (so everything under `crates/` merges into one "crates" group);
  // anything coarser collapses to a single root group. Top-level-dir is robust
  // from file paths alone — unlike the exact crate-root, which is fuzzy when all
  // of a crate's files share a deeper dir (e.g. `src/`).
  if (z === -1) return dirs[0] || '_root';
  return '_root';
}

// A `groupOf(node)` closure for a given zoom. grouperForZoom(level, 0) reproduces
// makeGroupOf's crate/dir grouping.
function grouperForZoom(level, zoom) {
  return n => groupKeyAtZoom(level, n, zoom || 0);
}
window.grouperForZoom = grouperForZoom;

// Legacy entry point (zoom 0). Kept so existing callers keep working; moved here
// from layout.js because grouping is now its own concern.
function makeGroupOf(level) {
  return grouperForZoom(level, 0);
}

// Aggregate the per-node cycle statuses of a group's members into one status for
// the group node (used to red-stroke groups that contain a dependency cycle).
function aggCycleStatus(statuses) {
  let b = false, c = false, both = false;
  for (const s of statuses) {
    if (s === 'both') both = true;
    else if (s === 'baseline-only') b = true;
    else if (s === 'current-only') c = true;
  }
  if (both || (b && c)) return 'both';
  if (b) return 'baseline-only';
  if (c) return 'current-only';
  return 'none';
}
