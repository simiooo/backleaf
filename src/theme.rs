//! The editable design and the native app read the same semantic color roles.
use gpui::{BoxShadow, Hsla, Window, WindowAppearance, point, px, rgba};
use std::sync::OnceLock;

#[derive(Clone, Copy)]
#[repr(usize)]
pub enum Role {
    Canvas,
    Chrome,
    Floating,
    InputRest,
    InputActive,
    Primary,
    Secondary,
    Disabled,
    Border,
    Accent,
    OnAccent,
    Selected,
    OnSelected,
    InactiveSelected,
    OnInactiveSelected,
    Focus,
    Hover,
    Pressed,
    Scrim,
    Shadow,
    FolderFill,
    FolderStroke,
    PaperFill,
    PaperContour,
    Negative,
    Positive,
}
const ROLES: [(Role, &str); 26] = [
    (Role::Canvas, "surface/canvas"),
    (Role::Chrome, "surface/chrome"),
    (Role::Floating, "surface/floating"),
    (Role::InputRest, "surface/input-rest"),
    (Role::InputActive, "surface/input-active"),
    (Role::Primary, "content/primary"),
    (Role::Secondary, "content/secondary"),
    (Role::Disabled, "content/disabled"),
    (Role::Border, "border/subtle"),
    (Role::Accent, "accent/strong"),
    (Role::OnAccent, "accent/on-strong"),
    (Role::Selected, "selection/surface"),
    (Role::OnSelected, "selection/content"),
    (Role::InactiveSelected, "selection/inactive-surface"),
    (Role::OnInactiveSelected, "selection/inactive-content"),
    (Role::Focus, "focus/ring"),
    (Role::Hover, "interaction/hover"),
    (Role::Pressed, "interaction/pressed"),
    (Role::Scrim, "overlay/scrim"),
    (Role::Shadow, "overlay/shadow"),
    (Role::FolderFill, "icon/folder-fill"),
    (Role::FolderStroke, "icon/folder-stroke"),
    (Role::PaperFill, "icon/paper-fill"),
    (Role::PaperContour, "icon/paper-contour"),
    (Role::Negative, "status/negative"),
    (Role::Positive, "status/positive"),
];
pub struct Palette {
    pub dark: bool,
    values: [u32; 26],
}
impl Palette {
    fn load(dark: bool) -> Self {
        let source: serde_json::Value =
            serde_json::from_str(include_str!("../design/color-system.json"))
                .expect("design color system");
        let mode = &source["modes"][if dark { "Dark" } else { "Light" }];
        let mut values = [0; 26];
        for (role, key) in ROLES {
            let value = mode[key]
                .as_str()
                .expect("semantic role")
                .trim_start_matches('#');
            let raw = u32::from_str_radix(value, 16).expect("sRGB color");
            values[role as usize] = if value.len() == 6 {
                raw << 8 | 255
            } else {
                raw
            };
        }
        Self { dark, values }
    }
    pub fn color(&self, role: Role) -> Hsla {
        rgba(self.values[role as usize]).into()
    }
    pub fn hex(&self, role: Role) -> u32 {
        self.values[role as usize] >> 8
    }
    pub fn overlay(&self, base: Role, interaction: Role) -> Hsla {
        let a = self.values[interaction as usize];
        let opacity = (a & 255) as f32 / 255.;
        rgba((mix(self.hex(base), a >> 8, opacity) << 8) | 255).into()
    }
    pub fn shadow(&self) -> BoxShadow {
        BoxShadow {
            color: self.color(Role::Shadow),
            offset: point(px(0.), px(8.)),
            blur_radius: px(24.),
            spread_radius: px(0.),
            inset: false,
        }
    }
    pub fn focus_ring(&self) -> BoxShadow {
        BoxShadow {
            color: self.color(Role::Focus),
            offset: point(px(0.), px(0.)),
            blur_radius: px(0.),
            spread_radius: px(crate::layout::LAYOUT.control.focus_width),
            inset: false,
        }
    }
    pub fn input_shadows(&self) -> Vec<BoxShadow> {
        self.control_shadows(true)
    }
    pub fn control_shadows(&self, focused: bool) -> Vec<BoxShadow> {
        let alpha = if focused {
            crate::layout::CHROME.focus_shadow_opacity
        } else {
            crate::layout::CHROME.hover_shadow_opacity
        };
        [(1., 2.), (2., 6.)]
            .into_iter()
            .zip(alpha)
            .map(|((y, blur), a)| {
                let mut color = self.color(Role::Shadow);
                color.a = a;
                BoxShadow {
                    color,
                    offset: point(px(0.), px(y)),
                    blur_radius: px(blur),
                    spread_radius: px(0.),
                    inset: false,
                }
            })
            .collect()
    }
}
pub fn palette(dark: bool) -> &'static Palette {
    static LIGHT: OnceLock<Palette> = OnceLock::new();
    static DARK: OnceLock<Palette> = OnceLock::new();
    if dark {
        DARK.get_or_init(|| Palette::load(true))
    } else {
        LIGHT.get_or_init(|| Palette::load(false))
    }
}
pub fn for_window(window: &Window) -> &'static Palette {
    palette(matches!(
        window.appearance(),
        WindowAppearance::Dark | WindowAppearance::VibrantDark
    ))
}
pub fn mix(a: u32, b: u32, amount: f32) -> u32 {
    [16, 8, 0].into_iter().fold(0, |rgb, shift| {
        let (x, y) = (((a >> shift) & 255) as f32, ((b >> shift) & 255) as f32);
        rgb | (((x + (y - x) * amount).round() as u32) << shift)
    })
}
