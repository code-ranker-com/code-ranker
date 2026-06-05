// Which snapshot the diagrams / tables / modal show: 'baseline' or 'current'.
// In review mode (no baseline) it is always 'baseline'.
// `current` is the primary snapshot (the report's current state, normally always
// present); `baseline` is the optional baseline. activeSnap returns whichever the
// shown side has, falling back to the other so a single-snapshot report works.
function activeSnap() {
  return window.viewSide === 'baseline'
    ? (window.BASELINE ?? window.CURRENT)
    : (window.CURRENT ?? window.BASELINE);
}

// The three view modes: 'review' (no baseline → single "view"), or, in a
// diff, 'baseline' / 'current' depending on which side the user is looking at. This
// is the single source of truth for the labels/headers/URL across the viewer.
function viewMode() {
  if (!window.BASELINE || !window.CURRENT) return 'review';   // only one snapshot loaded
  return window.viewSide === 'current' ? 'current' : 'baseline';
}
// Label suffix for the active side: ' Baseline' / ' Current' in a diff, '' in review.
function viewModeSuffix() {
  const m = viewMode();
  return m === 'current' ? ' Current' : m === 'baseline' ? ' Baseline' : '';
}
function activeGraph(level) {
  return activeSnap()?.graphs?.[level] || { nodes: [], edges: [] };
}

// The graph drawn on the main map. External (3rd-party library) nodes and the
// edges into them are dropped here — they would clutter the file map. They are
// still kept in the snapshot and shown in the per-node neighbourhood modal.
function activeLocalGraph(level) {
  const g = activeGraph(level);
  const nodes = g.nodes.filter(n => !isExternalNode(n, level));
  const ids = new Set(nodes.map(n => n.id));
  const edges = g.edges.filter(e => ids.has(e.source) && ids.has(e.target));
  return { nodes, edges };
}

// The graph the main map is *laid out* from: the union of baseline+current (the diff
// graph), which is already external-free and carries a per-element `status`
// (added / removed / unchanged / affected). Laying out the union ONCE — then
// merely hiding the other side's added/removed elements via CSS — keeps every
// node that exists on both sides pinned in place: toggling Baseline/Current no longer
// reflows the graph, only the genuinely added/removed parts appear or disappear.
// (In review mode the diff is baseline-vs-baseline, so everything is `unchanged`.)
function unionGraph(level) {
  return window.DIFF?.[level] || { nodes: [], edges: [] };
}

// Flip which side's exclusive elements are visible on a frame, without relayout.
// `baseline` hides current-only (added) elements; `current` hides baseline-only (removed)
// ones; review keeps the lot. Drives the `.hide-{nodes,edges}-{added,removed}`
// CSS rules already defined for the frame.
function applySideVisibility(frame) {
  if (!frame) return;
  frame.classList.remove('hide-nodes-added', 'hide-edges-added',
                         'hide-nodes-removed', 'hide-edges-removed',
                         'side-baseline', 'side-current');
  const m = viewMode();
  if (m === 'baseline')     frame.classList.add('hide-nodes-added', 'hide-edges-added');
  else if (m === 'current') frame.classList.add('hide-nodes-removed', 'hide-edges-removed');
  // The `side-*` marker gates cycle highlighting (a `baseline-only` cycle is red
  // only on Baseline, `current-only` only on Current, `both` on either) and follows the
  // side actually shown — in review the single snapshot's cycles are all `both`.
  frame.classList.add(window.viewSide === 'current' ? 'side-current' : 'side-baseline');
}

