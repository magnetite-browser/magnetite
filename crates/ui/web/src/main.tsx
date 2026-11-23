import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./index.css";
import { App } from "./App";
import { CaptureOverlay } from "./CaptureOverlay";
import { CommandPalette } from "./CommandPalette";
import { DevBar } from "./DevBar";
import { DeviceBar } from "./DeviceBar";
import { DownloadsPage } from "./DownloadsPage";
import { Empty } from "./Empty";
import { Licenses } from "./Licenses";
import { Notifications } from "./Notifications";
import { PanelGrip } from "./PanelGrip";
import { SplitDrop } from "./SplitDrop";
import { SettingsPage } from "./SettingsPage";
import { Shared } from "./Shared";
import { SmartFolderDialog } from "./SmartFolderDialog";
import { SpaceDialog } from "./SpaceDialog";
import { Switcher } from "./Switcher";
import { TopBar } from "./TopBar";
import { TopBarPopup } from "./TopBarPopup";
import { Welcome } from "./Welcome";

/** Each magnetite://ui page: the sidebar, the top bar, settings, a shared folder, all downloads, the welcome page, the page behind the tabs, and the small windows over the page area. */
function Route() {
  const path = location.pathname;
  if (path.startsWith("/settings")) return <SettingsPage />;
  if (path.startsWith("/shared")) return <Shared />;
  if (path.startsWith("/welcome")) return <Welcome />;
  if (path.startsWith("/downloads")) return <DownloadsPage />;
  if (path.startsWith("/licenses")) return <Licenses />;
  if (path.startsWith("/capture")) return <CaptureOverlay />;
  if (path.startsWith("/command")) return <CommandPalette />;
  if (path.startsWith("/notifications")) return <Notifications />;
  if (path.startsWith("/panel-grip")) return <PanelGrip />;
  if (path.startsWith("/splitdrop")) return <SplitDrop />;
  if (path.startsWith("/switcher")) return <Switcher />;
  if (path.startsWith("/devbar")) return <DevBar />;
  if (path.startsWith("/device")) return <DeviceBar />;
  if (path.startsWith("/smart-folder")) return <SmartFolderDialog />;
  if (path.startsWith("/space")) return <SpaceDialog />;
  if (path.startsWith("/topbar")) return <TopBar />;
  if (path.startsWith("/empty")) return <Empty />;
  if (path.startsWith("/popup")) return <TopBarPopup />;
  return <App />;
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Route />
  </StrictMode>,
);
