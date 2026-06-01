// ── Fixed-position tooltip (no layout shift) ─────────────────────────────────
let _ttDiv = null;
function _getTooltip() {
  if (!_ttDiv) {
    _ttDiv = document.createElement('div');
    _ttDiv.id = 'nm-tooltip';
    document.body.appendChild(_ttDiv);
  }
  return _ttDiv;
}
function _positionTooltip(tt, anchor) {
  const r  = anchor.getBoundingClientRect();
  const tw = tt.offsetWidth, th = tt.offsetHeight;
  let left = r.right + 8;
  let top  = r.top - 4;
  if (left + tw > window.innerWidth  - 10) left = r.left - tw - 8;
  if (top  + th > window.innerHeight - 10) top  = window.innerHeight - th - 10;
  if (top < 10) top = 10;
  tt.style.left = left + 'px';
  tt.style.top  = top  + 'px';
}
document.addEventListener('click', e => {
  const btn = e.target.closest('.nm-copy-btn');
  if (!btn) return;
  navigator.clipboard.writeText(btn.dataset.copy ?? '').then(() => {
    const prev = btn.textContent;
    btn.textContent = '✓';
    setTimeout(() => { btn.textContent = prev; }, 1200);
  });
});

// Click-to-copy on the whole main card (an SVG <g>, so feedback is a CSS class,
// not a textContent swap that would clobber the group's children).
document.addEventListener('click', e => {
  const card = e.target.closest('.mn-card');
  if (!card || !card.dataset.copy) return;
  navigator.clipboard.writeText(card.dataset.copy).then(() => {
    card.classList.add('copied');
    setTimeout(() => card.classList.remove('copied'), 1000);
  });
});

document.addEventListener('mouseover', e => {
  const h = e.target.closest('.nm-has-hint');
  if (h && h.dataset.nmHint) {
    const tt = _getTooltip();
    tt.textContent = `${h.textContent.trim()}: ${h.dataset.nmHint}`;
    tt.style.display = 'block';
    requestAnimationFrame(() => _positionTooltip(tt, h));
  } else if (!e.target.closest('#nm-tooltip')) {
    if (_ttDiv) _ttDiv.style.display = 'none';
  }
});

function getModal() {
  let overlay = document.getElementById('node-modal-overlay');
  if (!overlay) {
    overlay = document.createElement('div');
    overlay.id = 'node-modal-overlay';
    overlay.innerHTML =
      '<div id="node-modal">' +
        '<div id="node-modal-hdr">' +
          '<div id="node-modal-hdr-title"></div>' +
          '<button id="node-modal-close" title="Close">✕</button>' +
        '</div>' +
        '<div id="node-modal-main">' +
          '<div id="node-modal-body"></div>' +
          '<div id="node-modal-diagram"></div>' +
        '</div>' +
      '</div>';
    document.body.appendChild(overlay);
    document.getElementById('node-modal-close').addEventListener('click', closeModal);
    overlay.addEventListener('mousedown', e => { if (e.target === overlay) closeModal(); });
    document.addEventListener('keydown', e => { if (e.key === 'Escape') closeModal(); });
    document.getElementById('node-modal-diagram').addEventListener('click', e => {
      const g = e.target.closest('[data-diag-node]');
      if (!g) return;
      const nodeId = g.dataset.diagNode;
      const level  = document.querySelector('.view.active')?.dataset.view;
      if (!nodeId || !level) return;
      if (window.openModalForNode?.(nodeId, level)) window.navPush?.(level, nodeId);
    });
  }
  return overlay;
}

function closeModal() {
  window.hideMetricTooltip?.();
  const overlay = document.getElementById('node-modal-overlay');
  if (overlay) overlay.style.display = 'none';
  document.body.style.overflow = '';
  window.navPush?.(document.querySelector('.view.active')?.dataset.view ?? null, null);
}
function closeModalSilent() {
  window.hideMetricTooltip?.();
  const overlay = document.getElementById('node-modal-overlay');
  if (overlay) overlay.style.display = 'none';
  document.body.style.overflow = '';
}
