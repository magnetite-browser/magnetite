//! Device mode: a page with `DevTools` open shown as a phone, tablet or a
//! size of one's own, with the device's touch, browser identity, network
//! speed and Save-Data. Chromium's own device toolbar only exists for
//! `DevTools` docked in a Chrome tab window, which Magnetite's are not.
//!
//! The button in the `DevTools` toolbar (`devtools_menu.js`) turns it on and
//! off. While on, a bar of Magnetite's own (`magnetite://ui/device`) sits
//! above the page in the page area with the choices; the page sits below
//! it at the device's size (scaled to the zoom), with handles on its right,
//! bottom and corner to resize it in "Responsive". The page is emulated over
//! the DevTools protocol. Device mode ends with the `DevTools`.

use cef::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::client::MagnetiteClient;
use crate::engine::Engine;
use crate::events::{ContentId, DividerAction};

/// Height of the bar above the page.
const BAR_HEIGHT: i32 = 36;
/// Room around the page inside its area, and the handles' thickness.
const PAD: i32 = 8;
const HANDLE: i32 = 10;
/// The sizes a device may have (CSS pixels).
const MIN_SIZE: u32 = 50;
const MAX_SIZE: u32 = 4000;

/// What the page is shown as: sent by the bar, kept by the engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct DeviceConfig {
    /// The bar's name for the choice ("Responsive", "iPhone SE", …).
    pub preset: String,
    /// CSS pixels, as the device is held (portrait or landscape).
    pub width: u32,
    pub height: u32,
    /// Device pixels per CSS pixel.
    pub dpr: f64,
    /// A touch device with a mobile browser: touch events, mobile viewport.
    pub mobile: bool,
    /// Whose browser identity the page sees; `None`: Magnetite's own.
    pub ua: Option<Ua>,
    /// Percent; `None`: as large as fits.
    pub zoom: Option<u32>,
    pub throttle: Throttle,
    /// The `Save-Data` hint; `None`: the browser's own.
    pub save_data: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Ua {
    Iphone,
    Ipad,
    Android,
}

/// Network speed, as Chrome's presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Throttle {
    #[default]
    None,
    Fast4g,
    Slow4g,
    #[serde(rename = "3g")]
    ThreeG,
    Offline,
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
                "Mozilla/5.0 (Linux; Android 15; Pixel 9) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/156.0.0.0 Mobile Safari/537.36"
            }
        }
    }
}

impl Throttle {
    /// `Network.emulateNetworkConditions` for it: latency in ms, down and
    /// up in bytes per second (-1: no limit).
    fn conditions(self) -> Value {
        let kbit = |k: f64| k * 1000.0 / 8.0;
        let (offline, latency, down, up) = match self {
            Self::None => (false, 0.0, -1.0, -1.0),
            Self::Fast4g => (false, 165.0, kbit(9000.0), kbit(1500.0)),
            Self::Slow4g => (false, 562.5, kbit(1600.0), kbit(750.0)),
            Self::ThreeG => (false, 2000.0, kbit(400.0), kbit(400.0)),
            Self::Offline => (true, 0.0, 0.0, 0.0),
        };
        json!({ "offline": offline, "latency": latency, "downloadThroughput": down, "uploadThroughput": up })
    }
}

impl Default for DeviceConfig {
    /// What device mode starts with the first time: "Responsive" at a
    /// phone's size.
    fn default() -> Self {
        Self {
            preset: "Responsive".into(),
            width: 390,
            height: 844,
            dpr: 2.0,
            mobile: true,
            ua: None,
            zoom: None,
            throttle: Throttle::None,
            save_data: None,
        }
    }
}

impl DeviceConfig {
    /// A configuration from the bar, if its numbers make sense.
    fn parse(value: Value) -> Option<Self> {
        let c: Self = serde_json::from_value(value).ok()?;
        let sizes =
            (MIN_SIZE..=MAX_SIZE).contains(&c.width) && (MIN_SIZE..=MAX_SIZE).contains(&c.height);
        let zoom = c.zoom.is_none_or(|z| (25..=200).contains(&z));
        (sizes && zoom && (1.0..=4.0).contains(&c.dpr) && c.preset.len() <= 60).then_some(c)
    }

