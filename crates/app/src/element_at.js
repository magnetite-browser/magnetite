// Describes the element at a point of the page, or the first one matching
// `find` (a CSS selector), for "copy as context" and coding agents
// (dev.rs): a selector that finds it again, its markup and text (cut to a
// readable length) and the computed styles that shape it. Returns null when
// nothing is there.
(x, y, find) => {
  const el = find ? document.querySelector(find) : document.elementFromPoint(x, y);
  if (!el) return null;
  const selector = (e) => {
    const parts = [];
    while (e && e.nodeType === 1 && parts.length < 6) {
      if (e.id) {
        parts.unshift("#" + CSS.escape(e.id));
        break;
      }
      let part = e.localName + [...e.classList].slice(0, 3).map((c) => "." + CSS.escape(c)).join("");
      const same = e.parentElement ? [...e.parentElement.children].filter((s) => s.localName === e.localName) : [];
      if (same.length > 1) part += `:nth-of-type(${same.indexOf(e) + 1})`;
      parts.unshift(part);
      e = e.parentElement;
    }
    return parts.join(" > ");
  };
  const cut = (text, most) => (text.length > most ? text.slice(0, most) + "…" : text);
  const NAMES = [
    "display", "position", "box-sizing", "width", "height", "margin", "padding", "color", "background-color",
    "font-family", "font-size", "font-weight", "line-height", "text-align", "border", "border-radius", "box-shadow",
    "opacity", "z-index", "overflow", "flex-direction", "justify-content", "align-items", "gap", "grid-template-columns",
  ];
  // What every element has says nothing about this one.
  const PLAIN = new Set(["none", "normal", "auto", "static", "visible", "0px", "1", "row", "start", "content-box", "rgba(0, 0, 0, 0)"]);
  const computed = getComputedStyle(el);
  const styles = {};
  for (const name of NAMES) {
    const value = computed.getPropertyValue(name);
    if (value && !PLAIN.has(value) && !value.startsWith("0px none")) styles[name] = value;
  }
  const r = el.getBoundingClientRect();
  return {
    selector: selector(el),
    url: location.href,
    title: document.title,
    rect: [r.x, r.y, r.width, r.height],
    viewport: [innerWidth, innerHeight],
    text: cut((el.innerText || "").trim().replace(/\s+/g, " "), 300),
    html: cut(el.outerHTML, 4000),
    styles,
  };
}