// In the metric size modes (loc/hk), resize each circle to the *active side's*
// value while keeping the union-layout centre — so toggling Baseline/Current changes
// sizes (a file that grew/shrank) without ever moving a node. Default mode draws
// fixed boxes, identical on both sides, so it needs no per-side resize.
function applySideSizing(frame, level) {
  if (!frame) return;
  const mode = window.nodeSizeMode || 'default';
  if (mode === 'default') return;
  const byId = new Map((activeSnap()?.graphs?.[level]?.nodes || []).map(n => [n.id, n]));
  frame.querySelectorAll('g.node').forEach(g => {
    const n   = byId.get(g.dataset.nodeId);
    const ell = g.querySelector('ellipse');
    if (!n || !ell) return;   // node absent on this side → leave it (it's hidden)
    const cx = parseFloat(ell.getAttribute('cx'));
    const cy = parseFloat(ell.getAttribute('cy'));
    const d  = metricNodeDiam(n, mode);
    const r  = (d * 36).toFixed(2);     // graphviz: radius(pt) = diameter(in) × 72 / 2
    ell.setAttribute('rx', r);
    ell.setAttribute('ry', r);
    const txt = g.querySelector('text');
    if (txt) {
      const fs = metricFontSize(d);
      const v  = metricNodeVal(n, mode);
      txt.setAttribute('font-size', fs.toFixed(2));
      txt.setAttribute('x', cx);
      txt.setAttribute('y', (cy + fs * 0.3).toFixed(2));   // baseline ≈ centre + 0.3·fs
      txt.textContent = v > 0 ? fmtMetricShort(v) : '';
    }
  });
}

// Toggle Baseline/Current. The map is a single shared (union) layout, so switching
// sides is just a visibility flip — no relayout, no pan/zoom reset, and nodes
// present on both sides never move. Only the tables (active side) and the
// warning count are refreshed.
function setViewSide(side) {
  if (side === window.viewSide
      || (side === 'current'  && !window.CURRENT)
      || (side === 'baseline' && !window.BASELINE)) return;
  window.viewSide = side;
  window.navSetSide?.();
  // Clear stale hover highlights: refreshing the table below replaces the hovered
  // row without firing its `mouseleave`, which would otherwise leave its map node
  // lit — and a second hover would then light a second node.
  document.querySelectorAll('g.node.node-hl').forEach(n => n.classList.remove('node-hl'));
  document.querySelectorAll('.row-hl').forEach(r => r.classList.remove('row-hl'));
  // Active-side feedback is the highlighted control (snap-active); the old
  // Baseline/Current nav buttons are gone.
  document.querySelectorAll('.view').forEach(sec => {
    const frame = sec.querySelector('.svg-frame');
    applySideVisibility(frame);
    applySideSizing(frame, sec.dataset.view);
    sec._refreshNodeTable?.();
  });
  updateWarnCount();
  updateActiveSnapGroup();
  // If a node modal is open, re-render it for the new side so its diagram and
  // fields follow the toggle (otherwise only the map/tables/URL would update).
  const m = window._modalNode;
  if (m && document.getElementById('node-modal-overlay')?.style.display === 'flex')
    window.openModalForNode?.(m.id, m.level);
}

// The header "toggle" button and the `t` hotkey switch baseline⇄current. A no-op
// in review (a single snapshot — nothing to toggle).
function toggleViewSide() {
  if (!window.BASELINE || !window.CURRENT) return;
  setViewSide(window.viewSide === 'current' ? 'baseline' : 'current');
}
function setupModeToggle() {
  document.getElementById('meta-mode')?.addEventListener('click', toggleViewSide);
  document.addEventListener('keydown', e => {
    if ((e.key !== 't' && e.key !== 'T') || e.metaKey || e.ctrlKey || e.altKey) return;
    const t = e.target;
    if (t && (/^(input|textarea|select)$/i.test(t.tagName) || t.isContentEditable)) return;
    toggleViewSide();
  });
}

