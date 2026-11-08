import { useState } from "react";
import { hostOf } from "./bridge";

/**
 * Favicon for a tab. Sources, in order: the live icon of a loaded page
 * (`src`), then the engine's local cache (`/favicons/<host>.png` on this
 * page's own origin, magnetite://ui; written whenever a page was loaded
 * before), then a neutral letter tile. The sidebar never contacts a site the
 * user has not opened. The path is relative so the website's live demo, which
 * runs this code, finds its sample icons the same way.
 */
export function Favicon({ url, src, loading }: { url: string; src?: string | null; loading?: boolean }) {
  const [failed, setFailed] = useState<ReadonlySet<string>>(new Set());
  if (loading) return <span className="favicon"><span className="spinner" /></span>;
  const candidate = [src, cachedIcon(url)].find((c): c is string => !!c && loadable(c) && !failed.has(c));
  if (candidate) {
    return (
      <span className="favicon" data-img>
        <img src={candidate} alt="" onError={() => setFailed((f) => new Set(f).add(candidate))} draggable={false} />
      </span>
    );
  }
  return <span className="favicon">{hostOf(url).charAt(0).toUpperCase() || "•"}</span>;
}

/**
 * The sidebar is a secure page: plain-http images get upgraded to https and
 * can hang without ever firing an error, which would leave an empty tile.
 */
function loadable(src: string): boolean {
  const ownOrigin = src.startsWith("/") && !src.startsWith("//");
  return ownOrigin || src.startsWith("https://") || src.startsWith("magnetite://") || src.startsWith("data:");
}

function httpUrl(url: string): URL | null {
  try {
    const u = new URL(url);
    return u.protocol === "https:" || u.protocol === "http:" ? u : null;
  } catch {
    return null;
  }
}

/** Same key as `favicon::cache_key` in the engine. */
function cachedIcon(url: string): string | null {
  const u = httpUrl(url);
  return u ? `/favicons/${u.host.toLowerCase().replace(/^www\./, "").replace(":", "_")}.png` : null;
}
