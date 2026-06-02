// Online principle docs (principles/<lang>/<slug>.md on GitHub) keyed by preset.
const PRINCIPLE_DOCS = {
  'CPX': 'reduce-complexity',
  ADP:   'acyclic-dependencies-principle',
  SRP:   'solid-single-responsibility',
  OCP:   'solid-open-closed',
  LSP:   'solid-liskov-substitution',
  ISP:   'solid-interface-segregation',
  DIP:   'solid-dependency-inversion',
  DRY:   'dry',
  KISS:  'kiss',
  LoD:   'law-of-demeter',
  MISU:  'make-invalid-states-unrepresentable',
  CoI:   'composition-over-inheritance',
  YAGNI: 'yagni',
};
const PRINCIPLES_URL = 'https://github.com/ffedoroff/code-split/blob/main/principles';

// The principle-corpus language for the analyzed plugin (JS/TS share `typescript`).
function principleLang() {
  const plugin = (window.AFTER ?? window.BEFORE)?.plugin || 'rust';
  return plugin === 'javascript' ? 'typescript' : plugin;
}

// Online URL of a preset's full principle doc, or null for the meta-presets.
function principleUrl(key) {
  const slug = PRINCIPLE_DOCS[key];
  return slug ? `${PRINCIPLES_URL}/${principleLang()}/${slug}.md` : null;
}

// ── Metric values & thresholds (shared by the popup and the nav warning count) ─
// Per-node value for each metric.
const METRIC_VAL = {
  sloc:       n => n.complexity?.loc?.source ?? n.loc ?? 0,
  hk:         n => n.complexity?.coupling?.hk ?? 0,
  fan_out:    n => n.complexity?.coupling?.fan_out ?? 0,
  cyclomatic: n => n.complexity?.cyclomatic ?? 0,
  cognitive:  n => n.complexity?.cognitive ?? 0,
  item_count: n => n.item_count ?? 0,
};

// Two-tier thresholds per metric: `info` (worth a look) and `warning` (likely a
// problem). EMPIRICALLY calibrated and **language-specific** (a language without
// its own data falls back to `rust`). Targets: ~50 % of projects breach `info`,
// ~10 % breach `warning` (binary: ≥1 file over the line).
//
// ⚠ SOURCE OF TRUTH IS SHARED WITH DOCS — every value here MUST be kept in sync
// with `principles/<lang>/metric-thresholds.md` (Rust →
// `principles/rust/metric-thresholds.md`), which records how each number was
// derived. Change a threshold → update that file in the same commit; add a
// language block → add its sibling md file too.
const METRIC_TH_BY_LANG = {
  // Rust — calibrated on 21 Rust crates (each ≥2K SLOC) from a 35-repo corpus.
  // Sync: principles/rust/metric-thresholds.md
  rust: {
    hk:         { info: 150000, warning: 10000000 },
    sloc:       { info: 800,    warning: 3000 },
    fan_out:    { info: 8,      warning: 18 },
    item_count: { info: 20,     warning: 50 },
    // cyclomatic & cognitive are intentionally NOT tracked for Rust: file-level
    // cyclomatic is ~1 (not aggregated) and cognitive is not emitted, so neither
    // has a usable distribution. Presets that sort by them show no count.
  },
};
// Thresholds for the analyzed language (plugin name; JS/TS use `rust`'s block
// until their own corpora are calibrated).
function metricThresholds() {
  const lang = (window.AFTER ?? window.BEFORE)?.plugin || 'rust';
  return METRIC_TH_BY_LANG[lang] || METRIC_TH_BY_LANG.rust;
}

// Distinct warning *types* — how many metrics have at least one internal file
// over their `warning` threshold, plus `cycle` as one binary type (any file in
// a dependency cycle). Shown next to the Prompt-Generator (AI) button.
function warningTypeCount(level) {
  // Count over the active side, so the badge tracks Before/After like the rest.
  const nodes = ((typeof activeGraph === 'function' ? activeGraph(level).nodes : window.DIFF?.[level]?.nodes) || [])
    .filter(n => !n.external);
  const th = metricThresholds();
  let count = Object.keys(th).filter(m =>
    nodes.some(n => (METRIC_VAL[m]?.(n) ?? 0) > th[m].warning)
  ).length;
  const cy = window.CYCLES?.[level]?.nodeCycleStatus;
  if (cy && nodes.some(n => { const cs = cy.get(n.id); return cs != null && cs !== 'none'; })) count += 1;
  return count;
}
window.warningTypeCount = warningTypeCount;

