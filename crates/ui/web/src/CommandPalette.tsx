// The command palette page (magnetite://ui/command?mode=…&side=…&tab=…), shown
// in a floating window centred over the page area. Every command-bar prompt
// opens here. The window follows this page's height; Esc, a pick, or a click
// anywhere else closes it.
import { useEffect, useRef, useState } from "react";
import { CommandBar } from "./CommandBar";
import { PALETTE_MODES, applyAppearance, send, type UiState } from "./bridge";

export function CommandPalette() {
  const params = new URLSearchParams(location.search);
  const mode = PALETTE_MODES.find((m) => m === params.get("mode")) ?? "new";
  const side = (["left", "right", "top", "bottom"] as const).find((s) => s === params.get("side"));
  // The favorite a new profile is for (`new_profile`); none means the active space.
  const profileFor = params.get("tab");
  const [state, setState] = useState<UiState | null>(null);
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    document.title = "Command";
    send({ cmd: "get_state" }).then((s) => {
      // Text size and density as saved, set before the bar shows.
      if (s.settings) applyAppearance(s.settings.appearance);
      setState(s);
    });
  }, []);
  useEffect(() => {
    const el = root.current;
    if (!el) return;
    const report = () => send({ cmd: "palette_resize", height: Math.ceil(el.getBoundingClientRect().height) });
    const ro = new ResizeObserver(report);
    ro.observe(el);
    return () => ro.disconnect();
  }, [state]);
  const close = () => send({ cmd: "palette_close" });
  return (
    <div ref={root}>
      {state && (
        <CommandBar
          mode={mode}
          splitSide={side}
          profileFor={profileFor}
          state={state}
          onClose={close}
          onOpenArchive={() => { send({ cmd: "show_archive" }); close(); }}
          onOpenSettings={() => { send({ cmd: "new_tab", url: "magnetite://ui/settings" }); close(); }}
        />
      )}
    </div>
  );
}
