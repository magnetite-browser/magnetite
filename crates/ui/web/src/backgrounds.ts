// Backgrounds from Settings → Appearance, each space its own: the effect
// across the window or the own image in the sidebar, and the user's own
// images (stored by the app, served at /user-images/<slot>-<space>). A
// space's own icon is made here too; it is kept in the space itself.
import type { CSSProperties } from "react";
import { DEFAULT_ACCENT, accentCss, send, type Color, type Id, type SidebarSide, type SpaceLook } from "./bridge";
import { t } from "./i18n";

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

/** The window's width and height on this page (`--win-w` and `--win-h` are the app's, in window points; `--zoom` is the text size). */
const WINDOW_W = "calc(var(--win-w) / var(--zoom, 1))";
const WINDOW_H = "calc(var(--win-h) / var(--zoom, 1))";
const WINDOW_SIZE = `${WINDOW_W} ${WINDOW_H}`;

/**
 * Where `part`'s picture is held with the sidebar at `side`. `anchor`: the
 * corner of the window the part reaches, whatever the sidebar's width and
 * with or without the top bar, so a picture as large as the window held
 * there shows the part's own slice of it. `grain`: where the grain starts so
 * that its tiles count from the window's top left corner everywhere; a
 * percentage places a tile against the part's far edge, and a whole tile
 * more or less changes nothing, so "100% minus the window's size" is the
 * window's near edge. The frame and the developer bar are on the far side
 * from the sidebar; the top bar spans the window.
 */
function placement(part: WindowPart, side: SidebarSide): { anchor: string; grain: string } {
  const right = part !== "topbar" && (part === "sidebar") === (side === "right");
  const [x, grainX] = right ? ["right", `calc(100% - ${WINDOW_W})`] : ["left", "0"];
  const [y, grainY] = part === "topbar" ? ["top", "0"] : part === "devbar" ? [ABOVE, ABOVE] : ["bottom", `calc(100% - ${WINDOW_H})`];
  return { anchor: `${x} ${y}`, grain: `${grainX} ${grainY}` };
}

/**
 * The background of `part` of the window, with the sidebar docked at
 * `side`: its colour `chrome` and, over it, the effect of the space's
 * `look`. An effect is one picture across the whole window, in the space
 * colour `accent` and under the grain; each part shows its slice, so no
 * edge shows between them. An own image is the sidebar's alone and lies
 * under a veil of `chrome`, so the rows stay readable on any picture.
 */
export function windowBackground(part: WindowPart, chrome: string, space: Id, look: SpaceLook, side: SidebarSide, accent?: Color): CSSProperties {
  const flat: CSSProperties = { backgroundColor: chrome };
  const at = placement(part, side);
  // The grain repeats; the effect's layers under it span the window once.
  const effect = (...images: string[]): CSSProperties => ({
    ...flat,
    backgroundImage: [GRAIN, ...images].join(", "),
    backgroundRepeat: ["repeat", ...images.map(() => "no-repeat")].join(", "),
    backgroundSize: ["auto", ...images.map(() => WINDOW_SIZE)].join(", "),
    backgroundPosition: [at.grain, ...images.map(() => at.anchor)].join(", "),
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

/** `file` scaled down so its longer side is at most `maxSide` px. */
async function scaled(file: File, maxSide: number): Promise<HTMLCanvasElement> {
  const bitmap = await createImageBitmap(file);
  const scale = Math.min(1, maxSide / Math.max(bitmap.width, bitmap.height));
  const canvas = document.createElement("canvas");
  canvas.width = Math.round(bitmap.width * scale);
  canvas.height = Math.round(bitmap.height * scale);
  canvas.getContext("2d")!.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
  bitmap.close();
  return canvas;
}

/** `blob` as a `data:` address. */
function dataUrl(blob: Blob): Promise<string> {
  return new Promise((done, fail) => {
    const reader = new FileReader();
    reader.onload = () => done(String(reader.result));
    reader.onerror = () => fail(new Error(t("settings.appearance.picture.unreadable")));
    reader.readAsDataURL(blob);
  });
}

/**
 * `file` drawn whole into the middle of a square of `side` px, clear where
 * its shape leaves room. Read as a web page reads a picture, so an SVG
 * works too and is drawn at this size; what is stored is its picture, never
 * the SVG itself.
 */
async function squared(file: File, side: number): Promise<HTMLCanvasElement> {
  const img = new Image();
  // A `data:` address: the pages' policy lets pictures load from no `blob:`.
  img.src = await dataUrl(file);
  await img.decode().catch(() => Promise.reject(new Error(t("settings.appearance.picture.unreadable"))));
  // An SVG may name no size of its own.
  const width = img.naturalWidth || side;
  const height = img.naturalHeight || side;
  const scale = side / Math.max(width, height);
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = side;
  canvas.getContext("2d")!.drawImage(img, (side - width * scale) / 2, (side - height * scale) / 2, width * scale, height * scale);
  return canvas;
}

/** `canvas` as a WebP file of `quality` (0 to 1). */
function webp(canvas: HTMLCanvasElement, quality: number): Promise<Blob> {
  return new Promise((done, fail) => canvas.toBlob((blob) => (blob ? done(blob) : fail(new Error(t("settings.appearance.picture.unreadable")))), "image/webp", quality));
}

/** The base64 part of `blob`'s `data:` address. */
const base64 = async (blob: Blob) => (await dataUrl(blob)).split(",")[1] ?? "";

/**
 * A space's own icon is stored 64 px square: it shows at 15 to 18 points,
 * so that is sharp on every screen and at every text size. It is part of
 * the space and syncs with it, so it may weigh no more than
 * `ICON_MAX_BYTES` (`MAX_ICON_IMAGE_BYTES` in the app): an icon, not a way
 * to pass pictures around.
 */
const ICON_SIDE = 64;
const ICON_MAX_BYTES = 4 * 1024;

/** The address of a space's own icon (`Space.icon_image`), for an `<img>`. */
export const iconUrl = (image: string) => `data:image/webp;base64,${image}`;

/**
 * Make `file` (an SVG too) space `space`'s own icon, or remove it (`null`):
 * drawn into a square of `ICON_SIDE` px and stored as WebP, which leaves
 * out the file's metadata. A picture too rich in detail for
 * `ICON_MAX_BYTES` is stored coarser, then smaller.
 */
export async function storeIcon(space: Id, file: File | null): Promise<void> {
  if (!file) {
    await send({ cmd: "set_space_icon_image", id: space, data: null });
    return;
  }
  for (const side of [ICON_SIDE, 48, 32]) {
    const canvas = await squared(file, side);
    for (const quality of [0.9, 0.7, 0.5]) {
      const blob = await webp(canvas, quality);
      if (blob.size <= ICON_MAX_BYTES) {
        await send({ cmd: "set_space_icon_image", id: space, data: await base64(blob) });
        return;
      }
    }
  }
  throw new Error(t("settings.appearance.picture.too_detailed"));
}

/**
 * Store `file` as space `space`'s own image for `slot` (`null`: remove it),
 * scaled down to `maxSide` px on its longer side and stored as WebP, which
 * leaves out the file's metadata.
 */
export async function storeImage(slot: ImageSlot, space: Id, file: File | null, maxSide: number): Promise<void> {
  if (!file) {
    await send({ cmd: "set_appearance_image", slot, space, data: null });
    return;
  }
  const canvas = await scaled(file, maxSide);
  await send({ cmd: "set_appearance_image", slot, space, data: await base64(await webp(canvas, 0.85)) });
}
