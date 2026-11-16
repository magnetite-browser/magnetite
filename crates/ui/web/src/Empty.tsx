// The page behind the tabs (magnetite://ui/empty), seen while no tab is on
// screen: the logo and how to open a tab, or the current space's own picture
// from Settings → Appearance.
import { useEffect, useLayoutEffect, useState } from "react";
import icon from "./assets/icon.png";
import { userImageUrl } from "./backgrounds";
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
  // The space in front decides: each has its own picture.
  const stamp = lookOf(appearance, state.active_space).empty_image;
  if (stamp != null) {
    return <div className="h-full bg-cover bg-center" style={{ backgroundColor: state.chrome, backgroundImage: `url("${userImageUrl("empty", state.active_space, stamp)}")` }} />;
  }
  return (
    <div className="flex h-full select-none flex-col items-center justify-center gap-3.5" style={{ background: state.chrome }}>
      <img src={icon} width={72} height={72} alt="" draggable={false} />
      <div className="text-[15px] text-[var(--muted)]">Press {IS_MAC ? "⌘T" : "Ctrl+T"} to open a tab</div>
    </div>
  );
}
