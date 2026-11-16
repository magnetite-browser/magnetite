// The tab switcher (magnetite://ui/switcher): the tabs Ctrl+Tab goes through
// while Ctrl is held, by last use, with the one it stands on marked. It
// only shows: the app counts the presses and chooses the tab when Ctrl is
// let go, and this window never has the keyboard.
import { useEffect, useLayoutEffect, useState } from "react";
import { Favicon } from "./Favicon";
import { accentCss, applyAppearance, findTab, hostOf, send, subscribe, type UiState } from "./bridge";

export function Switcher() {
  const [state, setState] = useState<UiState | null>(null);
  useEffect(() => subscribe(setState), []);
  useEffect(() => {
    document.documentElement.dataset.window = "";
    send({ cmd: "get_state" }).then((s) => window.__magnetite?.setState(s));
  }, []);
  const appearance = state?.settings.appearance;
  useLayoutEffect(() => {
    if (appearance) applyAppearance(appearance);
  }, [appearance]);
  const accent = state && accentCss(state.sidebar.spaces.find((s) => s.id === state.active_space)?.accent);
  useLayoutEffect(() => {
    if (accent) document.documentElement.style.setProperty("--space", accent);
  }, [accent]);
  const round = state?.switcher;
  if (!state || !round) return null;
  return (
    <div className="switcher" role="listbox" aria-label="Tabs by last use">
      {round.tabs.map((id, i) => {
        const tab = findTab(state, id);
        if (!tab) return null;
        const rt = state.runtime[id];
        return (
          <div key={id} className="result" role="option" aria-selected={i === round.at} data-selected={i === round.at || undefined}>
            <Favicon url={tab.url} src={rt?.favicon} />
            <span className="min-w-0 flex-1 truncate">{tab.title || hostOf(tab.url)}</span>
            <span className="meta">{hostOf(tab.url)}</span>
          </div>
        );
      })}
    </div>
  );
}