// Relocate the live <header> into a full-screen overlay (the node modal / the
// Prompt-Generator popup) as a slide-down bar, so its controls (toggle, snapshot
// controls, prompt button) stay reachable while the overlay covers the page;
// restored on close. One owner at a time. Fullscreen does its own header
// relocation (panzoom), so this defers while a fullscreen element is active.
window.flyoutHeader = (function () {
  let home = null, homeNext = null, bar = null, owner = null, ownerEl = null;
  return {
    mount(container, key) {
      if (owner || document.fullscreenElement || !container) return;
      const header = document.querySelector('header');
      if (!header) return;
      home = header.parentElement; homeNext = header.nextSibling;
      bar = document.createElement('div');
      // Always visible in these overlays — not a hover-reveal flyout.
      bar.className = 'fs-bar visible';
      bar.append(header);
      container.appendChild(bar);
      // Reserve exactly the bar's height as top space (it may wrap to >1 line, so
      // a fixed value would be wrong) — reading offsetHeight forces a sync layout.
      container.classList.add('fly-header-host');
      container.style.paddingTop = bar.offsetHeight + 'px';
      owner = key; ownerEl = container;
    },
    unmount(key) {
      if (owner !== key) return;
      const header = document.querySelector('header');
      if (home && header) home.insertBefore(header, homeNext);
      bar?.remove();
      ownerEl?.classList.remove('fly-header-host');
      if (ownerEl) ownerEl.style.paddingTop = '';
      bar = null; home = null; owner = null; ownerEl = null;
    }
  };
})();

// Refresh the distinct-warning-type count next to the Prompt-Generator button
// for the active level (it tracks the active side).
function updateWarnCount() {
  const warnEl = document.getElementById('nav-warn-count');
  if (!warnEl) return;
  const n = window.warningTypeCount?.(currentLevel()) ?? 0;
  warnEl.textContent = n ? String(n) : '';
}

function updateHeader() {
  const meta      = window.META;
  const hasBaseline = meta.baseline !== null;
  const hasCurrent  = meta.current  !== null;
  const isReview  = !hasBaseline || !hasCurrent;   // only one snapshot present → "review"

  document.body.classList.toggle('mode-review', isReview);
  // The mode word moved into the meta area, so the title is just the project.
  // Capped to an ellipsis in CSS; the full value stays reachable via `title`.
  const titleEl = document.getElementById('title');
  titleEl.textContent = meta.target;
  titleEl.title = meta.target;
  // Toggle button: shown only in a diff (click or `t` to switch baseline⇄current).
  // In review there is a single snapshot — nothing to toggle — so it is hidden.
  const modeEl = document.getElementById('meta-mode');
  modeEl.style.display = isReview ? 'none' : '';
  modeEl.textContent = 'toggle';
  modeEl.title = 'Click to toggle baseline ⇄ current (press t)';

  // Each snapshot control is shown only when its snapshot exists; the missing
  // side's "Set …" action lives in the surviving control's popup.
  document.querySelector('.snap-group[data-snap="baseline"]').style.display = hasBaseline ? '' : 'none';
  const baselineName = document.getElementById('meta-baseline-name');
  baselineName.textContent = hasBaseline ? meta.baseline.name : '';
  baselineName.title       = hasBaseline ? meta.baseline.name : '';
  document.getElementById('meta-baseline-commit').textContent = hasBaseline && meta.baseline.commit ? ` ${meta.baseline.commit}` : '';

  document.querySelector('.snap-group[data-snap="current"]').style.display = hasCurrent ? '' : 'none';
  const currentName = document.getElementById('meta-current-name');
  currentName.textContent = hasCurrent ? meta.current.name : '';
  currentName.title       = hasCurrent ? meta.current.name : '';
  document.getElementById('meta-current-commit').textContent  = hasCurrent && meta.current.commit ? ` ${meta.current.commit}` : '';

  // Keep the shown side on a snapshot that actually exists.
  if (!hasCurrent)       window.viewSide = 'baseline';
  else if (!hasBaseline) window.viewSide = 'current';
  updateActiveSnapGroup();
}

// Files is the only graph level — nothing to toggle. Kept as a no-op so callers
// (app bootstrap, snapshot swap) need no changes.
function updateFilesTab() {}

