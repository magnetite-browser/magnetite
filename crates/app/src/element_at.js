// Describes the element at a point of the page, for "copy as context"
// (dev.rs): a selector that finds it again, its markup and text (cut to a
// readable length) and the computed styles that shape it. Returns null when
// nothing is there.
(x, y) => {
  const el = document.elementFromPoint(x, y);
  if (!el) return null;
  // From the element up through its ancestors, until the selector matches
  // this element alone (an id ends it at once, if it is the only one).
  const selector = (target) => {
    const parts = [];
    const alone = () => document.querySelectorAll(parts.join(" > ")).length === 1;
    for (let e = target; e && e.nodeType === 1; e = e.parentElement) {
      if (e.id) {
        parts.unshift("#" + CSS.escape(e.id));
        if (alone()) break;
        parts.shift();
      }
      let part = e.localName + [...e.classList].slice(0, 3).map((c) => "." + CSS.escape(c)).join("");
      const same = e.parentElement ? [...e.parentElement.children].filter((s) => s.localName === e.localName) : [];
      if (same.length > 1) part += `:nth-of-type(${same.indexOf(e) + 1})`;
      parts.unshift(part);
      if (alone()) break;
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
