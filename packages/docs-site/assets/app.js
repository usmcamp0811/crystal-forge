// Adapted from Matthew Camp's dotfiles OKF site; license in ../README.md.
// The local script index supports file:// and deployment prefixes without fetch.
(function () {
  "use strict";
  document.addEventListener("keydown", function (event) {
    if (event.key === "/" && !/^(INPUT|TEXTAREA|SELECT)$/.test(event.target.tagName) && !event.ctrlKey && !event.metaKey) {
      var box = document.querySelector("input[type=search]");
      if (box) { event.preventDefault(); box.focus(); }
    }
  });
  var root = document.documentElement;
  root.classList.add("js");
  var themeToggle = document.getElementById("theme-toggle");
  var navToggle = document.getElementById("nav-toggle");
  var nav = document.getElementById("document-nav");
  themeToggle.hidden = false;
  themeToggle.setAttribute("aria-pressed", String(root.dataset.theme === "light"));
  navToggle.hidden = false;
  function closeNav() {
    nav.removeAttribute("data-open");
    navToggle.setAttribute("aria-expanded", "false");
  }
  navToggle.addEventListener("click", function () {
    var open = navToggle.getAttribute("aria-expanded") !== "true";
    navToggle.setAttribute("aria-expanded", String(open));
    nav.toggleAttribute("data-open", open);
  });
  document.addEventListener("keydown", function (event) {
    if (event.key === "Escape" && nav.hasAttribute("data-open")) {
      closeNav();
      navToggle.focus();
    }
  });
  nav.addEventListener("click", function (event) {
    if (event.target.closest("a")) { closeNav(); }
  });

  var diagrams = [];
  document.querySelectorAll("pre > code.language-mermaid").forEach(function (code, index) {
    var node = document.createElement("figure");
    node.className = "mermaid";
    node.setAttribute("aria-label", "Diagram " + (index + 1));
    node.textContent = code.textContent;
    diagrams.push({ node: node, source: code.textContent, index: index });
    code.parentElement.replaceWith(node);
  });
  // Serialize renders, including theme changes: Mermaid has shared render state.
  // Keep original fence text so changing theme never reads SVG as diagram input.
  var pending = Promise.resolve();
  function renderDiagrams() {
    if (!window.mermaid || !diagrams.length) { return; }
    pending = pending.then(async function () {
      var renderTheme = root.dataset.theme;
      var colors = getComputedStyle(root);
      var color = function (name) { return colors.getPropertyValue(name).trim(); };
      window.mermaid.initialize({
        startOnLoad: false, securityLevel: "strict", theme: "base",
        themeVariables: {
          darkMode: root.dataset.theme === "dark",
          background: color("--cf-card-bg"), primaryColor: color("--cf-card-bg"),
          primaryTextColor: color("--cf-text-primary"), primaryBorderColor: color("--cf-brand-purple"),
          lineColor: color("--cf-text-muted"), textColor: color("--cf-text-primary"),
          secondaryColor: color("--cf-sidebar-bg"), tertiaryColor: color("--cf-page-bg"),
          fontFamily: colors.fontFamily
        }
      });
      for (var diagram of diagrams) {
        var node = diagram.node;
        node.removeAttribute("data-rendered");
        node.className = "mermaid";
        node.textContent = diagram.source;
        try {
          var result = await window.mermaid.render("diagram-" + diagram.index, diagram.source);
          node.innerHTML = result.svg;
          if (result.bindFunctions) { result.bindFunctions(node); }
          node.dataset.renderTheme = renderTheme;
          node.dataset.rendered = "true";
        } catch (error) {
          node.className = "mermaid-error";
          node.textContent = "Diagram rendering failed: " + error.message;
          console.error(error);
        }
      }
    });
  }
  themeToggle.addEventListener("click", function () {
    root.dataset.theme = root.dataset.theme === "dark" ? "light" : "dark";
    themeToggle.setAttribute("aria-pressed", String(root.dataset.theme === "light"));
    try { localStorage.setItem("cf-docs-theme", root.dataset.theme); } catch (_) { /* Storage is optional. */ }
    renderDiagrams();
  });

  renderDiagrams();

  var results = document.getElementById("results");
  if (!results || !window.OKF_INDEX) { return; }
  var query = new URLSearchParams(location.search).get("q") || "";
  document.querySelector("input[name=q]").value = query;
  var terms = query.toLowerCase().match(/(?:[^\s"]+|"[^"]*")+/g) || [];
  function matches(doc) {
    var text = [doc.t, doc.d, doc.x, doc.s].join(" ").toLowerCase();
    return terms.every(function (term) {
      var filter = /^(status|bundle|type):(.+)$/.exec(term);
      if (filter) {
        var key = { status: "s", bundle: "b", type: "y" }[filter[1]];
        return doc[key].toLowerCase() === filter[2].replace(/^"|"$/g, "");
      }
      return text.includes(term.replace(/^"|"$/g, ""));
    });
  }
  var hits = terms.length ? window.OKF_INDEX.filter(matches) : [];
  hits.sort(function (a, b) { return a.t.localeCompare(b.t); });
  document.getElementById("result-count").textContent = terms.length ? hits.length + " results" : "Enter a search term.";
  hits.slice(0, 100).forEach(function (doc) {
    var item = document.createElement("li");
    var link = document.createElement("a");
    link.href = doc.u;
    link.textContent = doc.t;
    var meta = document.createElement("p");
    meta.className = "result-meta";
    meta.textContent = doc.b + " · " + doc.y + " · Implementation status: " + doc.s;
    var description = document.createElement("p");
    description.textContent = doc.d;
    item.append(link, meta, description);
    results.appendChild(item);
  });
  if (hits.length > 100) {
    document.getElementById("result-count").textContent += ". Showing 100; narrow the search to see more.";
  }
})();
