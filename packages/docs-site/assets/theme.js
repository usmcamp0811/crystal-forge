// Apply the saved theme before styles load. Storage can be blocked on file://
// or by browser policy; dark remains the usable default in that case.
(function () {
  "use strict";
  var theme = "dark";
  try {
    var saved = localStorage.getItem("cf-docs-theme");
    if (saved === "light" || saved === "dark") { theme = saved; }
  } catch (_) { /* The theme control still works for this page. */ }
  document.documentElement.dataset.theme = theme;
})();
