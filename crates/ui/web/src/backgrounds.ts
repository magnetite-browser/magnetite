// Backgrounds from Settings → Appearance, each space its own: the effect
// across the window or the own image in the sidebar, and the user's own
// images (stored by the app, served at /user-images/<slot>-<space>).
import type { CSSProperties } from "react";
import { DEFAULT_ACCENT, accentCss, send, type Color, type Id, type SpaceLook } from "./bridge";

export type ImageSlot = "empty" | "sidebar";

/** Address of space `space`'s stored image for `slot`; `stamp` changes with every new image. */
export const userImageUrl = (slot: ImageSlot, space: Id, stamp: number) => `/user-images/${slot}-${space}?v=${stamp}`;

/** Fine white noise, the same for every space. */
const GRAIN = `url("data:image/svg+xml,${encodeURIComponent(
  "<svg xmlns='http://www.w3.org/2000/svg' width='160' height='160'>" +
    "<filter id='n'><feTurbulence type='fractalNoise' baseFrequency='0.9' numOctaves='2' stitchTiles='stitch'/>" +
    "<feColorMatrix values='0 0 0 0 1  0 0 0 0 1  0 0 0 0 1  0.7 0 0 0 0'/></filter>" +
    "<rect width='100%' height='100%' filter='url(#n)' opacity='0.22'/></svg>",
)}")`;

/** `c` with its hue turned by `degrees`, as CSS with `alpha`. */
function turned(c: Color | undefined, degrees: number, alpha: number): string {
  const { red: r, green: g, blue: b } = c ?? DEFAULT_ACCENT;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const d = max - min;
  const l = (max + min) / 2;
  const s = d === 0 ? 0 : d / (1 - Math.abs(2 * l - 1));
  const h = d === 0 ? 0 : max === r ? ((g - b) / d) % 6 : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
  return `hsl(${Math.round(h * 60 + degrees)} ${Math.round(s * 100)}% ${Math.round(l * 100)}% / ${alpha})`;
}

/** A soft patch of `colour` around `at`, `size` wide and tall. */
const patch = (colour: string, at: string, size: string) => `radial-gradient(${size} at ${at}, ${colour}, transparent 72%)`;

/**
 * A part of the window that paints its own background: the sidebar (docked
 * or floating), the frame around the pages, the top bar.
 */
export type WindowPart = "sidebar" | "frame" | "topbar" | "devbar";

/** How far above the developer bar the window begins: the top bar's height while it is on (`--above`, set by the bar). */
const ABOVE = "var(--above, 0px)";

/**
 * The corner of the window each part reaches, whatever the sidebar's width
 * and with or without the top bar: a picture as large as the window, held at
 * that corner, shows the part's own slice of it.
 */
const ANCHOR: Record<WindowPart, string> = { sidebar: "left bottom", frame: "right bottom", topbar: "left top", devbar: `right ${ABOVE}` };

/** The window's width and height on this page (`--win-w` and `--win-h` are the app's, in window points; `--zoom` is the text size). */
const WINDOW_W = "calc(var(--win-w) / var(--zoom, 1))";
const WINDOW_H = "calc(var(--win-h) / var(--zoom, 1))";
const WINDOW_SIZE = `${WINDOW_W} ${WINDOW_H}`;

/**
 * Where the grain starts in each part so that its tiles count from the
 * window's top left corner everywhere. A percentage places a tile against
 * the part's far edge, and a whole tile more or less changes nothing, so
 * "100% minus the window's size" is the window's near edge.
 */
const GRAIN_AT: Record<WindowPart, string> = {
  sidebar: `0 calc(100% - ${WINDOW_H})`,
  frame: `calc(100% - ${WINDOW_W}) calc(100% - ${WINDOW_H})`,
  topbar: "0 0",
  devbar: `calc(100% - ${WINDOW_W}) ${ABOVE}`,
};

/**
 * The background of `part` of the window: its colour `chrome` and, over it,
 * the effect of the space's `look`. An effect is one picture across the
 * whole window, in the space colour `accent` and under the grain; each part
 * shows its slice, so no edge shows between them. An own image is the
 * sidebar's alone and lies under a veil of `chrome`, so the rows stay
 * readable on any picture.
 */
export function windowBackground(part: WindowPart, chrome: string, space: Id, look: SpaceLook, accent?: Color): CSSProperties {
  const flat: CSSProperties = { backgroundColor: chrome };
  // The grain repeats; the effect's layers under it span the window once.
  const effect = (...images: string[]): CSSProperties => ({
    ...flat,
    backgroundImage: [GRAIN, ...images].join(", "),
    backgroundRepeat: ["repeat", ...images.map(() => "no-repeat")].join(", "),
    backgroundSize: ["auto", ...images.map(() => WINDOW_SIZE)].join(", "),
    backgroundPosition: [GRAIN_AT[part], ...images.map(() => ANCHOR[part])].join(", "),
  });
  switch (look.sidebar_effect) {
    case "grain":
      return effect();
    case "glow":
      return effect(patch(accentCss(accent, 0.34), "0% 0%", "45% 60%"), patch(accentCss(accent, 0.22), "100% 100%", "45% 55%"));
    case "aurora":
      return effect(patch(accentCss(accent, 0.32), "0% 0%", "45% 50%"), patch(turned(accent, 70, 0.24), "100% 30%", "40% 50%"), patch(turned(accent, -60, 0.26), "20% 100%", "50% 45%"));
    case "fade":
      return effect(`linear-gradient(${accentCss(accent, 0.3)}, transparent 70%)`);
    case "image": {
      if (part !== "sidebar" || look.sidebar_image == null) return flat;
      const veil = `color-mix(in srgb, ${chrome} 62%, transparent)`;
      return { ...flat, backgroundImage: `linear-gradient(${veil}, ${veil}), url("${userImageUrl("sidebar", space, look.sidebar_image)}")`, backgroundSize: "cover", backgroundPosition: "center" };
    }
    default:
      return flat;
  }
}

/**
 * Store `file` as space `space`'s image for `slot`: scaled down to `maxSide` points on
 * its longer side and re-encoded as WebP, which also drops the picture's
 * metadata (place, camera). `null` removes the stored image.
 */
export async function storeImage(slot: ImageSlot, space: Id, file: File | null, maxSide: number): Promise<void> {
  if (!file) {
    await send({ cmd: "set_appearance_image", slot, space, data: null });
    return;
  }
  const bitmap = await createImageBitmap(file);
  const scale = Math.min(1, maxSide / Math.max(bitmap.width, bitmap.height));
  const canvas = document.createElement("canvas");
  canvas.width = Math.round(bitmap.width * scale);
  canvas.height = Math.round(bitmap.height * scale);
  canvas.getContext("2d")!.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
  bitmap.close();
  const blob = await new Promise<Blob | null>((done) => canvas.toBlob(done, "image/webp", 0.85));
  if (!blob) throw new Error("That picture could not be read");
  const url = await new Promise<string>((done, fail) => {
    const reader = new FileReader();
    reader.onload = () => done(String(reader.result));
    reader.onerror = () => fail(new Error("That picture could not be read"));
    reader.readAsDataURL(blob);
  });
  await send({ cmd: "set_appearance_image", slot, space, data: url.slice(url.indexOf(",") + 1) });
}
