//! What the page area shows while no page is on screen: the logo and how
//! to open a tab. Two labels in the frame's content panel, behind where the
//! pages go; hidden while a page is shown, so it never flashes up between
//! two tabs.

use cef::*;

use crate::engine::Engine;

/// The logo's width and height in DIP.
const LOGO: i32 = 72;
/// The app icon, 128 pixels wide.
const LOGO_PNG: &[u8] = include_bytes!("../../ui/web/src/assets/icon.png");
const HINT: &str = "Press ⌘T to open a tab";

/// The labels in the content panel.
pub(crate) struct EmptyHint {
    logo: LabelButton,
    text: LabelButton,
}

impl EmptyHint {
    /// Put the labels in the middle of `panel`, hidden.
    pub(crate) fn new(panel: &Panel) -> Option<Self> {
        panel.set_to_box_layout(Some(&BoxLayoutSettings {
            horizontal: 0,
            main_axis_alignment: AxisAlignment::CENTER,
            cross_axis_alignment: AxisAlignment::CENTER,
            between_child_spacing: 14,
            ..Default::default()
        }))?;
        let label = |text: &str| {
            let label =
                label_button_create(Some(&mut HintLabel::new()), Some(&CefString::from(text)))?;
            // A label, not a button: no hover, no focus, no click.
            label.set_enabled(0);
            label.set_ink_drop_enabled(0);
            label.set_focusable(0);
            label.set_visible(0);
            panel.add_child_view(Some(&mut View::from(&label)));
            Some(label)
        };
        let logo = label("")?;
        let image = image_create()?;
        #[allow(clippy::cast_precision_loss)]
        image.add_png(128.0 / LOGO as f32, Some(LOGO_PNG));
        logo.set_image(ButtonState::DISABLED, Some(&mut image.clone()));
        let text = label(HINT)?;
        text.set_font_list(Some(&CefString::from("Helvetica Neue, 15px")));
        Some(Self { logo, text })
    }

    fn set_visible(&self, visible: bool) {
        self.logo.set_visible(visible.into());
        self.text.set_visible(visible.into());
    }

    /// The text halfway between the background `argb` and white or black,
    /// whichever stands out.
    fn set_background(&self, argb: u32) {
        let channel = |shift: u32| (argb >> shift) & 0xFF;
        let light = channel(16) * 3 + channel(8) * 6 + channel(0) > 1280;
        let toward = if light { 0 } else { 0xFF };
        let mix = |shift: u32| u32::midpoint(channel(shift), toward) << shift;
        self.text.set_text_color(
            ButtonState::DISABLED,
            0xFF00_0000 | mix(16) | mix(8) | mix(0),
        );
    }
}

impl Engine {
    /// Show the hint for an empty window, or hide it: call with whether a
    /// page is on screen.
    pub fn set_empty_hint(&self, visible: bool) {
        let inner = self.lock();
        if let Some(hint) = inner.frame.as_ref().and_then(|f| f.empty.as_ref()) {
            hint.set_visible(visible);
        }
    }

    /// The hint's text follows the page area's colour.
    pub(crate) fn colour_empty_hint(&self, argb: u32) {
        let inner = self.lock();
        if let Some(hint) = inner.frame.as_ref().and_then(|f| f.empty.as_ref()) {
            hint.set_background(argb);
        }
    }
}

wrap_button_delegate! {
    struct HintLabel {}
    impl ViewDelegate {}
    impl ButtonDelegate {}
}
