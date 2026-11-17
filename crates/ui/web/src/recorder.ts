// Records a page as a video. The app sends the recorded page's pictures one
// by one; they are drawn on a canvas, and the browser's own encoder makes the
// video of it (MP4 where it can, else WebM), which goes back to the app in
// pieces and lands in the Downloads folder. Runs in a page that stays (the
// sidebar, the top bar), started by the app's `record` call.
import { send } from "./bridge";

/** The kinds of video the encoder is asked for, best first. */
const TYPES = ["video/mp4;codecs=avc1.640028", "video/webm;codecs=vp9", "video/webm"];

/** `bytes` as base64, as the app takes them. */
function base64(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result).split(",")[1] ?? "");
    reader.onerror = () => reject(reader.error ?? new Error("could not read"));
    reader.readAsDataURL(blob);
  });
}

/** A JPEG in base64 as a picture to draw. */
function picture(jpeg: string): Promise<ImageBitmap> {
  const bytes = Uint8Array.from(atob(jpeg), (c) => c.charCodeAt(0));
  return createImageBitmap(new Blob([bytes], { type: "image/jpeg" }));
}

/**
 * Make the video of the recording the app just asked for. Several pages hear
 * the request; the app takes the first that answers, and the others stop
 * here. A page that is not on screen waits a moment, so one that is wins.
 */
export async function record() {
  if (document.visibilityState !== "visible") await new Promise((r) => setTimeout(r, 150));
  try {
    await send({ cmd: "dev_record_start" });
  } catch {
    return;
  }
  const type = TYPES.find((t) => MediaRecorder.isTypeSupported(t));
  const canvas = document.createElement("canvas");
  const ctx = canvas.getContext("2d");
  let recorder: MediaRecorder | null = null;
  // The video's pieces reach the app in order.
  let sent: Promise<unknown> = Promise.resolve();
  try {
    for (;;) {
      const next = await send({ cmd: "dev_record_frame" });
      if (!next.frame || !type || !ctx) break;
      const frame = await picture(next.frame);
      if (!recorder) {
        // The first picture sets the video's size: even numbers, as encoders want.
        canvas.width = frame.width & ~1;
        canvas.height = frame.height & ~1;
        recorder = new MediaRecorder(canvas.captureStream(30), { mimeType: type, videoBitsPerSecond: 8_000_000 });
        recorder.ondataavailable = (e) => {
          if (e.data.size === 0) return;
          sent = sent.then(async () => send({ cmd: "dev_record_data", data: await base64(e.data), mp4: type.startsWith("video/mp4") })).catch(() => {});
        };
        recorder.start(1000);
      }
      // A page that changed its size is fitted in.
      ctx.drawImage(frame, 0, 0, canvas.width, canvas.height);
      frame.close();
    }
    if (recorder && ctx) {
      // Drawn once more, so the video runs up to now and not to the last change.
      ctx.drawImage(canvas, 0, 0);
      const stopped = new Promise((resolve) => recorder?.addEventListener("stop", resolve, { once: true }));
      recorder.stop();
      await stopped;
      await sent;
    }
  } finally {
    await send({ cmd: "dev_record_end" });
  }
}
