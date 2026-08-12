// Scroll-spy for the sidebar, mirroring the dashboard's nav-tab-active state.
// External file (not inline) because the node's CSP is script-src 'self'.
(function () {
  var links = Array.prototype.slice.call(
    document.querySelectorAll('.sidebar-nav .sidebar-nav-item[href^="#"]')
  )
  if (!links.length || !('IntersectionObserver' in window)) return

  var sections = links
    .map(function (a) { return document.getElementById(a.getAttribute('href').slice(1)) })
    .filter(Boolean)

  function activate(id) {
    links.forEach(function (a) {
      a.classList.toggle('nav-tab-active', a.getAttribute('href') === '#' + id)
    })
  }

  var visible = {}
  var observer = new IntersectionObserver(function (entries) {
    entries.forEach(function (e) { visible[e.target.id] = e.isIntersecting })
    // Topmost section currently on screen wins, so the highlight tracks reading position.
    for (var i = 0; i < sections.length; i++) {
      if (visible[sections[i].id]) { activate(sections[i].id); return }
    }
  }, { rootMargin: '-10% 0px -70% 0px', threshold: 0 })

  sections.forEach(function (s) { observer.observe(s) })
})()
