// The page behind the tabs (magnetite://ui/empty). Around the pages it is the
// window's frame, with the effect of the current space's look; while no tab is
// on screen it also shows the logo and how to open a tab, or the space's own
// picture from Settings → Appearance.
import { useEffect, useLayoutEffect, useState } from "react";
import icon from "./assets/icon.png";
import { userImageUrl, windowBackground } from "./backgrounds";
import { IS_MAC, applyAppearance, lookOf, send, subscribe, type UiState } from "./bridge";
import { useSidebarShortcuts } from "./useSidebarShortcuts";

export function Empty() {
  const [state, setState] = useState<UiState | null>(null);
  useEffect(() => subscribe(setState), []);
  useEffect(() => {
    send({ cmd: "get_state" }).then((s) => window.__magnetite?.setState(s));
  }, []);
  useSidebarShortcuts(false, () => {});
  const appearance = state?.settings.appearance;
  useLayoutEffect(() => {
    if (appearance) applyAppearance(appearance);
  }, [appearance]);

  if (!state || !appearance) return <div className="h-full" />;
  // The space in front decides: each has its own look.
  const look = lookOf(appearance, state.active_space);
  const space = state.sidebar.spaces.find((s) => s.id === state.active_space);
  const frame = windowBackground("frame", state.chrome, state.active_space, look, space?.accent);
  // Pages lie over this one: only the gaps around them show.
  if (state.panes.length > 0 || state.peek) return <div className="h-full" style={frame} />;
  const stamp = look.empty_image;
  if (stamp != null) {
    return <div className="h-full bg-cover bg-center" style={{ backgroundColor: state.chrome, backgroundImage: `url("${userImageUrl("empty", state.active_space, stamp)}")` }} />;
  }
  return (
    <div className="flex h-full select-none flex-col items-center justify-center gap-3.5" style={frame}>
      <img src={icon} width={72} height={72} alt="" draggable={false} />
      <div className="text-[15px] text-[var(--muted)]">Press {IS_MAC ? "⌘T" : "Ctrl+T"} to open a tab</div>
    </div>
  );
}
