import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./index.css";
import { loadLanguage } from "./i18n";

// The page's language first: the pages' modules may call `t` as they load.
await loadLanguage(document.documentElement.lang);
const { Route } = await import("./Route");

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Route />
  </StrictMode>,
);