// Recompute everything after the user swaps a snapshot via the upload controls.
function recomputeAll() {
  const EMPTY = { graphs: {} };
  const baseline = window.BASELINE;
  const current  = window.CURRENT;
  // Either side may be absent (review). Fall back the missing side to the present
  // one so the diff is against itself (everything "unchanged"), never against empty.
  window.DIFF   = computeDiff(baseline ?? current ?? EMPTY, current ?? baseline ?? EMPTY);
  window.CYCLES = computeCycles(baseline ?? current ?? EMPTY, current ?? baseline ?? EMPTY);
  window.META   = computeMeta(baseline, current);

  buildSummary();
  updateFilesTab();

  // Reset rendered state for all views; the active one re-renders below.
  document.querySelectorAll('.view').forEach(sec => { sec.dataset.rendered = 'false'; });

  updateHeader();

  // Re-render active view
  const active = document.querySelector('.view.active');
  if (active && window.gv) renderView(active);
}

function renderView(section, opts = {}) {
  const level   = section.dataset.view;
  const frame   = section.querySelector('.svg-frame');
  const loading = section.querySelector('.loading-indicator');

  // Plain count of distinct warning types next to the Prompt-Generator (AI) button.
  updateWarnCount();

  // Preserve pan/zoom across a re-render (a size-mode switch — Baseline/Current no
  // longer relayouts). Size modes have different coordinate extents, so we carry
  // the view as *relative* zoom + fractional centre (vs each layout's fit-all
  // viewBox) rather than absolute coords — otherwise the framing drifts.
  let viewSpec = null, wasZoomed = false;
  if (opts.preserve) {
    const cur = frame.querySelector('svg')?.getAttribute('viewBox');
    const nat = frame.dataset.naturalVB;
    if (cur && nat && cur !== nat) {
      const [cx, cy, cw, ch] = cur.split(/[ ,]+/).map(Number);
      const [ox, oy, ow, oh] = nat.split(/[ ,]+/).map(Number);
      if (ow > 0 && oh > 0) {
        viewSpec = {
          zw: cw / ow, zh: ch / oh,
          fx: (cx + cw / 2 - ox) / ow,
          fy: (cy + ch / 2 - oy) / oh,
        };
        wasZoomed = frame.classList.contains('zoomed');
      }
    }
  }

  if (loading) { loading.textContent = 'Computing layout…'; loading.classList.add('on'); }
  setTimeout(() => {
    // Lay out the union graph once; the active side is shown via CSS visibility.
    const g = unionGraph(level);
    drawSVG(frame, g.nodes, g.edges, level);
    applySideVisibility(frame);
    applySideSizing(frame, level);
    window._ntSelected?.[level]?.forEach(id => section._gNodeMap?.get(id)?.classList.add('node-selected'));
    if (viewSpec) {
      const svg = frame.querySelector('svg');
      const [ox, oy, ow, oh] = (frame.dataset.naturalVB || '0 0 0 0').split(/[ ,]+/).map(Number);
      if (svg && ow > 0 && oh > 0) {
        const nw = viewSpec.zw * ow, nh = viewSpec.zh * oh;
        const cx = ox + viewSpec.fx * ow, cy = oy + viewSpec.fy * oh;
        svg.setAttribute('viewBox', `${cx - nw / 2} ${cy - nh / 2} ${nw} ${nh}`);
        if (wasZoomed) frame.classList.add('zoomed');
      }
    }
    section.dataset.rendered = 'true';
    section._refreshNodeTable?.();
    if (loading) loading.classList.remove('on');
  }, 30);
}