    /// Where the page goes inside `area` and the scale it is drawn at: the
    /// device's size at the zoom (or as large as fits, at most its own
    /// size), centred across, at the top; never larger than `area`.
    fn fit(&self, area: &Rect) -> (Rect, f64) {
        let (w, h) = (f64::from(self.width), f64::from(self.height));
        let fits = (f64::from(area.width) / w).min(f64::from(area.height) / h);
        let scale = self
            .zoom
            .map_or(fits.min(1.0), |z| f64::from(z) / 100.0)
            .max(0.01);
        // Within the area's size, so the casts stay in range.
        #[allow(clippy::cast_possible_truncation)]
        let (width, height) = (
            ((w * scale).round() as i32).min(area.width).max(1),
            ((h * scale).round() as i32).min(area.height).max(1),
        );
        let rect = Rect {
            x: area.x + (area.width - width) / 2,
            y: area.y,
            width,
            height,
        };
        (rect, scale)
    }
}

/// A resize handle beside the page in "Responsive".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Edge {
    Right,
    Bottom,
    Corner,
}

/// Device mode of one page.
pub(crate) struct DeviceMode {
    pub config: DeviceConfig,
    /// The bar above the page, and its window once made.
    pub bar: BrowserView,
    pub bar_window: Option<Window>,
    /// What the page was last told: size, pixel ratio, scale, mobile.
    told: Option<(u32, u32, f64, f64, bool)>,
    /// Where the page sits and its scale, for the handles.
    layout: Option<(Rect, f64)>,
    /// The handles beside the page ("Responsive" only).
    pub handles: Vec<(Rect, Edge)>,
    /// The scale when the drag of a handle began: held until it ends, so
    /// the size follows the pointer evenly even while "Fit" shrinks the page.
    drag_scale: Option<f64>,
}

impl Engine {
    /// Device mode for page `id` on or off: on with the device used last
    /// (`DeviceConfig::default` the first time). From the `DevTools` button.
    pub(crate) fn toggle_device(&self, id: ContentId) {
        let on = self
            .lock()
            .docked
            .get(&id)
            .is_some_and(|d| d.device.is_some());
        if on {
            self.device_off(id);
        } else {
            let config = self.lock().last_device.clone().unwrap_or_default();
            self.device_on(id, &config);
        }
    }

    fn device_on(&self, id: ContentId, config: &DeviceConfig) {
        if !self.lock().docked.contains_key(&id) {
            return;
        }
        let mut client = MagnetiteClient::overlay();
        let mut delegate = BarViewDelegate::new();
        let url = format!("magnetite://ui/device?page={id}");
        let Some(bar) = browser_view_create(
            Some(&mut client),
            Some(&CefString::from(url.as_str())),
            Some(&BrowserSettings {
                background_color: self.lock().chrome_color,
                ..Default::default()
            }),
            None,
            None,
            Some(&mut delegate),
        ) else {
            return;
        };
        {
            let mut inner = self.lock();
            inner.last_device = Some(config.clone());
            let Some(d) = inner.docked.get_mut(&id) else {
                return;
            };
            d.device = Some(DeviceMode {
                config: config.clone(),
                bar: bar.clone(),
                bar_window: None,
                told: None,
                layout: None,
                handles: Vec::new(),
                drag_scale: None,
            });
        }
        self.emulate(id, Some(config), None);
        let mut wd = BarWindowDelegate::new(id, bar);
        window_create_top_level(Some(&mut wd));
        self.replace_page(id);
        self.tell_devtools_device(id, true);
    }

    /// Device mode for page `id` off: the page as itself again.
    pub(crate) fn device_off(&self, id: ContentId) {
        let mode = self
            .lock()
            .docked
            .get_mut(&id)
            .and_then(|d| d.device.take());
        let Some(mode) = mode else { return };
        self.end_device(id, mode);
        self.replace_page(id);
        self.tell_devtools_device(id, false);
    }

    /// Undo device mode `mode` of page `id` (its `DevTools` closing, or off).
    pub(crate) fn end_device(&self, id: ContentId, mode: DeviceMode) {
        if let Some(w) = mode.bar_window {
            w.close();
        }
        self.emulate(id, None, Some(&mode.config));
        self.show_dividers();
    }

    /// The bar's question: page `id`'s device, `null` when device mode is off.
    pub fn device_config(&self, id: ContentId) -> Value {
        let inner = self.lock();
        let config = inner
            .docked
            .get(&id)
            .and_then(|d| d.device.as_ref())
            .map(|m| &m.config);
        serde_json::to_value(config).unwrap_or(Value::Null)
    }

