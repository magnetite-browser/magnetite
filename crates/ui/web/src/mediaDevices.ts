// The cameras and microphones connected now, reported to the app for the
// order in Settings → Permissions. The main page reads them: a Settings tab
// gets their names empty (`MediaDevices` in crates/app/src/app.rs).
import { send, type MediaDevice } from "./bridge";

/** The devices of `kind` as the app keeps them, each once: the system's default microphone as `default`, every other device by its name, which is what Chromium's order goes by. */
function devicesOf(all: MediaDeviceInfo[], kind: MediaDeviceKind): MediaDevice[] {
  const devices = all.filter((d) => d.kind === kind && d.label).map((d) => ({ id: d.deviceId === "default" ? "default" : d.label, label: d.label }));
  return devices.filter((d, i) => devices.findIndex((other) => other.id === d.id) === i);
}

/** Report the devices now and whenever they change; returns what stops that. */
export function reportMediaDevices() {
  const media = navigator.mediaDevices;
  if (!media) return () => {};
  const report = () =>
    void media
      .enumerateDevices()
      .then((all) => send({ cmd: "media_devices", cameras: devicesOf(all, "videoinput"), microphones: devicesOf(all, "audioinput") }))
      .catch(() => {});
  report();
  media.addEventListener("devicechange", report);
  return () => media.removeEventListener("devicechange", report);
}
