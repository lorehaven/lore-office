// office-theme: keeps html[data-theme] and html[data-palette] in step with the browser's storage (keys q:theme, q:palette and
// q:paper), and answers clicks on the theme menu. With no theme stored the system's scheme decides.
(function () {
  var root = document.documentElement;
  var PALETTES = ['blue', 'green', 'teal', 'purple', 'rose', 'orange'];
  function get(key) { try { return localStorage.getItem('q:' + key); } catch (e) { return null; } }
  function put(key, value) {
    try { if (value === null) localStorage.removeItem('q:' + key); else localStorage.setItem('q:' + key, value); } catch (e) { /* storage may be blocked */ }
  }
  function apply() {
    var mode = get('theme');
    if (mode === 'light' || mode === 'dark') root.setAttribute('data-theme', mode); else root.removeAttribute('data-theme');
    var palette = get('palette');
    if (PALETTES.indexOf(palette) !== -1) root.setAttribute('data-palette', palette); else root.removeAttribute('data-palette');
    // The page or sheet in light colours, apart from the theme of the rest of the screen.
    var light = get('paper') === 'light';
    if (light) root.setAttribute('data-paper', 'light'); else root.removeAttribute('data-paper');
    document.querySelectorAll('[data-theme-paper]').forEach(function (b) { b.setAttribute('aria-pressed', light ? 'true' : 'false'); });
  }
  apply();
  addEventListener('storage', apply);
  document.addEventListener('click', function (e) {
    var t = e.target && e.target.closest ? e.target : e.target && e.target.parentNode;
    if (!t || !t.closest) return;
    var mode = t.closest('[data-theme-mode]');
    if (mode) {
      var value = mode.getAttribute('data-theme-mode');
      put('theme', value === 'system' ? null : value);
      apply();
    }
    if (t.closest('[data-theme-paper]')) { put('paper', get('paper') === 'light' ? null : 'light'); apply(); }
    var palette = t.closest('[data-theme-palette]');
    if (palette) { put('palette', palette.getAttribute('data-theme-palette')); apply(); }
    var keep = t.closest('details.theme-menu');
    document.querySelectorAll('details.theme-menu[open]').forEach(function (d) { if (d !== keep) d.removeAttribute('open'); });
  });
  document.addEventListener('keydown', function (e) {
    if (e.key === 'Escape') document.querySelectorAll('details.theme-menu[open]').forEach(function (d) { d.removeAttribute('open'); });
  });
})();