    /// The bar's choice for page `id`: another device or setting, or
    /// `null` to end device mode. A choice that makes no sense is ignored.
    pub fn set_device_config(&self, id: ContentId, value: Value) {
        if value.is_null() {
            self.device_off(id);
            return;
        }
        let Some(config) = DeviceConfig::parse(value) else {
            return;
        };
        let before = {
            let mut inner = self.lock();
            inner.last_device = Some(config.clone());
            let mode = inner.docked.get_mut(&id).and_then(|d| d.device.as_mut());
            mode.map(|m| std::mem::replace(&mut m.config, config.clone()))
        };
        if let Some(before) = before {
            self.emulate(id, Some(&config), Some(&before));
            self.replace_page(id);
        }
    }

    /// Give page `id` what `config` asks for apart from its size (touch,
    /// identity, network, Save-Data), or its own back (`None`); only what
    /// differs from `before`, the configuration in force.
    fn emulate(&self, id: ContentId, config: Option<&DeviceConfig>, before: Option<&DeviceConfig>) {
        let ignore = || Box::new(|_| {});
        let changed =
            |f: &dyn Fn(Option<&DeviceConfig>) -> Value| before.is_none() || f(config) != f(before);
        let touch = |c: Option<&DeviceConfig>| json!(c.is_some_and(|c| c.mobile));
        if changed(&touch) {
            self.touch(id, config.is_some_and(|c| c.mobile));
        }
        // An empty identity is none: the browser's own again.
        let ua = |c: Option<&DeviceConfig>| json!(c.and_then(|c| c.ua).map_or("", Ua::string));
        if changed(&ua) {
            self.devtools(
                id,
                "Emulation.setUserAgentOverride",
                &json!({ "userAgent": ua(config) }),
                ignore(),
            );
        }
        let throttle =
            |c: Option<&DeviceConfig>| c.map_or(Throttle::None, |c| c.throttle).conditions();
        if changed(&throttle) {
            self.devtools(id, "Network.enable", &json!({}), ignore());
            self.devtools(
                id,
                "Network.emulateNetworkConditions",
                &throttle(config),
                ignore(),
            );
        }
        // Without a value: the browser's own again.
        let save = |c: Option<&DeviceConfig>| json!(c.and_then(|c| c.save_data));
        if changed(&save) {
            let params = match config.and_then(|c| c.save_data) {
                Some(on) => json!({ "dataSaverEnabled": on }),
                None => json!({}),
            };
            self.devtools(id, "Emulation.setDataSaverOverride", &params, ignore());
        }
        if config.is_none() {
            self.devtools(
                id,
                "Emulation.clearDeviceMetricsOverride",
                &json!({}),
                ignore(),
            );
        }
    }

    /// Page `id` takes the mouse for touches (`mobile`), unless the element
    /// picker of its `DevTools` is on: a page that gets touches shows no
    /// element under the pointer. Chromium's device mode does the same.
    fn touch(&self, id: ContentId, mobile: bool) {
        let inspecting = self.lock().docked.get(&id).is_some_and(|d| d.inspecting);
        let on = mobile && !inspecting;
        let ignore = || Box::new(|_| {});
        self.devtools(
            id,
            "Emulation.setTouchEmulationEnabled",
            &json!({ "enabled": on, "maxTouchPoints": 5 }),
            ignore(),
        );
        self.devtools(
            id,
            "Emulation.setEmitTouchEventsForMouse",
            &json!({ "enabled": on, "configuration": "mobile" }),
            ignore(),
        );
    }

    /// The element picker of page `id`'s `DevTools` went on or off: in
    /// device mode the touches stop meanwhile.
    pub(crate) fn device_inspecting(&self, id: ContentId, on: bool) {
        let mobile = {
            let mut inner = self.lock();
            let Some(d) = inner.docked.get_mut(&id) else {
                return;
            };
            if std::mem::replace(&mut d.inspecting, on) == on {
                return;
            }
            d.device.as_ref().map(|m| m.config.mobile)
        };
        if mobile == Some(true) {
            self.touch(id, true);
        }
    }

    /// Lay page `id` out again where it is.
    fn replace_page(&self, id: ContentId) {
        let slot = self.lock().contents.get(&id).and_then(|c| c.slot.clone());
        if let Some(slot) = slot {
            self.place(id, &slot);
        }
    }