function buildSnapPopupHTML(snap, refSnap, sideLabel) {
  if (!snap) return '';
  const sections = [];

  // Which snapshot this is (Baseline / Current) and whether it is the one currently
  // shown on the map / tables.
  if (sideLabel) {
    const shown = sideLabel.toLowerCase() === window.viewSide;
    sections.push(`<div class="sp-side ${shown ? 'sp-side-shown' : ''}">${sideLabel}${shown ? ' · currently shown' : ''}</div>`);
  }

  // General
  const genRows = [];
  if (snap.generated_at) {
    let dateStr = fmtDate(snap.generated_at);
    if (refSnap?.generated_at) {
      const diffMs = new Date(snap.generated_at) - new Date(refSnap.generated_at);
      dateStr += ` <span class="sp-gen-diff">(diff ${fmtDuration(Math.abs(diffMs))})</span>`;
    }
    genRows.push(`<div class="sp-row"><span class="sp-lbl">Generated</span><span>${dateStr}</span></div>`);
  }
  if (snap.command)
    genRows.push(`<div class="sp-cmd-block"><div class="sp-cmd-bar"><span class="sp-lbl">Command</span><button class="sp-copy-btn" data-copy="${escAttr(snap.command)}" title="Copy">⧉</button></div><textarea class="sp-cmd-input" readonly rows="3">${escHtml(snap.command)}</textarea></div>`);
  if (genRows.length)
    sections.push(`<div class="sp-section-label">General</div>${genRows.join('')}`);

  // Git
  if (snap.git) {
    const { branch, commit, dirty_files } = snap.git;
    const gitRows = [];
    if (branch)              gitRows.push(`<div class="sp-row"><span class="sp-lbl">Branch</span><span>${escHtml(branch)}</span></div>`);
    if (commit)              gitRows.push(`<div class="sp-row"><span class="sp-lbl">Commit hash</span><span>${escHtml(commit)}</span></div>`);
    if (dirty_files != null) gitRows.push(`<div class="sp-row"><span class="sp-lbl">Dirty files</span><span>${dirty_files > 0 ? dirty_files : '0 (clean)'}</span></div>`);
    if (gitRows.length)
      sections.push(`<div class="sp-section-label">Git</div>${gitRows.join('')}`);
  }

  // Duration
  if (snap.timings?.length) {
    const trows = snap.timings.map(t =>
      `<div class="sp-row"><span class="sp-lbl">${escHtml(t.stage)}</span><span>${fmtMs(t.ms)}</span></div>`
    ).join('');
    sections.push(`<div class="sp-section-label">Duration</div>${trows}`);
  }

  // Actions — moved out of the header. Wired in `show()`. The baseline control
  // offers replace + remove; the current control offers replace, plus "Set
  // baseline" in review (where there is no separate baseline control).
  // Symmetric actions: each side can be replaced; it can be removed only while the
  // OTHER side exists (so at least one snapshot always remains); and the missing
  // side can be set from here.
  const acts = [];
  if (sideLabel === 'Baseline') {
    acts.push('<button class="sp-action" data-act="upload-baseline">↑ Replace baseline</button>');
    if (window.CURRENT)   acts.push('<button class="sp-action sp-action-x" data-act="remove-baseline">✕ Remove baseline</button>');
    if (!window.CURRENT)  acts.push('<button class="sp-action" data-act="upload-current">↑ Set current</button>');
  } else if (sideLabel === 'Current') {
    acts.push('<button class="sp-action" data-act="upload-current">↑ Replace current</button>');
    if (window.BASELINE)  acts.push('<button class="sp-action sp-action-x" data-act="remove-current">✕ Remove current</button>');
    if (!window.BASELINE) acts.push('<button class="sp-action" data-act="upload-baseline">↑ Set baseline</button>');
  }
  if (acts.length)
    sections.push(`<div class="sp-section-label">Actions</div><div class="sp-actions">${acts.join('')}</div>`);

  return sections.join('');
}

