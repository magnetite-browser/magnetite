// The device bar (magnetite://ui/device?page=<id>): above a page in device
// mode, as Chrome's device toolbar. It picks the device or a size of one's
// own ("Responsive", also resized with the handles beside the page), the
// zoom, the network speed and Save-Data, turns the device, and ends device
// mode. The engine applies each choice (crates/engine/src/device.rs).
import { useEffect, useState } from "react";
import { Close } from "./Icons";
import { send, type DeviceConfig } from "./bridge";

type Preset = { name: string; width: number; height: number; dpr: number; mobile: boolean; ua: DeviceConfig["ua"] };

const RESPONSIVE = "Responsive";

/** The devices offered, in groups, as held upright (CSS pixels). */
const GROUPS: { label: string; presets: Preset[] }[] = [
  {
    label: "Phones",
    presets: [
      { name: "iPhone SE", width: 375, height: 667, dpr: 2, mobile: true, ua: "iphone" },
      { name: "iPhone 16", width: 393, height: 852, dpr: 3, mobile: true, ua: "iphone" },
      { name: "iPhone 16 Pro Max", width: 440, height: 956, dpr: 3, mobile: true, ua: "iphone" },
      { name: "Pixel 9", width: 412, height: 915, dpr: 2.625, mobile: true, ua: "android" },
      { name: "Galaxy S24", width: 360, height: 780, dpr: 3, mobile: true, ua: "android" },
    ],
  },
  {
    label: "Tablets",
    presets: [
      { name: "iPad Mini", width: 768, height: 1024, dpr: 2, mobile: true, ua: "ipad" },
      { name: "iPad Pro 13", width: 1032, height: 1376, dpr: 2, mobile: true, ua: "ipad" },
    ],
  },
  {
    label: "Computers",
    presets: [
      { name: "Laptop", width: 1440, height: 900, dpr: 2, mobile: false, ua: null },
      { name: "Desktop", width: 1920, height: 1080, dpr: 1, mobile: false, ua: null },
    ],
  },
];
const PRESETS = GROUPS.flatMap((g) => g.presets);

const ZOOMS = [50, 75, 100, 125, 150];

const THROTTLES: [DeviceConfig["throttle"], string][] = [
  ["none", "No throttling"],
  ["fast4g", "Fast 4G"],
  ["slow4g", "Slow 4G"],
  ["3g", "3G"],
  ["offline", "Offline"],
];

/** A size typed into the bar, kept within what the engine takes. */
const size = (text: string) => Math.min(4000, Math.max(50, Math.round(Number(text)) || 50));

/** A width or height field: the value is taken on Enter or when leaving it. */
function SizeField({ label, value, onChange }: { label: string; value: number; onChange: (v: number) => void }) {
  const [text, setText] = useState(String(value));
  useEffect(() => setText(String(value)), [value]);
  const take = () => {
    const v = size(text);
    setText(String(v));
    if (v !== value) onChange(v);
  };
  return (
    <input
      className="field sm device-size"
      aria-label={label}
      inputMode="numeric"
      value={text}
      onChange={(e) => setText(e.target.value.replace(/\D/g, ""))}
      onBlur={take}
      onKeyDown={(e) => e.key === "Enter" && take()}
    />
  );
}

export function DeviceBar() {
  const page = Number(new URLSearchParams(location.search).get("page"));
  const [config, setConfig] = useState<DeviceConfig | null>(null);
  useEffect(() => {
    document.title = "Device";
    void send({ cmd: "device_get", page }).then(setConfig);
    // The engine shows sizes dragged with the handles here.
    window.__magnetiteDevice = setConfig;
    return () => {
      window.__magnetiteDevice = undefined;
    };
  }, [page]);
  if (!config) return <div className="device-bar" />;

  const change = (patch: Partial<DeviceConfig>) => {
    const next = { ...config, ...patch };
    setConfig(next);
    send({ cmd: "device_set", page, config: next });
  };
  const pick = (name: string) => {
    const preset = PRESETS.find((p) => p.name === name);
    // Responsive keeps the size it has; a device comes upright.
    if (!preset) change({ preset: RESPONSIVE });
    else change({ preset: preset.name, width: preset.width, height: preset.height, dpr: preset.dpr, mobile: preset.mobile, ua: preset.ua });
  };
  // A size of one's own is "Responsive", with the rest as it was.
  const resize = (patch: Partial<DeviceConfig>) => change({ ...patch, preset: RESPONSIVE });

  return (
    <div className="device-bar">
      <label className="device-group">
        <select className="field sm device-preset" aria-label="Dimensions" value={config.preset} onChange={(e) => pick(e.target.value)}>
          <option value={RESPONSIVE}>{RESPONSIVE}</option>
          {GROUPS.map((g) => (
            <optgroup key={g.label} label={g.label}>
              {g.presets.map((p) => <option key={p.name} value={p.name}>{p.name}</option>)}
            </optgroup>
          ))}
        </select>
      </label>
      <span className="device-group" title="Width × height in CSS pixels">
        <SizeField label="Width" value={config.width} onChange={(width) => resize({ width })} />
        <span aria-hidden>×</span>
        <SizeField label="Height" value={config.height} onChange={(height) => resize({ height })} />
      </span>
      <button className="ib sm" title="Rotate" aria-label="Rotate" onClick={() => change({ width: config.height, height: config.width })}>
        <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
          <path d="M3 12a9 9 0 1 0 3-6.7L3 8" />
          <path d="M3 3v5h5" />
        </svg>
      </button>
      <select className="field sm" aria-label="Zoom" value={config.zoom ?? "fit"} onChange={(e) => change({ zoom: e.target.value === "fit" ? null : Number(e.target.value) })}>
        <option value="fit">Fit</option>
        {ZOOMS.map((z) => <option key={z} value={z}>{z}%</option>)}
      </select>
      <select className="field sm" aria-label="Network" value={config.throttle} onChange={(e) => change({ throttle: e.target.value as DeviceConfig["throttle"] })}>
        {THROTTLES.map(([value, label]) => <option key={value} value={value}>{label}</option>)}
      </select>
      <select
        className="field sm"
        aria-label="Save-Data"
        title="The Save-Data hint sites get"
        value={config.save_data === null ? "default" : config.save_data ? "on" : "off"}
        onChange={(e) => change({ save_data: e.target.value === "default" ? null : e.target.value === "on" })}
      >
        <option value="default">Save-Data</option>
        <option value="on">Save-Data: on</option>
        <option value="off">Save-Data: off</option>
      </select>
      <button className="ib sm" title="End device mode" aria-label="End device mode" onClick={() => send({ cmd: "device_set", page, config: null })}>
        <Close width={14} height={14} />
      </button>
    </div>
  );
}
