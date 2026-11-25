//! The order of cameras and microphones, Chromium's own: a page that asks
//! for a camera or a microphone without naming one gets the first device
//! of its profile's list that is connected, and `enumerateDevices` lists
//! them in that order. Devices not in the list come after those in it, in
//! the system's order. Chrome fills the lists from the device preview in
//! its permission prompt; Magnetite asks with prompts of its own, so they
//! are set from the app's settings, the same in every profile.

use cef::*;
use serde::{Deserialize, Serialize};

use crate::engine::Engine;

/// The most devices a list keeps.
const MAX_DEVICES: usize = 32;
/// The longest name kept: Chromium's names are far shorter.
const MAX_NAME: usize = 256;

const CAMERAS: &str = "media.video_input.user_preference_ranking";
const MICROPHONES: &str = "media.audio_input.user_preference_ranking";

/// The order, as the app's settings keep it. Devices are named as
/// Chromium names them (`MediaDeviceInfo.label`: a USB device's name ends
/// in its `(vid:pid)`); `default` is the system's default microphone,
/// whichever that is.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DeviceOrder {
    /// The preferred camera first. Empty: the system's order.
    pub cameras: Vec<String>,
    /// The preferred microphone first. Empty: the system's default first.
    pub microphones: Vec<String>,
}

impl DeviceOrder {
    /// Each name once, none empty or overlong, the first `MAX_DEVICES` of
    /// each list.
    #[must_use]
    pub fn sanitised(self) -> Self {
        let clean = |names: Vec<String>| {
            let mut kept: Vec<String> = Vec::new();
            for name in names {
                if !name.is_empty() && name.len() <= MAX_NAME && !kept.contains(&name) {
                    kept.push(name);
                }
            }
            kept.truncate(MAX_DEVICES);
            kept
        };
        Self {
            cameras: clean(self.cameras),
            microphones: clean(self.microphones),
        }
    }

    /// Set the lists on `ctx`. False if Chromium refused one (a context
    /// that is not initialised yet refuses all; it gets them in
    /// `prepare_context`).
    pub(crate) fn apply(&self, ctx: &RequestContext) -> bool {
        let cameras = set_list_pref(ctx, CAMERAS, &self.cameras);
        set_list_pref(ctx, MICROPHONES, &self.microphones) && cameras
    }
}

/// Set Chromium preference `name` of `ctx` to the texts `items`, or back to
/// Chromium's own value (an empty list) with none.
fn set_list_pref(ctx: &RequestContext, name: &str, items: &[String]) -> bool {
    // Not empty: cef-rs passes an empty string as a null pointer, and CEF
    // refuses to set anything without a place for its error message.
    let mut error = CefString::from(" ");
    let name = CefString::from(name);
    if items.is_empty() {
        return ctx.set_preference(Some(&name), None, Some(&mut error)) != 0;
    }
    let (Some(mut value), Some(mut list)) = (value_create(), list_value_create()) else {
        return false;
    };
    for (i, item) in items.iter().enumerate() {
        list.set_string(i, Some(&CefString::from(item.as_str())));
    }
    value.set_list(Some(&mut list));
    ctx.set_preference(Some(&name), Some(&mut value), Some(&mut error)) != 0
}

impl Engine {
    /// Use `order` from now on: in every profile that exists, and in those
    /// made later.
    pub fn set_device_order(&self, order: &DeviceOrder) {
        let contexts: Vec<RequestContext> = {
            let mut inner = self.lock();
            inner.device_order.clone_from(order);
            inner.contexts.values().cloned().collect()
        };
        for ctx in request_context_get_global_context()
            .into_iter()
            .chain(contexts)
        {
            if !order.apply(&ctx) {
                crate::trace("cannot set the order of cameras and microphones");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DeviceOrder;

    #[test]
    fn each_device_counts_once() {
        let order = DeviceOrder {
            cameras: vec![
                "Cam (046d:085e)".into(),
                String::new(),
                "Cam (046d:085e)".into(),
            ],
            microphones: vec!["x".repeat(300), "default".into()],
        }
        .sanitised();
        assert_eq!(order.cameras, ["Cam (046d:085e)"]);
        assert_eq!(order.microphones, ["default"]);
    }
}
