//! Device mode: a page with `DevTools` open shown at a phone's or tablet's
//! size, with touch and that device's browser identity. Chromium's own
//! device toolbar only exists for `DevTools` docked in a Chrome tab window,
//! which Magnetite's are not; `devtools_menu.js` adds the button and the
//! device choice to the `DevTools` toolbar instead, and the page is emulated
//! over the DevTools protocol. Device mode ends with the `DevTools`.

use cef::Rect;
use serde::Deserialize;
use serde_json::json;

use crate::engine::Engine;
use crate::events::ContentId;

/// The device a page is shown as (sent by `devtools_menu.js`).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub(crate) struct Device {
    /// CSS pixels, as the device is held (portrait or landscape).
    pub width: u32,
    pub height: u32,
    /// Device pixels per CSS pixel.
    pub dpr: f64,
    /// A touch device with a mobile browser: touch events, mobile viewport.
    pub mobile: bool,
    /// Whose browser identity the page sees; `None`: Magnetite's own.
    pub ua: Option<Ua>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Ua {
    Iphone,
    Ipad,
    Android,
}

impl Ua {
    fn string(self) -> &'static str {
        match self {
            Self::Iphone => {
                "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1"
            }
            Self::Ipad => {
                "Mozilla/5.0 (iPad; CPU OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1"
            }
            Self::Android => {
                "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/156.0.0.0 Mobile Safari/537.36"
            }
        }
    }
}

impl Device {
    /// A device from `devtools_menu.js`'s message, if its sizes make sense.
    pub(crate) fn parse(json: &str) -> Option<Self> {
        let d: Self = serde_json::from_str(json).ok()?;
        let size_ok = (100..=4000).contains(&d.width) && (100..=4000).contains(&d.height);
        (size_ok && (1.0..=4.0).contains(&d.dpr)).then_some(d)
    }

    /// Where the page goes inside `area` (the room it would have), and the
    /// scale it is drawn at: the device's size, scaled down to fit,
    /// centred across and at the top.
    pub(crate) fn fit(&self, area: &Rect) -> (Rect, f64) {
        let (w, h) = (f64::from(self.width), f64::from(self.height));
        let scale = (f64::from(area.width) / w)
            .min(f64::from(area.height) / h)
            .min(1.0);
        // Within the area's size, so the casts stay in range.
        #[allow(clippy::cast_possible_truncation)]
        let (width, height) = ((w * scale).round() as i32, (h * scale).round() as i32);
        let rect = Rect {
            x: area.x + (area.width - width) / 2,
            y: area.y,
            width,
            height,
        };
        (rect, scale)
    }
}

impl Engine {
    /// Show page `id` as `device`, or as itself again (`None`).
    pub(crate) fn set_device(&self, id: ContentId, device: Option<Device>) {
        let slot = {
            let mut inner = self.lock();
            let Some(d) = inner.docked.get_mut(&id) else {
                return;
            };
            d.device = device;
            d.device_scale = None;
            inner.contents.get(&id).and_then(|c| c.slot.clone())
        };
        self.emulate(id, device);
        if let Some(slot) = slot {
            self.place(id, &slot);
        }
    }

    /// Give page `id` the touch and browser identity of `device`, or its
    /// own back with its own size (`None`). The size itself is set where
    /// the page is placed (`device_rect`).
    pub(crate) fn emulate(&self, id: ContentId, device: Option<Device>) {
        let ignore = || Box::new(|_| {});
        let touch = device.is_some_and(|d| d.mobile);
        self.devtools(
            id,
            "Emulation.setTouchEmulationEnabled",
            &json!({ "enabled": touch, "maxTouchPoints": 5 }),
            ignore(),
        );
        self.devtools(
            id,
            "Emulation.setEmitTouchEventsForMouse",
            &json!({ "enabled": touch, "configuration": "mobile" }),
            ignore(),
        );
        // An empty identity is none: the browser's own again.
        let ua = device.and_then(|d| d.ua).map_or("", Ua::string);
        self.devtools(
            id,
            "Emulation.setUserAgentOverride",
            &json!({ "userAgent": ua }),
            ignore(),
        );
        if device.is_none() {
            self.devtools(
                id,
                "Emulation.clearDeviceMetricsOverride",
                &json!({}),
                ignore(),
            );
        }
    }

    /// Page `id`'s rectangle in device mode inside `page` (where it would
    /// go), telling the page its device size and the scale when that
    /// changed; `page` as it is without device mode.
    pub(crate) fn device_rect(&self, id: ContentId, page: Rect) -> Rect {
        let device = self.lock().docked.get(&id).and_then(|d| d.device);
        let Some(device) = device else { return page };
        let (rect, scale) = device.fit(&page);
        let changed = {
            let mut inner = self.lock();
            let d = inner.docked.get_mut(&id);
            d.is_some_and(|d| d.device_scale.replace(scale) != Some(scale))
        };
        if changed {
            let landscape = device.width > device.height;
            let params = json!({
                "width": device.width,
                "height": device.height,
                "deviceScaleFactor": device.dpr,
                "mobile": device.mobile,
                "scale": scale,
                "screenWidth": device.width,
                "screenHeight": device.height,
                "screenOrientation": {
                    "type": if landscape { "landscapePrimary" } else { "portraitPrimary" },
                    "angle": if landscape { 90 } else { 0 },
                },
            });
            self.devtools(
                id,
                "Emulation.setDeviceMetricsOverride",
                &params,
                Box::new(|_| {}),
            );
        }
        rect
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn devices_from_the_menu_are_checked() {
        let phone =
            Device::parse(r#"{"width":393,"height":852,"dpr":3,"mobile":true,"ua":"iphone"}"#);
        assert_eq!(
            phone.map(|d| (d.width, d.ua)),
            Some((393, Some(Ua::Iphone)))
        );
        assert!(
            Device::parse(r#"{"width":1280,"height":800,"dpr":1,"mobile":false,"ua":null}"#)
                .is_some()
        );
        assert!(
            Device::parse(r#"{"width":5,"height":852,"dpr":3,"mobile":true,"ua":null}"#).is_none()
        );
        assert!(
            Device::parse(r#"{"width":393,"height":852,"dpr":30,"mobile":true,"ua":null}"#)
                .is_none()
        );
        assert!(
            Device::parse(r#"{"width":393,"height":852,"dpr":3,"mobile":true,"ua":"nokia"}"#)
                .is_none()
        );
    }

    #[test]
    fn a_device_fits_into_the_page_area() {
        let phone = Device {
            width: 400,
            height: 800,
            dpr: 3.0,
            mobile: true,
            ua: None,
        };
        let area = |width, height| Rect {
            x: 100,
            y: 50,
            width,
            height,
        };
        // Room enough: its own size, centred across, at the top.
        let (r, scale) = phone.fit(&area(1000, 900));
        assert_eq!(
            (r.x, r.y, r.width, r.height, scale),
            (400, 50, 400, 800, 1.0)
        );
        // Too short: scaled down to the height.
        let (r, scale) = phone.fit(&area(1000, 400));
        assert_eq!((r.width, r.height, scale), (200, 400, 0.5));
    }
}
