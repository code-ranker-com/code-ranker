function getNavParams() {
  const p = new URLSearchParams(location.search);
  return { level: p.get('level'), node: p.get('node'), side: p.get('side') };
}
// The active diff side carried in the URL — only in diff mode (an after snapshot
// exists); review mode has a single view and omits the param.
function navSide() {
  return window.AFTER && window.viewSide ? window.viewSide : null;
}
window.navPush = function(level, nodeId) {
  const p = new URLSearchParams();
  if (level)     p.set('level', level);
  const side = navSide();
  if (side)      p.set('side', side);
  if (nodeId)    p.set('node', nodeId);
  const url = p.toString() ? '?' + p : location.pathname;
  history.pushState({ level: level ?? null, node: nodeId ?? null, side }, '', url);
};
// Update only the `side` param in place (Before/After toggle), preserving the
// level / node / Prompt-Generator params already in the URL.
window.navSetSide = function() {
  const p = new URLSearchParams(location.search);
  const side = navSide();
  if (side) p.set('side', side); else p.delete('side');
  const url = p.toString() ? '?' + p : location.pathname;
  history.replaceState({ ...(history.state || {}), side }, '', url);
};
function currentLevel() {
  return document.querySelector('.view.active')?.dataset.view ?? null;
}
function switchToLevel(target) {
  document.querySelectorAll('.view').forEach(v => v.classList.toggle('active', v.dataset.view === target));
  document.querySelectorAll('.report-switch a').forEach(l => l.classList.toggle('selected', l.dataset.view === target));
  const sec = document.querySelector('.view.active');
  if (sec && sec.dataset.rendered !== 'true' && window.gv) renderView(sec);
}
function openModalForNode(nodeId, level) {
  const nodeData = activeGraph(level).nodes.find(n => n.id === nodeId)
    ?? window.DIFF?.[level]?.nodes?.find(n => n.id === nodeId);
  if (!nodeData) return false;
  // Clear any tooltip anchored to the element we're about to replace.
  window.hideMetricTooltip?.();
  const section = document.querySelector(`.view[data-view="${level}"]`);
  const overlay = getModal();
  const mc = buildModalContent(nodeData, level);
  document.getElementById('node-modal-hdr-title').innerHTML = mc.hdr;
  document.getElementById('node-modal-body').innerHTML = mc.body;
  window.setModalDiagram(mc.diagram);
  attachModalCheckbox(nodeData, level, section);
  overlay.style.display = 'flex';
  document.body.style.overflow = 'hidden';
  return true;
}