function setupSnapPopup() {
  let popup = null, openFor = null;   // openFor = the .snap-group whose popup is shown

  function getPopup() {
    if (!popup) {
      popup = document.createElement('div');
      popup.id = 'snap-popup';
      // Fullscreen shows only the frame; attach there so the header's popup is visible.
      (document.fullscreenElement || document.body).appendChild(popup);
    }
    return popup;
  }

  function hide() { if (popup) popup.style.display = 'none'; openFor = null; }

  function show(snap, anchor, refSnap, sideLabel) {
    if (!snap) return;
    const p = getPopup();
    p.innerHTML = buildSnapPopupHTML(snap, refSnap, sideLabel);
    p.querySelectorAll('.sp-copy-btn').forEach(btn => {
      btn.addEventListener('click', e => {
        e.stopPropagation();
        navigator.clipboard?.writeText(btn.dataset.copy).then(() => {
          btn.textContent = '✓';
          setTimeout(() => { btn.textContent = '⧉'; }, 1500);
        });
      });
    });
    // Upload / remove actions (moved here from the header).
    p.querySelectorAll('.sp-action').forEach(btn => {
      btn.addEventListener('click', e => {
        e.stopPropagation();
        const act = btn.dataset.act;
        if (act === 'upload-baseline')      document.getElementById('input-baseline').click();
        else if (act === 'upload-current')  document.getElementById('input-current').click();
        else if (act === 'remove-baseline') { window.BASELINE = null; recomputeAll(); }
        else if (act === 'remove-current')  { window.CURRENT = null;  recomputeAll(); }
        hide();
      });
    });
    p.style.display = 'block';
    openFor = anchor;
    requestAnimationFrame(() => {
      const r  = anchor.getBoundingClientRect();
      let left = r.left;
      let top  = r.bottom + 6;
      if (left + p.offsetWidth  > window.innerWidth  - 8) left = window.innerWidth  - p.offsetWidth  - 8;
      if (top  + p.offsetHeight > window.innerHeight - 8) top  = r.top - p.offsetHeight - 6;
      p.style.left = left + 'px';
      p.style.top  = top  + 'px';
    });
  }

  document.querySelectorAll('.snap-group').forEach((grp, i) => {
    // Click the control body → switch which side is shown on the map.
    grp.addEventListener('click', () => setViewSide(i === 0 ? 'baseline' : 'current'));
    // Click the pencil → open/close the details + actions popup (no hover trigger).
    // `stopPropagation` so it does not also switch the side or count as an outside click.
    grp.querySelector('.snap-edit')?.addEventListener('click', e => {
      e.stopPropagation();
      if (openFor === grp) { hide(); return; }
      const snap = i === 0 ? window.BASELINE : window.CURRENT;
      const ref  = i === 1 ? window.BASELINE : null;
      show(snap, grp, ref, i === 0 ? 'Baseline' : 'Current');
    });
  });

  // Dismiss on an outside click or Escape.
  document.addEventListener('click', e => {
    if (openFor && popup && !popup.contains(e.target) && !openFor.contains(e.target)) hide();
  });
  document.addEventListener('keydown', e => { if (e.key === 'Escape') hide(); });
}

// Mark the header snapshot group (the file uploader at the top) for the side
// currently shown, so the active input is visually distinguished. Hidden in
// review mode (a single snapshot — nothing to distinguish).
function updateActiveSnapGroup() {
  const active = window.CURRENT ? window.viewSide : null;
  document.querySelectorAll('.snap-group[data-snap]').forEach(grp => {
    grp.classList.toggle('snap-active', grp.dataset.snap === active);
  });
}

// Read a snapshot embedded inline in this page (a `<script type="application/json">` tag).
function readEmbeddedSnapshot(id) {
  const el = document.getElementById(id);
  if (!el) return null;
  const t = el.textContent.trim();
  return t && t !== 'null' ? JSON.parse(t) : null;
}

// Parse a snapshot from uploaded file text — either a raw JSON snapshot, or a code-split
// HTML report with the snapshot embedded (prefer `cs-current`, else `cs-baseline`).
function extractSnapshotFromText(text) {
  const s = text.trim();
  if (s.startsWith('{')) return JSON.parse(s);
  const doc = new DOMParser().parseFromString(text, 'text/html');
  const read = id => {
    const t = doc.getElementById(id)?.textContent?.trim();
    return t && t !== 'null' ? JSON.parse(t) : null;
  };
  return read('cs-current') || read('cs-baseline');
}

