import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./index.css";
import { App } from "./App";
import { CaptureOverlay } from "./CaptureOverlay";
import { CommandPalette } from "./CommandPalette";
import { Licenses } from "./Licenses";
import { Notifications } from "./Notifications";
import { SplitDrop } from "./SplitDrop";
import { SettingsPage } from "./SettingsPage";
import { SmartFolderDialog } from "./SmartFolderDialog";
import { Welcome } from "./Welcome";

/** Each magnetite://ui page: the sidebar, settings, the welcome page, and the small windows over the page area. */
function Route() {
  const path = location.pathname;
  if (path.startsWith("/settings")) return <SettingsPage />;
  if (path.startsWith("/welcome")) return <Welcome />;
  if (path.startsWith("/licenses")) return <Licenses />;
  if (path.startsWith("/capture")) return <CaptureOverlay />;
  if (path.startsWith("/command")) return <CommandPalette />;
  if (path.startsWith("/notifications")) return <Notifications />;
  if (path.startsWith("/splitdrop")) return <SplitDrop />;
  if (path.startsWith("/smart-folder")) return <SmartFolderDialog />;
  return <App />;
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Route />
  </StrictMode>,
);