    /// Page `id`'s rectangle inside `page` (where it would go): in device
    /// mode below the bar at the device's size, with the bar and handles
    /// placed and the page told its size when that changed; else `page`.
    pub(crate) fn device_rect(&self, id: ContentId, page: Rect) -> Rect {
        let (config, bar) = {
            let inner = self.lock();
            let mode = inner.docked.get(&id).and_then(|d| d.device.as_ref());
            match mode {
                Some(m) => (m.config.clone(), m.bar_window.clone()),
                None => return page,
            }
        };
        let bar_rect = Rect {
            height: BAR_HEIGHT,
            ..page.clone()
        };
        if let Some(w) = bar {
            w.set_bounds(Some(&bar_rect));
        }
        // Room for the handles on both sides, so the page stays centred.
        let area = Rect {
            x: page.x + PAD + HANDLE,
            y: page.y + BAR_HEIGHT + PAD,
            width: (page.width - 2 * (PAD + HANDLE)).max(1),
            height: (page.height - BAR_HEIGHT - 2 * PAD - HANDLE).max(1),
        };
        let (rect, scale) = config.fit(&area);
        let handles = if config.preset == "Responsive" {
            let (right, bottom) = (rect.x + rect.width, rect.y + rect.height);
            vec![
                (
                    Rect {
                        x: right,
                        y: rect.y,
                        width: HANDLE,
                        height: rect.height,
                    },
                    Edge::Right,
                ),
                (
                    Rect {
                        x: rect.x,
                        y: bottom,
                        width: rect.width,
                        height: HANDLE,
                    },
                    Edge::Bottom,
                ),
                (
                    Rect {
                        x: right,
                        y: bottom,
                        width: HANDLE,
                        height: HANDLE,
                    },
                    Edge::Corner,
                ),
            ]
        } else {
            Vec::new()
        };
        let metrics = (
            config.width,
            config.height,
            config.dpr,
            scale,
            config.mobile,
        );
        let tell = {
            let mut inner = self.lock();
            let mode = inner.docked.get_mut(&id).and_then(|d| d.device.as_mut());
            mode.is_some_and(|m| {
                m.layout = Some((rect.clone(), scale));
                m.handles = handles;
                m.told.replace(metrics) != Some(metrics)
            })
        };
        if tell {
            let landscape = config.width > config.height;
            let params = json!({
                "width": config.width,
                "height": config.height,
                "deviceScaleFactor": config.dpr,
                "mobile": config.mobile,
                "scale": scale,
                "screenWidth": config.width,
                "screenHeight": config.height,
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

    /// A resize handle of page `id` was dragged to `(x, y)` (screen DIP):
    /// "Responsive" takes the new size, and the bar shows it. Let go, the
    /// scale held during the drag is free again.
    #[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
    pub(crate) fn device_handle(
        &self,
        id: ContentId,
        edge: Edge,
        x: i32,
        y: i32,
        action: DividerAction,
    ) {
        let config = {
            let mut inner = self.lock();
            let Some(mode) = inner.docked.get_mut(&id).and_then(|d| d.device.as_mut()) else {
                return;
            };
            let DividerAction::Move(_) = action else {
                mode.drag_scale = None;
                return;
            };
            let Some((rect, scale)) = mode.layout.clone() else {
                return;
            };
            let scale = *mode.drag_scale.get_or_insert(scale);
            let size = |px: f64| {
                // Clamped to the device sizes, so the cast stays in range.
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let css = (px / scale)
                    .round()
                    .clamp(f64::from(MIN_SIZE), f64::from(MAX_SIZE))
                    as u32;
                css
            };
            // The page stays centred: its width grows on both sides.
            let centre = f64::from(rect.x) + f64::from(rect.width) / 2.0;
            if edge != Edge::Bottom {
                mode.config.width = size((f64::from(x) - centre) * 2.0);
            }
            if edge != Edge::Right {
                mode.config.height = size(f64::from(y - rect.y));
            }
            let config = mode.config.clone();
            inner.last_device = Some(config.clone());
            config
        };
        self.replace_page(id);
        self.tell_bar(id, &config);
    }

    /// Show the bar of page `id` the device as it is now.
    fn tell_bar(&self, id: ContentId, config: &DeviceConfig) {
        let bar = self
            .lock()
            .docked
            .get(&id)
            .and_then(|d| d.device.as_ref())
            .map(|m| m.bar.clone());
        let json = serde_json::to_string(config).unwrap_or_default();
        if let Some(frame) = bar.and_then(|b| b.browser()).and_then(|b| b.main_frame()) {
            let js = format!("window.__magnetiteDevice?.({json})");
            frame.execute_java_script(Some(&CefString::from(js.as_str())), None, 0);
        }
    }

    /// Show or hide page `id`'s device bar with the page.
    pub(crate) fn show_device_bar(&self, id: ContentId, show: bool) {
        let bar = self
            .lock()
            .docked
            .get(&id)
            .and_then(|d| d.device.as_ref())
            .and_then(|m| m.bar_window.clone());
        if let Some(w) = bar {
            if show {
                w.show();
                w.bring_to_top();
            } else {
                w.hide();
            }
        }
    }

    /// Tell page `id`'s `DevTools` whether device mode is on (their button).
    fn tell_devtools_device(&self, id: ContentId, on: bool) {
        let view = self.lock().docked.get(&id).map(|d| d.view.clone());
        if let Some(frame) = view.and_then(|v| v.browser()).and_then(|b| b.main_frame()) {
            let js = format!("window.__magnetiteDeviceOn = {on}; window.__magnetiteScan?.();");
            frame.execute_java_script(Some(&CefString::from(js.as_str())), None, 0);
        }
    }
}

wrap_browser_view_delegate! {
    struct BarViewDelegate {}
    impl ViewDelegate {}
    impl BrowserViewDelegate {
        fn browser_runtime_style(&self) -> RuntimeStyle { RuntimeStyle::ALLOY }
    }
}

wrap_window_delegate! {
    struct BarWindowDelegate {
        id: ContentId,
        view: BrowserView,
    }
    impl ViewDelegate {}
    impl PanelDelegate {}
    impl WindowDelegate {
        fn on_window_created(&self, window: Option<&mut Window>) {
            let Some(window) = window else { return };
            window.set_to_fill_layout();
            let mut v = View::from(&self.view);
            window.add_child_view(Some(&mut v));
            #[cfg(target_os = "macos")]
            crate::mac::pin_window(window.window_handle());
            // A plain bar: Windows neither rounds it nor draws a line
            // around it.
            #[cfg(target_os = "windows")]
            crate::win::square_window(window.window_handle());
            let engine = Engine::global();
            let shown = {
                let mut inner = engine.lock();
                let d = inner.docked.get_mut(&self.id);
                d.and_then(|d| {
                    let shown = d.shown;
                    d.device.as_mut().map(|m| {
                        m.bar_window = Some(window.clone());
                        shown
                    })
                })
            };
            match shown {
                // Device mode ended while the window was being made.
                None => window.close(),
                Some(shown) => {
                    engine.replace_page(self.id);
                    if shown {
                        window.show();
                        window.bring_to_top();
                    }
                }
            }
        }
        fn parent_window(
            &self,
            _window: Option<&mut Window>,
            _is_menu: Option<&mut ::std::os::raw::c_int>,
            _can_activate_menu: Option<&mut ::std::os::raw::c_int>,
        ) -> Option<Window> {
            Engine::global().frame_window()
        }
        fn is_frameless(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int { 1 }
        fn with_standard_window_buttons(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int { 0 }
        fn can_close(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int { 1 }
        fn accepts_first_mouse(&self, _window: Option<&mut Window>) -> State { State::ENABLED }
        fn window_runtime_style(&self) -> RuntimeStyle { RuntimeStyle::ALLOY }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choices_from_the_bar_are_checked() {
        let ok = serde_json::to_value(DeviceConfig::default()).unwrap();
        assert_eq!(
            DeviceConfig::parse(ok.clone()),
            Some(DeviceConfig::default())
        );
        let with = |key: &str, value: Value| {
            let mut v = ok.clone();
            v[key] = value;
            DeviceConfig::parse(v)
        };
        assert!(with("width", json!(10)).is_none());
        assert!(with("dpr", json!(30)).is_none());
        assert!(with("zoom", json!(500)).is_none());
        assert!(with("ua", json!("nokia")).is_none());
        assert!(with("throttle", json!("3g")).is_some_and(|c| c.throttle == Throttle::ThreeG));
    }

    #[test]
    fn a_device_fits_into_its_area() {
        let phone = DeviceConfig {
            width: 400,
            height: 800,
            ..DeviceConfig::default()
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
        // A zoom of its own; larger than the area is cut to it.
        let zoomed = DeviceConfig {
            zoom: Some(50),
            ..phone.clone()
        };
        assert_eq!(zoomed.fit(&area(1000, 900)).0.width, 200);
        let large = DeviceConfig {
            zoom: Some(200),
            ..phone
        };
        assert_eq!(large.fit(&area(1000, 900)).0.height, 900);
    }
}