// ── Prompt-Generator state in the URL ────────────────────────────────────────
// The popup persists its full state in the query string so a refresh restores it
// exactly (open state, preset, source, count, sort metric, connection toggles,
// and the selected node ids). `epsel` is repeated once per selected id.
const EP_KEYS = ['ep', 'eppreset', 'epsrc', 'epn', 'epsort', 'epconn', 'epsel'];

function epWriteUrlState(s) {
  const p = new URLSearchParams(location.search);
  EP_KEYS.forEach(k => p.delete(k));
  p.set('ep', s.level);
  if (s.preset) p.set('eppreset', s.preset);
  p.set('epsrc', s.src === 'selected' ? 'sel' : 'rec');
  if (s.n != null && s.n !== '') p.set('epn', String(s.n));
  if (s.sort) p.set('epsort', s.sort);
  if (s.conn && s.conn.length) p.set('epconn', s.conn.join(','));
  (s.sel || []).forEach(id => p.append('epsel', id));
  history.replaceState(history.state, '', '?' + p);
}

function epReadUrl() {
  const p = new URLSearchParams(location.search);
  if (!p.has('ep')) return null;
  return {
    level:  p.get('ep'),
    preset: p.get('eppreset') || null,
    src:    p.get('epsrc') || null,
    n:      p.get('epn'),
    sort:   p.get('epsort') || null,
    conn:   (p.get('epconn') || '').split(',').filter(Boolean),
    sel:    p.getAll('epsel'),
  };
}

function epClearUrl() {
  const p = new URLSearchParams(location.search);
  let changed = false;
  EP_KEYS.forEach(k => { if (p.has(k)) { changed = true; p.delete(k); } });
  if (changed) history.replaceState(history.state, '', p.toString() ? '?' + p : location.pathname);
}

