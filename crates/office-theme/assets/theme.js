// office-theme: keeps html[data-theme] and html[data-palette] in step with the browser's storage (keys q:theme, q:palette and
// q:paper), and answers clicks on the theme menu. With no theme stored the system's scheme decides.
(function () {
  var root = document.documentElement;
  var PALETTES = ['blue', 'green', 'teal', 'purple', 'rose', 'orange'];
  function get(key) { try { return localStorage.getItem('q:' + key); } catch (e) { return null; } }
  function put(key, value) {
    try { if (value === null) localStorage.removeItem('q:' + key); else localStorage.setItem('q:' + key, value); } catch (e) { /* storage may be blocked */ }
  }
  // The choices the forge services keep in cookies (the same address, so one choice holds for all of them).
  function cookie(name) {
    var m = document.cookie.match(new RegExp('(?:^|; )' + name + '=([^;]*)'));
    return m ? decodeURIComponent(m[1]) : null;
  }
  function bake(name, value) {
    document.cookie = name + '=' + encodeURIComponent(value || '') + '; max-age=' + (value ? 31536000 : 0) + '; path=/; SameSite=Lax';
  }
  // What forge says wins over what this app remembered; where it says nothing the app's own choice stands.
  function adopt() {
    var theme = cookie('qtheme');
    if (theme) put('theme', theme.indexOf('light') !== -1 ? 'light' : 'dark');
    var palette = cookie('fui_palette');
    if (palette) put('palette', palette);
    var language = cookie('qlocale');
    if (language) put('language', language);
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
  adopt();
  apply();
  addEventListener('storage', apply);
  // Changed in another service's tab: taken up when this one is looked at again.
  addEventListener('focus', function () {
    adopt();
    apply();
  });
  document.addEventListener('change', function (e) {
    if (e.target && e.target.id === 'language') bake('qlocale', e.target.value);
  });
  // An account picture that loaded replaces the icon; one that is missing leaves it.
  document.addEventListener('load', function (e) {
    var img = e.target;
    if (img && img.tagName === 'IMG' && img.parentNode && img.parentNode.classList && img.parentNode.classList.contains('fbar-avatar')) img.parentNode.classList.add('has-avatar');
  }, true);
  document.addEventListener('click', function (e) {
    var t = e.target && e.target.closest ? e.target : e.target && e.target.parentNode;
    if (!t || !t.closest) return;
    var mode = t.closest('[data-theme-mode]');
    if (mode) {
      var value = mode.getAttribute('data-theme-mode');
      put('theme', value === 'system' ? null : value);
      bake('qtheme', value === 'system' ? null : 'default-' + value);
      apply();
    }
    if (t.closest('[data-theme-paper]')) { put('paper', get('paper') === 'light' ? null : 'light'); apply(); }
    var palette = t.closest('[data-theme-palette]');
    if (palette) {
      var chosen = palette.getAttribute('data-theme-palette');
      put('palette', chosen);
      bake('fui_palette', chosen);
      apply();
    }
    var keep = t.closest('details.theme-menu, details.fbar-menu');
    document.querySelectorAll('details.theme-menu[open], details.fbar-menu[open]').forEach(function (d) { if (d !== keep) d.removeAttribute('open'); });
  });
  document.addEventListener('keydown', function (e) {
    if (e.key === 'Escape') document.querySelectorAll('details.theme-menu[open], details.fbar-menu[open]').forEach(function (d) { d.removeAttribute('open'); });
  });
})();