// Manual snapshot swap: load a different baseline/current snapshot (.json or .html) into the viewer.
function setupFileControls() {
  const inputBaseline = document.getElementById('input-baseline');
  const inputCurrent  = document.getElementById('input-current');

  // The upload / remove buttons now live in the snapshot popup (see `show`),
  // which clicks these hidden inputs. Here we only handle the chosen file.

  inputBaseline.addEventListener('change', () => {
    const file = inputBaseline.files[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = e => {
      try { window.BASELINE = extractSnapshotFromText(e.target.result); } catch { alert('Invalid snapshot file'); return; }
      recomputeAll();
    };
    reader.readAsText(file);
    inputBaseline.value = '';
  });

  inputCurrent.addEventListener('change', () => {
    const file = inputCurrent.files[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = e => {
      try { window.CURRENT = extractSnapshotFromText(e.target.result); } catch { alert('Invalid snapshot file'); return; }
      recomputeAll();
    };
    reader.readAsText(file);
    inputCurrent.value = '';
  });
}

document.addEventListener('DOMContentLoaded', async () => {
  window.nodeSizeMode = 'default';

  // Read the snapshots embedded inline in the page (cs-baseline / cs-current script tags).
  window.BASELINE = readEmbeddedSnapshot('cs-baseline');
  window.CURRENT  = readEmbeddedSnapshot('cs-current');

  const EMPTY = { graphs: {} };
  window.DIFF   = computeDiff(window.BASELINE ?? window.CURRENT ?? EMPTY, window.CURRENT ?? window.BASELINE ?? EMPTY);
  window.CYCLES = computeCycles(window.BASELINE ?? window.CURRENT ?? EMPTY, window.CURRENT ?? window.BASELINE ?? EMPTY);
  window.META   = computeMeta(window.BASELINE, window.CURRENT);

  // Restore the active side from the URL (`side=baseline/current`); default to the
  // current (primary) snapshot. `baseline` is only honoured when a baseline exists.
  const urlSide = getNavParams().side;
  window.viewSide = (urlSide === 'baseline' && window.BASELINE) ? 'baseline'
                  : window.CURRENT ? 'current'
                  : 'baseline';
  // If the Prompt Generator was open (state in the URL), restore its selected
  // nodes before the tables render so those rows come up already selected.
  const epState = (typeof epReadUrl === 'function') ? epReadUrl() : null;
  if (epState?.sel?.length) {
    if (!window._ntSelected) window._ntSelected = {};
    window._ntSelected[epState.level] = new Set(epState.sel);
  }
  document.querySelectorAll('.view').forEach(sec => setupNodeTable(sec, sec.dataset.view));
  setupSnapPopup();
  setupModeToggle();
  setupFileControls();
  setupTooltip();
  buildSummary();
  updateFilesTab();
  updateHeader();

  document.getElementById('summary-header')?.addEventListener('click', () => {
    document.querySelector('.summary').classList.toggle('collapsed');
  });


  const active = document.querySelector('.view.active');
  const loading = active?.querySelector('.loading-indicator');
  if (loading) { loading.textContent = 'Loading Graphviz…'; loading.classList.add('on'); }

  window.gv = await window['@hpcc-js/wasm/graphviz'].Graphviz.load();

  renderView(active);

  // Restore state from URL, then set initial history entry
  const { level: urlLevel, node: urlNode } = getNavParams();
  if (urlLevel && urlLevel !== currentLevel()) switchToLevel(urlLevel);
  if (urlNode) openModalForNode(urlNode, urlLevel ?? currentLevel());
  // Replace initial history state so popstate can restore it
  history.replaceState({ level: currentLevel(), node: urlNode ?? null }, '', location.href);

  // Re-open the Prompt Generator if the URL says it was open.
  if (epState) {
    if (epState.level && epState.level !== currentLevel()) switchToLevel(epState.level);
    openExportPopup(epState.level, epState);
  }

  window.addEventListener('popstate', e => {
    const st = e.state || getNavParams();
    const lvl  = st.level;
    const nid  = st.node;
    const side = st.side;
    if (window.CURRENT && (side === 'baseline' || side === 'current')) setViewSide(side);
    if (lvl && lvl !== currentLevel()) switchToLevel(lvl);
    if (nid) {
      openModalForNode(nid, lvl ?? currentLevel());
    } else {
      closeModalSilent();
    }
  });
});