function openExportPopup(level, restore) {
  const selectedIds = window._ntSelected?.[level];
  // Operate on the active side (Before/After), so the generated prompt matches
  // the snapshot the user is looking at — same source the map and table use.
  // (Review mode → the single snapshot.) Edges are kept to local↔local pairs,
  // mirroring the diff's edge set: no external links, no cross-side noise.
  const activeG     = (typeof activeGraph === 'function') ? activeGraph(level) : (window.DIFF?.[level] || {});
  const allNodes    = activeG.nodes || [];
  const localIds    = new Set(allNodes.filter(n => !n.external).map(n => n.id));
  const allEdges    = (activeG.edges || []).filter(e => localIds.has(e.from) && localIds.has(e.to));
  const selNodes    = allNodes.filter(n => selectedIds?.has(n.id));

  const cleanPath = p => (p || '').replace(/^\{[^}]+\}\//, '');
  // Edge endpoints are node ids; render them as the node's path (id as a fallback)
  // so connection lists in the prompt use paths, not raw ids.
  const nodeById = new Map(allNodes.map(n => [n.id, n]));
  const pathOf = id => { const n = nodeById.get(id); return n ? (cleanPath(n.path) || n.id) : id; };

  // Each value is `title\n\nsummary` (principle gist + the identify/propose task),
  // language-neutral — `composePrompt` wraps it into the full instruction.
  const PROMPTS = {
    'CPX':
`CPX — Reduce Complexity

These modules are too complex and I want to reduce their complexity.
Reduce it by splitting large units into smaller single-responsibility ones,
extracting repeated patterns into shared helpers, flattening deeply nested
control flow, and breaking large functions into focused helpers.`,

    'ADP':
`ADP — Acyclic Dependencies Principle

The dependency graph between modules must form a DAG. When module A depends
on module B, no chain of dependencies should bring B back to A.

Identify any cycles in the modules below. For each cycle, propose a concrete
refactoring (extract a shared abstraction, invert a dependency, split a module)
that makes the graph acyclic without breaking existing functionality.

When splitting a module to break a cycle, the new structure should:
- Preserve existing API contracts
- Minimise coupling in the new structure
- Follow the Single Responsibility Principle
- Not introduce new dependency cycles`,

    'SRP':
`SRP — Single Responsibility Principle

A module should have one reason to change — it should serve one actor
and encapsulate one coherent set of decisions.

For each module below, identify whether it has more than one responsibility.
Propose how to split responsibilities so each module changes for only one reason,
and specify the new module boundaries.`,

    'OCP':
`OCP — Open/Closed Principle

A module should be open for extension but closed for modification: new behaviour
should be addable without editing existing, working code.

For each module below, identify extension points that currently require editing
existing code (e.g. growing match/switch/if-else chains). Propose an extension
mechanism (polymorphism, strategy, plug-in registration) so new cases can be added
without modifying these modules.`,

    'LSP':
`LSP — Liskov Substitution Principle

Every implementation of an interface must honour its full contract — return-value
invariants, error/exception behaviour, side effects, and resource ownership — not
just the method signatures. A subtype must be substitutable for its base without
surprising callers.

Identify the interface implementations in the modules below. For each, check it can
replace any other implementation of the same interface without breaking callers.
Flag violations and propose fixes.`,

    'ISP':
`ISP — Interface Segregation Principle

Clients should not be forced to depend on methods they do not use. Prefer several
small, focused interfaces over one wide interface.

Identify interfaces in the modules below that are wider than their consumers need.
Propose how to split them into narrower interfaces so each consumer depends only on
what it actually uses.`,

    'DIP':
`DIP — Dependency Inversion Principle

High-level modules should not depend on low-level modules; both should depend on
abstractions, and abstractions should not depend on details.

Find places in the modules below where a high-level module depends directly on a
concrete low-level type. Propose an abstraction (interface) to invert each such
dependency, and specify where the concrete implementation should be wired in.`,

    'DRY':
`DRY — Don't Repeat Yourself

Every piece of knowledge must have a single authoritative representation.
DRY is about knowledge duplication, not just code duplication.

Identify concepts, rules, or policies that are duplicated across the modules
below. For each duplication, propose a canonical location and the refactoring
needed to consolidate it.`,

    'KISS':
`KISS — Keep It Simple

When two designs solve the same problem, prefer the simpler one — fewer
abstractions, fewer indirection layers, fewer moving parts.

Identify over-engineered or needlessly complex constructs in the modules below.
For each, describe the simpler alternative and estimate the risk of simplifying.`,

    'LoD':
`Law of Demeter — Principle of Least Knowledge

A method should only call methods on: itself, its direct fields,
its parameters, and objects it constructs locally.
Avoid \`x.foo().bar().baz()\` chains that traverse object graphs.

Identify method chains or deep field traversals in the modules below that
violate LoD. For each, propose a narrow accessor or a facade that exposes only
what the caller needs, reducing coupling.`,

    'MISU':
`MISU — Make Invalid States Unrepresentable

Move correctness from runtime checks into the type system, so invalid states
cannot be constructed and fail at compile time rather than at runtime.

Identify data structures or function signatures in the modules below where invalid
states are representable at runtime. For each, propose a type-level encoding
(sum type / enum, newtype, typestate) that makes the invalid state unrepresentable
by construction.`,

    'CoI':
`CoI — Composition Over Inheritance

Build behaviour by composing small, focused pieces rather than through deep
inheritance hierarchies.

Identify large types that accumulate behaviour in the modules below. Propose how to
decompose them into smaller composable parts, and show how consumers would assemble
the behaviour they need.`,

    'YAGNI':
`YAGNI — You Aren't Gonna Need It

Build for the problem you have now, not one you imagine you might have later.
Don't add an abstraction, a generic parameter, or a public API for a hypothetical
future use.

Identify abstractions, generics, or public APIs in the modules below that were
added speculatively. For each, assess whether multiple real callers use it today,
and propose simplification if not.`,
  };

  // ── popup DOM (created once) ──────────────────────────────────────────
  let overlay = document.getElementById('export-popup-overlay');
  if (!overlay) {
    const principleKeys = ['ADP','SRP','OCP','LSP','ISP','DIP','DRY','KISS','LoD','MISU','CoI','YAGNI'];
    overlay = document.createElement('div');
    overlay.id = 'export-popup-overlay';
    overlay.innerHTML =
      '<div id="export-popup">' +
        '<div id="export-popup-hdr">' +
          '<h3 id="export-popup-title">Prompt Generator</h3>' +
          '<button id="export-popup-close">✕</button>' +
        '</div>' +
        '<div class="exp-modes">' +
          '<div class="exp-cb-group">' +
            '<span class="exp-conn-label">Connections:</span>' +
            '<label class="exp-mode-cb"><input type="checkbox" data-mode="conn-in"> in</label>' +
            '<label class="exp-mode-cb"><input type="checkbox" data-mode="conn-out"> out</label>' +
            '<label class="exp-mode-cb"><input type="checkbox" data-mode="conn-common"> common</label>' +
          '</div>' +
          '<div class="exp-source-group">' +
            '<label class="exp-src-radio"><input type="radio" name="exp-source" value="selected" checked> <span class="exp-sel-count">0</span> Selected</label>' +
            '<span class="exp-source-or">OR</span>' +
            '<label class="exp-src-radio"><input type="radio" name="exp-source" value="recommended"> <input type="number" class="exp-rec-count" min="1" max="9999" value="1"></label>' +
            '<select class="exp-sort-select" title="Recommend the top rows sorted by this metric">' +
              '<option value="hk">HK</option>' +
              '<option value="sloc">SLOC</option>' +
              '<option value="fan_out">fan-out</option>' +
              '<option value="cyclomatic">cyclomatic</option>' +
              '<option value="cognitive">cognitive</option>' +
              '<option value="item_count">item count</option>' +
              '<option value="cycle">in a cycle</option>' +
            '</select>' +
          '</div>' +
        '</div>' +
        '<div class="exp-textarea-wrap">' +
          '<div id="export-preview" class="exp-md-preview"></div>' +
          '<textarea id="export-textarea" readonly></textarea>' +
          '<button class="exp-copy-btn">Copy markdown <span class="exp-copy-icon">⎘</span></button>' +
        '</div>' +
        '<div class="exp-presets">' +
          '<div class="exp-presets-label">Presets</div>' +
          '<div class="exp-preset-btns">' +
            '<button class="exp-preset-btn" data-preset="CPX">CPX<span class="exp-preset-count"></span></button>' +
            principleKeys.map(k => `<button class="exp-preset-btn" data-preset="${k}">${k}<span class="exp-preset-count"></span></button>`).join('') +
          '</div>' +
        '</div>' +
      '</div>';
    document.body.appendChild(overlay);

    const closeExport = () => { overlay.style.display = 'none'; document.body.style.overflow = ''; epClearUrl(); };
    document.getElementById('export-popup-close').addEventListener('click', closeExport);
    overlay.addEventListener('mousedown', e => { if (e.target === overlay) closeExport(); });
    document.addEventListener('keydown', e => { if (e.key === 'Escape' && overlay.style.display !== 'none') closeExport(); });
    overlay.querySelector('.exp-copy-btn').addEventListener('click', () => {
      const ta = document.getElementById('export-textarea');
      navigator.clipboard?.writeText(ta.value).then(() => {
        const btn = overlay.querySelector('.exp-copy-btn');
        const orig = btn.innerHTML;
        btn.innerHTML = 'Copied ✓';
        setTimeout(() => { btn.innerHTML = orig; }, 1400);
      });
    });
  }

  // Which checkboxes to activate for each preset (`paths` is the node-list base).
  // Connection checkboxes a preset auto-selects (node paths are always included).
  const PRESET_CHECKS = {
    'CPX': [],
    'ADP':  ['conn-common', 'conn-out'],
    'SRP':  ['conn-in', 'conn-out'],
    'OCP':  [],
    'LSP':  [],
    'ISP':  ['conn-in'],
    'DIP':  ['conn-common', 'conn-out'],
    'DRY':  [],
    'KISS': [],
    'LoD':  ['conn-common', 'conn-out'],
    'MISU': [],
    'CoI':  ['conn-common'],
    'YAGNI':['conn-out'],
  };

  // Short label + the `METRIC_DESCS`/`METRIC_FORMULAS` key (or an inline desc) for
  // each sort metric — used to explain the ordering in the generated prompt.
  const METRIC_INFO = {
    hk:         { label: 'HK',         name: 'Henry–Kafura (HK)' },
    sloc:       { label: 'SLOC',       name: 'Source lines (sloc)' },
    fan_out:    { label: 'fan-out',    name: 'Fan-out' },
    cyclomatic: { label: 'cyclomatic', name: 'Cyclomatic complexity' },
    cognitive:  { label: 'cognitive',  name: 'Cognitive complexity' },
    item_count: { label: 'item-count', desc: 'Number of top-level items / definitions declared in the file.' },
    cycle:      { label: 'in-cycle',   desc: 'Files that participate in at least one dependency cycle.' },
  };
  const metricHeader = m => {
    const info = METRIC_INFO[m] || { label: m };
    const desc = info.name ? (typeof METRIC_DESCS !== 'undefined' ? METRIC_DESCS[info.name] : '') : info.desc;
    const formula = info.name && typeof METRIC_FORMULAS !== 'undefined' ? METRIC_FORMULAS[info.name] : '';
    return { label: info.label, desc: desc || '', formula: formula || '' };
  };
  // Per-metric thresholds for the analyzed language (see module scope; tiers
  // `info` / `warning`, empirically calibrated, synced with the principles docs).
  const METRIC_TH = metricThresholds();
  // Default sort metric each preset selects in the dropdown.
  const PRESET_METRIC = {
    ADP: 'cycle', SRP: 'sloc', DRY: 'sloc', YAGNI: 'sloc',
    OCP: 'cyclomatic', MISU: 'cyclomatic', KISS: 'cognitive', 'CPX': 'cognitive',
    DIP: 'fan_out', LoD: 'fan_out', ISP: 'item_count', CoI: 'item_count', LSP: 'hk',
  };
  const DEFAULT_SORT = 'hk';

  // Rebind handlers each open (closures capture fresh selNodes/edges)
  const ta = document.getElementById('export-textarea');
  let activePresetKey = null;

  const internalNodes = () => allNodes.filter(n => !n.external && n.status !== 'removed');

  // For a sort metric: ALL candidate nodes sorted worst-first (so the count can
  // keep adding rows), plus how many cross the `warning` / `info` thresholds.
  // `cycle` → only nodes in a cycle (sorted by HK).
  const recoFor = metric => {
    if (metric === 'cycle') {
      const cy = window.CYCLES?.[level];
      const hk = METRIC_VAL.hk;
      const inCycle = internalNodes().filter(n => cy?.nodeCycleStatus?.get(n.id) != null)
        .sort((a, b) => hk(b) - hk(a));
      return { metric: 'cycle', sorted: inCycle, warningCount: inCycle.length, infoCount: inCycle.length };
    }
    const th = METRIC_TH[metric] || METRIC_TH.hk;
    const val = METRIC_VAL[metric] || METRIC_VAL.hk, slocV = METRIC_VAL.sloc, itemV = METRIC_VAL.item_count;
    const sorted = internalNodes()
      .sort((a, b) => val(b) - val(a) || slocV(b) - slocV(a) || itemV(b) - itemV(a));
    const warningCount = sorted.filter(n => val(n) > th.warning).length;
    const infoCount    = sorted.filter(n => val(n) > th.info).length;
    return { metric, info: th.info, warning: th.warning, sorted, warningCount, infoCount };
  };

  const recCount = overlay.querySelector('.exp-rec-count');
  const sortSel  = overlay.querySelector('.exp-sort-select');
  const activeMetric = () => sortSel.value;

  // Mirror the current controls into the URL (called from buildContent, so every
  // state change is persisted). `sel` is the FULL selection set across both sides
  // (before-only + after-only + common), not just the active side's — otherwise
  // opening the popup on one side would drop the other side's selections on reload.
  const epWriteUrl = () => epWriteUrlState({
    level,
    preset: activePresetKey,
    src:    overlay.querySelector('input[name="exp-source"]:checked')?.value,
    n:      recCount.value,
    sort:   sortSel.value,
    conn:   [...overlay.querySelectorAll('.exp-mode-cb input')]
              .filter(c => c.checked && !c.disabled).map(c => c.dataset.mode),
    sel:    [...(window._ntSelected?.[level] || [])],
  });

  const getActiveNodes = () => {
    const src = overlay.querySelector('input[name="exp-source"]:checked')?.value;
    if (src === 'recommended') {
      const count = parseInt(recCount.value) || 0;
      return recoFor(activeMetric()).sorted.slice(0, count);
    }
    return selNodes;
  };

  // Emphasis by zone: warning gets a calm text-colour highlight; info is left
  // plain (no class) to keep the UI low-sensitivity.
  const colorCount = () => {
    const r = recoFor(activeMetric());
    const c = parseInt(recCount.value) || 0;
    recCount.classList.remove('exp-rec-warn');
    if (c > 0 && c <= r.warningCount) recCount.classList.add('exp-rec-warn');
  };

  // Selecting a preset points the sort dropdown at its metric and sets the count
  // to that preset's headline recommendation (warning count if any, else info).
  const updateRecoUI = key => {
    const metric = (key && PRESET_METRIC[key]) || DEFAULT_SORT;
    sortSel.value = metric;
    const r = recoFor(metric);
    recCount.value = String(r.warningCount > 0 ? r.warningCount : r.infoCount);
    colorCount();
  };

  // Per-preset badge: warning-level count as a calm text-colour pill (a label);
  // info-level count as a plain number (no pill, no emphasis); else nothing.
  const updatePresetBadges = () => {
    overlay.querySelectorAll('.exp-preset-btn').forEach(btn => {
      const badge = btn.querySelector('.exp-preset-count');
      if (!badge) return;
      const r = recoFor(PRESET_METRIC[btn.dataset.preset] || DEFAULT_SORT);
      if (r.warningCount > 0) {
        badge.textContent = String(r.warningCount);
        badge.className = 'exp-preset-count exp-preset-count--warn';
      } else if (r.infoCount > 0) {
        badge.textContent = String(r.infoCount);
        badge.className = 'exp-preset-count exp-preset-count--info';
      } else {
        badge.textContent = '';
        badge.className = 'exp-preset-count';
      }
    });
  };

  // Wrap a preset's `title\n\nsummary` into the full instruction the AI receives:
  // intent, the summary, the link to the full principle, and a research/report
  // protocol (download & read the principle, report violations in the modules
  // below, save the report to `.code-split/<timestamp>-<CODE>.md`).
  const composePrompt = key => {
    const raw = PROMPTS[key];
    if (!raw) return '';
    const nl = raw.indexOf('\n');
    const title   = (nl >= 0 ? raw.slice(0, nl) : raw).trim();
    const summary = (nl >= 0 ? raw.slice(nl + 1) : '').trim();
    const url = principleUrl(key);
    const lines = [
      `# ${title}`,
      '',
      'I want to apply this to some modules in my system.',
      '',
      '## Summary',
      '',
      summary,
      '',
    ];
    if (url) {
      lines.push(
        `**Full principle:** [${url}](${url})`,
        '',
        'Download and read the full principle to understand it in detail. If you cannot download it, **stop the task immediately**.',
        '');
    }
    lines.push(
      '## Task',
      '',
      '- Prepare a precise, detailed estimate and a report of where the modules below violate it.',
      '- If you find more serious violations elsewhere during research, mention them in the report too.',
      '- Show a summary of the report in chat.',
      `- If any violation is found, suggest saving the report to a file as a plan for a detailed review, named \`.code-split/<YYYYMMDD-HHMMSS>-${key}.md\` (e.g. \`.code-split/20260601-191019-${key}.md\`).`,
      '',
      '**Focus the research and report primarily on the modules below.**');
    return lines.join('\n');
  };

  const buildContent = () => {
    const activeNodes = getActiveNodes();
    const activeSet   = new Set(activeNodes.map(n => n.id));
    const innerEdges  = allEdges.filter(e => activeSet.has(e.from) && activeSet.has(e.to));
    const outerEdges  = allEdges.filter(e => activeSet.has(e.from) !== activeSet.has(e.to));
    const inEdges     = outerEdges.filter(e => activeSet.has(e.to));
    const outEdges    = outerEdges.filter(e => activeSet.has(e.from));

    // A checkbox is enabled only when it would actually contribute something;
    // otherwise it is disabled and unchecked (it can't influence the output).
    const counts = { 'conn-common': innerEdges.length, 'conn-in': inEdges.length, 'conn-out': outEdges.length };
    const cbs = [...overlay.querySelectorAll('.exp-mode-cb input')];
    cbs.forEach(cb => {
      const empty = !(counts[cb.dataset.mode] > 0);
      cb.disabled = empty;
      if (empty) cb.checked = false;
      cb.closest('.exp-mode-cb')?.classList.toggle('exp-mode-cb--off', empty);
    });

    const on = id => { const c = cbs.find(c => c.dataset.mode === id); return !!(c && !c.disabled && c.checked); };
    const parts = [];
    if (activePresetKey) {
      const p = composePrompt(activePresetKey);
      if (p) parts.push(p);
    }
    // Node paths are always included (the modules the prompt is about). In
    // Recommended mode they are ordered by the sort metric, annotated with each
    // node's value, and preceded by a short explanation of that metric.
    if (activeNodes.length) {
      const src = overlay.querySelector('input[name="exp-source"]:checked')?.value;
      const path = n => (cleanPath(n.path) || n.id) + (n.line != null ? `:${n.line}` : '');
      if (src === 'recommended') {
        const m = activeMetric();
        const { label, desc, formula } = metricHeader(m);
        const val = METRIC_VAL[m];
        const lines = activeNodes.map(n => {
          const v = val ? Math.round(val(n)) : 0;
          return v > 0 ? `- \`${path(n)}\` (${label}: ${v})` : `- \`${path(n)}\``;
        }).join('\n');
        const intro = [desc, formula ? `**Formula:** \`${formula}\`` : ''].filter(Boolean).join('\n\n');
        parts.push([`## Modules ordered by ${label}`, intro, lines].filter(Boolean).join('\n\n'));
      } else {
        parts.push('## Modules\n\n' + activeNodes.map(n => `- \`${path(n)}\``).join('\n'));
      }
    }
    const edgeFmt = edges => edges.length ? edges.map(e => `- \`${pathOf(e.from)}\` → \`${pathOf(e.to)}\` (${e.kind})`).join('\n') : '_(none)_';
    if (on('conn-common')) parts.push('## Connections — common\n\n' + edgeFmt(innerEdges));
    if (on('conn-in'))     parts.push('## Connections — in\n\n'  + edgeFmt(inEdges));
    if (on('conn-out'))    parts.push('## Connections — out\n\n' + edgeFmt(outEdges));
    ta.value = parts.join('\n\n');
    const preview = document.getElementById('export-preview');
    if (preview && typeof window.snarkdown === 'function') {
      preview.innerHTML = window.snarkdown(ta.value);
    }
    epWriteUrl();
  };

  overlay.querySelectorAll('.exp-mode-cb input').forEach(cb => { cb.onchange = buildContent; });

  overlay.querySelectorAll('input[name="exp-source"]').forEach(r => { r.onchange = buildContent; });
  // Editing the recommended count implies the Recommended source.
  overlay.querySelector('.exp-rec-count').oninput = () => {
    const rec = overlay.querySelector('input[name="exp-source"][value="recommended"]');
    if (rec) rec.checked = true;
    colorCount();
    buildContent();
  };
  // Changing the sort metric re-ranks the recommended list (implies Recommended).
  sortSel.onchange = () => {
    const rec = overlay.querySelector('input[name="exp-source"][value="recommended"]');
    if (rec) rec.checked = true;
    colorCount();
    buildContent();
  };

  const applyPresetChecks = key => {
    const active = (key && PRESET_CHECKS[key]) || [];
    overlay.querySelectorAll('.exp-mode-cb input').forEach(cb => {
      cb.checked = active.includes(cb.dataset.mode);
    });
  };

  overlay.querySelectorAll('.exp-preset-btn').forEach(btn => {
    btn.onclick = () => {
      const key = btn.dataset.preset;
      if (activePresetKey === key) {
        activePresetKey = null;
        btn.classList.remove('exp-preset-btn--active');
        applyPresetChecks(null);
      } else {
        activePresetKey = key;
        overlay.querySelectorAll('.exp-preset-btn').forEach(b => b.classList.remove('exp-preset-btn--active'));
        btn.classList.add('exp-preset-btn--active');
        applyPresetChecks(key);
        // Switch to Recommended and size the count to this preset's recommendation.
        const rec = overlay.querySelector('input[name="exp-source"][value="recommended"]');
        if (rec) rec.checked = true;
      }
      updateRecoUI(activePresetKey);
      buildContent();
    };
  });

  // With nothing selected, the "Selected" radio + "OR" are hidden and the source
  // defaults to Recommended; otherwise the source defaults to Selected.
  const noSel = selNodes.length === 0;
  overlay.querySelector('input[name="exp-source"][value="selected"]')
    ?.closest('.exp-src-radio')?.style.setProperty('display', noSel ? 'none' : '');
  overlay.querySelector('.exp-source-or')?.style.setProperty('display', noSel ? 'none' : '');
  // With nothing selected there is only one source — hide its lone radio dot too,
  // leaving just the count + sort dropdown.
  overlay.querySelector('input[name="exp-source"][value="recommended"]')
    ?.style.setProperty('display', noSel ? 'none' : '');
  // Real selected-node count shown next to the "Selected" radio.
  const selCountEl = overlay.querySelector('.exp-sel-count');
  if (selCountEl) selCountEl.textContent = String(selNodes.length);

  if (restore) {
    // Restore from the URL: preset, source, count, sort metric, connection toggles.
    activePresetKey = restore.preset || null;
    overlay.querySelectorAll('.exp-preset-btn').forEach(b =>
      b.classList.toggle('exp-preset-btn--active', b.dataset.preset === activePresetKey));
    const srcVal = restore.src === 'sel' ? 'selected' : 'recommended';
    overlay.querySelectorAll('input[name="exp-source"]').forEach(r => { r.checked = r.value === srcVal; });
    if (restore.sort) sortSel.value = restore.sort;
    recCount.value = (restore.n != null && restore.n !== '') ? restore.n : '1';
    overlay.querySelectorAll('.exp-mode-cb input').forEach(c => { c.checked = restore.conn.includes(c.dataset.mode); });
  } else {
    // Fresh open: only paths, no active preset; seed the criterion from default (hk).
    activePresetKey = null;
    overlay.querySelectorAll('.exp-preset-btn').forEach(b => b.classList.remove('exp-preset-btn--active'));
    overlay.querySelectorAll('.exp-mode-cb input').forEach(c => { c.checked = false; });
    overlay.querySelectorAll('input[name="exp-source"]').forEach(r => {
      r.checked = noSel ? r.value === 'recommended' : r.value === 'selected';
    });
    updateRecoUI(null);
    recCount.value = '1';   // default: recommend 1 row
  }
  colorCount();
  updatePresetBadges(); // count badges on each preset button
  buildContent();       // also mirrors state into the URL
  // Reflect the active side in the title: Prompt Generator / … Before / … After.
  const titleEl = document.getElementById('export-popup-title');
  if (titleEl) titleEl.textContent = 'Prompt Generator' +
    (typeof viewModeSuffix === 'function' ? viewModeSuffix() : '');
  overlay.style.display = 'flex';
  document.body.style.overflow = 'hidden';
}
