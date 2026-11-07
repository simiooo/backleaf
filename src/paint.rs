use crate::theme::{self, Palette, Role as ColorRole};
use gpui::{prelude::*, *};
use std::{borrow::Cow, cell::RefCell, collections::HashMap, rc::Rc};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Default)]
pub struct GridTextCache {
    pub lines: HashMap<(usize, u8), Rc<Vec<WrappedLine>>>,
    pub extents: HashMap<usize, (f32, f32)>,
    pub geometry: (u32, u32),
    pub builds: u64,
    pub peak: usize,
}
pub type SharedGridTextCache = Rc<RefCell<GridTextCache>>;
impl GridTextCache {
    pub fn clear(&mut self) {
        self.lines.clear();
        self.extents.clear();
    }
    pub fn retain(&mut self, range: &std::ops::Range<usize>) {
        self.lines.retain(|(id, _), _| range.contains(id));
        self.extents.retain(|id, _| range.contains(id));
    }
}
#[derive(Clone, Copy)]
pub struct FileState {
    pub selected: bool,
    pub active: bool,
    pub focused: bool,
    pub hovered: bool,
    pub pressed: bool,
}
fn grid_layout(
    id: usize,
    label: String,
    state: FileState,
    width: Pixels,
    cache: &SharedGridTextCache,
    colors: &'static Palette,
    window: &mut Window,
) -> Rc<Vec<WrappedLine>> {
    let mut cache = cache.borrow_mut();
    let key = (
        id,
        if state.selected {
            if state.active { 1 } else { 2 }
        } else {
            0
        },
    );
    if let std::collections::hash_map::Entry::Vacant(entry) = cache.lines.entry(key) {
        let text: SharedString = label.into();
        let mut font = font("Microsoft YaHei UI");
        font.weight = FontWeight::NORMAL;
        let font_size = px(12.);
        let runs = [TextRun {
            len: text.len(),
            font: font.clone(),
            color: colors.color(if state.selected {
                if state.active {
                    ColorRole::OnSelected
                } else {
                    ColorRole::OnInactiveSelected
                }
            } else {
                ColorRole::Primary
            }),
            ..Default::default()
        }];
        let mut wrapper = window.text_system().line_wrapper(font, font_size);
        let (text, runs) = wrapper.truncate_wrapped_line(
            text,
            width.max(px(1.)),
            crate::layout::LAYOUT.grid.maximum_name_lines,
            "…",
            &runs,
            TruncateFrom::End,
        );
        let lines = window
            .text_system()
            .shape_text(
                text,
                font_size,
                &runs,
                Some(width.max(px(1.))),
                Some(crate::layout::LAYOUT.grid.maximum_name_lines),
            )
            .unwrap_or_default();
        entry.insert(Rc::new(lines.into_vec()));
        cache.builds += 1;
        cache.peak = cache.peak.max(cache.lines.len());
    }
    let lines = cache.lines[&key].clone();
    let width = lines
        .iter()
        .map(|l| f32::from(l.width()))
        .fold(0., f32::max);
    let height = lines
        .iter()
        .map(|l| {
            f32::from(
                l.size(px(crate::layout::LAYOUT.grid.name_line_height))
                    .height,
            )
        })
        .sum::<f32>();
    let f = &crate::layout::LAYOUT.file;
    cache.extents.insert(
        id,
        (width + f.name_inset_x * 2., height + f.name_inset_y * 2.),
    );
    lines
}
/// The same measured rounded surface is used for pointer hit testing.
pub fn file_background(
    id: usize,
    label: String,
    state: FileState,
    cache: SharedGridTextCache,
    colors: &'static Palette,
) -> impl IntoElement {
    canvas(
        move |bounds, window, _| {
            let f = &crate::layout::LAYOUT.file;
            grid_layout(
                id,
                label,
                state,
                bounds.size.width - px(2. * (f.visual_padding_x + f.name_inset_x)),
                &cache,
                colors,
                window,
            );
            cache
                .borrow()
                .extents
                .get(&id)
                .copied()
                .unwrap_or((0., 20.))
        },
        move |bounds, (name_width, name_height), window, _| {
            if !state.selected && !state.hovered && !state.pressed {
                return;
            }
            let l = &crate::layout::LAYOUT;
            let surface = crate::file_geometry::surface(
                f32::from(bounds.size.width),
                name_width,
                name_height,
            );
            let base = if state.selected {
                if state.active {
                    ColorRole::Selected
                } else {
                    ColorRole::InactiveSelected
                }
            } else {
                ColorRole::Files
            };
            let fill = if state.pressed {
                colors.overlay(base, ColorRole::Pressed)
            } else if state.hovered {
                colors.overlay(base, ColorRole::Hover)
            } else {
                colors.color(base)
            };
            window.paint_quad(quad(
                Bounds::new(
                    point(
                        bounds.left() + px(surface.left),
                        bounds.top() + px(surface.top),
                    ),
                    size(px(surface.width), px(surface.height)),
                ),
                px(l.file.radius),
                fill,
                px(0.),
                transparent_black(),
                BorderStyle::Solid,
            ));
        },
    )
    .absolute()
    .inset_0()
}
/// Reuse the layout built by the state surface, including Unicode wrapping and ellipsis.
pub fn grid_name(
    id: usize,
    label: String,
    state: FileState,
    cache: SharedGridTextCache,
    colors: &'static Palette,
) -> impl IntoElement {
    canvas(
        move |bounds, window, _| {
            grid_layout(
                id,
                label,
                state,
                bounds.size.width - px(crate::layout::LAYOUT.file.name_inset_x * 2.),
                &cache,
                colors,
                window,
            )
        },
        move |bounds, lines, window, cx| {
            let l = &crate::layout::LAYOUT;
            window.with_content_mask(
                Some(ContentMask {
                    bounds: bounds.dilate(px(8.)),
                }),
                |window| {
                    let width = lines
                        .iter()
                        .map(|l| f32::from(l.width()))
                        .fold(0., f32::max);
                    let height = lines
                        .iter()
                        .map(|line| f32::from(line.size(px(l.grid.name_line_height)).height))
                        .sum::<f32>();
                    let name_bounds = Bounds::new(
                        point(
                            bounds.center().x - px(width / 2. + l.file.name_inset_x),
                            bounds.top(),
                        ),
                        size(
                            px(width + l.file.name_inset_x * 2.),
                            px(height + l.file.name_inset_y * 2.),
                        ),
                    );
                    if state.focused && state.active {
                        window.paint_drop_shadows(
                            name_bounds,
                            Corners::all(px(l.control.radius)),
                            &colors.control_shadows(true),
                        );
                        window.paint_quad(quad(
                            name_bounds,
                            px(l.control.radius),
                            colors.color(ColorRole::InputActive),
                            px(0.),
                            transparent_black(),
                            BorderStyle::Solid,
                        ));
                    }
                    let mut origin = point(
                        name_bounds.left() + px(l.file.name_inset_x),
                        name_bounds.top() + px(l.file.name_inset_y),
                    );
                    let text_bounds = Bounds::new(origin, size(px(width.max(1.)), px(height)));
                    for line in lines.iter() {
                        let _ = line.paint(
                            origin,
                            px(l.grid.name_line_height),
                            TextAlign::Center,
                            Some(text_bounds),
                            window,
                            cx,
                        );
                        origin.y += line.size(px(l.grid.name_line_height)).height;
                    }
                },
            );
        },
    )
    .w_full()
    .h(px(crate::layout::LAYOUT.grid.name_line_height
        * crate::layout::LAYOUT.grid.maximum_name_lines as f32
        + crate::layout::LAYOUT.file.name_inset_y * 2.))
    .flex_none()
}

pub struct Assets;
impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        if let Some(bytes) = crate::file_icons::asset_bytes(path) {
            return Ok(Some(Cow::Borrowed(bytes)));
        }
        match path {
            "paper-contour.svg" => {
                return Ok(Some(Cow::Borrowed(include_bytes!(
                    "../assets/file-types-generated/paper-contour.svg"
                ))));
            }
            "paper-contour-small.svg" => {
                return Ok(Some(Cow::Borrowed(include_bytes!(
                    "../assets/file-types-generated/paper-contour-small.svg"
                ))));
            }
            _ => {}
        }
        let key = path.strip_suffix(".svg").unwrap_or(path);
        if matches!(key, "folder" | "file" | "folder-dark" | "file-dark") {
            let colors = theme::palette(key.ends_with("-dark"));
            let stroke = colors.hex(ColorRole::FolderStroke);
            let data = if key.starts_with("folder") {
                format!(
                    r##"<svg width="96" height="96" viewBox="0 0 48 48" xmlns="http://www.w3.org/2000/svg"><path d="M5 13a3 3 0 0 1 3-3h10l4 5h18a3 3 0 0 1 3 3v19a3 3 0 0 1-3 3H8a3 3 0 0 1-3-3Z" fill="#{:06x}" stroke="#{stroke:06x}" stroke-width="1.2"/><path d="M5 19h38" fill="none" stroke="#{stroke:06x}" stroke-width="1"/></svg>"##,
                    colors.hex(ColorRole::FolderFill)
                )
            } else {
                format!(
                    r##"<svg width="96" height="96" viewBox="0 0 48 48" xmlns="http://www.w3.org/2000/svg"><path d="M12 5h16l10 10v26a2 2 0 0 1-2 2H12a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2Z" fill="#{:06x}" stroke="#{stroke:06x}" stroke-width="1.2"/><path d="M28 5v10h10M17 23h14M17 28h14M17 33h9" fill="none" stroke="#{stroke:06x}" stroke-width="1.2"/></svg>"##,
                    colors.hex(ColorRole::PaperFill)
                )
            };
            return Ok(Some(Cow::Owned(data.into_bytes())));
        }
        let data = match path.strip_suffix(".svg").unwrap_or(path) {
            "home" => {
                r##"<svg viewBox="0 0 18 18" xmlns="http://www.w3.org/2000/svg"><path d="m2 8 7-6 7 6M4 7v9h4v-5h3v5h3V7" fill="none" stroke="black" stroke-width="1.2" stroke-linejoin="round"/></svg>"##
            }
            "filter" => {
                r##"<svg viewBox="0 0 18 18" xmlns="http://www.w3.org/2000/svg"><path d="M2 4h14M5 9h8M7 14h4" fill="none" stroke="black" stroke-width="1.2" stroke-linecap="round"/></svg>"##
            }
            "save" => {
                r##"<svg viewBox="0 0 18 18" xmlns="http://www.w3.org/2000/svg"><path d="M5 2.5h8v13l-4-3-4 3Z" fill="none" stroke="black" stroke-width="1.2" stroke-linejoin="round"/></svg>"##
            }
            "drive" => {
                r##"<svg viewBox="0 0 18 18" xmlns="http://www.w3.org/2000/svg"><rect x="1.5" y="2.5" width="15" height="13" rx="3" fill="none" stroke="black" stroke-width="1.1"/><path d="M2 10.5h14" stroke="black" stroke-width="1"/><circle cx="13" cy="13" r=".8" fill="black"/></svg>"##
            }
            "search" => {
                r##"<svg viewBox="0 0 18 18" xmlns="http://www.w3.org/2000/svg"><circle cx="7.5" cy="7.5" r="5.2" fill="none" stroke="black" stroke-width="1.3"/><path d="m11.5 11.5 4 4" stroke="black" stroke-width="1.3"/></svg>"##
            }
            "reveal" => {
                r##"<svg viewBox="0 0 18 18" xmlns="http://www.w3.org/2000/svg"><path d="m5 13 8-8M6 5h7v7" fill="none" stroke="black" stroke-width="1.2"/></svg>"##
            }
            "location" => {
                r##"<svg viewBox="0 0 18 18" xmlns="http://www.w3.org/2000/svg"><path d="M2 4h5l2 2h7v9H2Z" fill="none" stroke="black" stroke-width="1.2"/></svg>"##
            }
            "back" => {
                r##"<svg viewBox="0 0 18 18" xmlns="http://www.w3.org/2000/svg"><path d="m11 4-5 5 5 5" fill="none" stroke="black" stroke-width="1.3"/></svg>"##
            }
            "up" => {
                r##"<svg viewBox="0 0 18 18" xmlns="http://www.w3.org/2000/svg"><path d="M9 15V3M4 8l5-5 5 5" fill="none" stroke="black" stroke-width="1.3"/></svg>"##
            }
            "more" => {
                r##"<svg viewBox="0 0 18 18" xmlns="http://www.w3.org/2000/svg"><path d="m5 7 4 4 4-4" fill="none" stroke="black" stroke-width="1.2"/></svg>"##
            }
            "collapse" => {
                r##"<svg viewBox="0 0 18 18" xmlns="http://www.w3.org/2000/svg"><path d="m5 11 4-4 4 4" fill="none" stroke="black" stroke-width="1.2"/></svg>"##
            }
            "close" => {
                r##"<svg viewBox="0 0 18 18" xmlns="http://www.w3.org/2000/svg"><path d="m5 5 8 8M13 5l-8 8" fill="none" stroke="black" stroke-width="1"/></svg>"##
            }
            "min" => {
                r##"<svg viewBox="0 0 18 18" xmlns="http://www.w3.org/2000/svg"><path d="M4 9h10" stroke="black" stroke-width="1"/></svg>"##
            }
            "max" => {
                r##"<svg viewBox="0 0 18 18" xmlns="http://www.w3.org/2000/svg"><path d="M5 5h8v8H5Z" fill="none" stroke="black" stroke-width="1"/></svg>"##
            }
            "copy" => {
                r##"<svg viewBox="0 0 18 18" xmlns="http://www.w3.org/2000/svg"><path d="M6 6h8v9H6ZM4 12H3V3h8v1" fill="none" stroke="black" stroke-width="1.2"/></svg>"##
            }
            _ => return Ok(None),
        };
        Ok(Some(Cow::Borrowed(data.as_bytes())))
    }
    fn list(&self, _: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok(vec![])
    }
}
pub fn file_artwork(
    source: ImageSource,
    side: f32,
    outlined: bool,
    colors: &'static Palette,
) -> Div {
    div()
        .relative()
        .size(px(side))
        .flex_none()
        .child(img(source).size_full())
        .when(outlined, |d| {
            d.child(
                svg()
                    .path(if side < 32. {
                        "paper-contour-small.svg"
                    } else {
                        "paper-contour.svg"
                    })
                    .absolute()
                    .top_0()
                    .left_0()
                    .size(px(side))
                    .text_color(colors.color(ColorRole::PaperContour)),
            )
        })
}
/// A decoded image preview confined to `width` × `height`. `ObjectFit::Contain`
/// centers the original aspect ratio and never paints outside the box.
pub fn thumbnail(source: ImageSource, width: f32, height: f32) -> Div {
    div()
        .relative()
        .w(px(width))
        .h(px(height))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .child(img(source).size_full().object_fit(ObjectFit::Contain))
}
pub fn icon(name: &'static str, colors: &'static Palette) -> Svg {
    svg()
        .path(name)
        .size(px(crate::layout::LAYOUT.control.icon_size))
        .text_color(colors.color(ColorRole::Secondary))
}
pub fn activation_key(event: &KeyDownEvent) -> bool {
    let modifiers = event.keystroke.modifiers;
    matches!(event.keystroke.key.as_str(), "enter" | "space")
        && !modifiers.control
        && !modifiers.alt
        && !modifiers.platform
}

pub struct Tip(pub String, pub &'static Palette);
impl Render for Tip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .max_w(px(520.))
            .px(px(8.))
            .py(px(12.))
            .bg(self.1.color(ColorRole::Floating))
            .border_1()
            .border_color(self.1.color(ColorRole::Border))
            .shadow(vec![self.1.shadow()])
            .rounded(px(crate::layout::LAYOUT.panel.radius))
            .text_size(px(12.))
            .text_color(self.1.color(ColorRole::Primary))
            .child(self.0.clone())
    }
}
pub fn tooltip(
    text: String,
    colors: &'static Palette,
) -> impl Fn(&mut Window, &mut App) -> AnyView {
    move |_, cx| cx.new(|_| Tip(text.clone(), colors)).into()
}

#[derive(Default)]
pub struct FlowCache {
    lines: HashMap<(String, u32, bool), Vec<ShapedLine>>,
    pub misses: u64,
}
impl FlowCache {
    pub fn clear(&mut self) {
        self.lines.clear();
    }
}
pub type SharedFlowCache = Rc<RefCell<FlowCache>>;
fn line(text: SharedString, color: u32, window: &mut Window) -> ShapedLine {
    window.text_system().shape_line(
        text.clone(),
        px(13.),
        &[TextRun {
            len: text.len(),
            font: font("Microsoft YaHei UI"),
            color: rgb(color).into(),
            ..Default::default()
        }],
        None,
    )
}
fn fit(text: &str, width: f32, color: u32, window: &mut Window) -> String {
    if f32::from(line(text.to_owned().into(), color, window).width()) <= width {
        return text.to_owned();
    }
    let boundaries: Vec<usize> = text
        .grapheme_indices(true)
        .map(|(i, _)| i)
        .chain(std::iter::once(text.len()))
        .collect();
    let (mut low, mut high) = (0, boundaries.len() - 1);
    while low < high {
        let mid = (low + high).div_ceil(2);
        let candidate = format!("{}…", &text[..boundaries[mid]]);
        if f32::from(line(candidate.into(), color, window).width()) <= width {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    format!("{}…", &text[..boundaries[low]])
}
/// One immutable text layout per color level. Motion only changes eight clip rectangles.
pub fn flowing_name(
    name: String,
    phase: f32,
    animate: bool,
    cache: SharedFlowCache,
    palette: &'static Palette,
) -> impl IntoElement {
    canvas(
        move |bounds, window, _| {
            let width = f32::from(bounds.size.width).max(1.);
            let key = (name.clone(), width.round() as u32, palette.dark);
            let mut cache = cache.borrow_mut();
            if !cache.lines.contains_key(&key) {
                if cache.lines.len() >= 16 {
                    cache.lines.clear();
                }
                let text: SharedString =
                    fit(&name, width, palette.hex(ColorRole::Accent), window).into();
                let colors = [0., 0.08, 0.16, 0.24, 0.32].map(|amount| {
                    theme::mix(
                        palette.hex(ColorRole::Accent),
                        palette.hex(ColorRole::Primary),
                        amount,
                    )
                });
                cache.lines.insert(
                    key.clone(),
                    colors.map(|c| line(text.clone(), c, window)).into(),
                );
                cache.misses += 1;
            }
            cache.lines[&key].clone()
        },
        move |bounds, lines, window, cx| {
            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                let origin = point(bounds.origin.x, bounds.origin.y + px(4.));
                let _ = lines[0].paint(origin, px(20.), TextAlign::Left, None, window, cx);
                if animate {
                    let center = (f32::from(bounds.size.width) + 96.) * phase - 48.;
                    for (strip, level) in [1usize, 2, 3, 4, 4, 3, 2, 1].into_iter().enumerate() {
                        let clip = Bounds::new(
                            point(
                                bounds.origin.x + px(center - 32. + strip as f32 * 8.),
                                bounds.origin.y,
                            ),
                            size(px(8.), bounds.size.height),
                        );
                        window.with_content_mask(Some(ContentMask { bounds: clip }), |window| {
                            let _ = lines[level].paint(
                                origin,
                                px(20.),
                                TextAlign::Left,
                                None,
                                window,
                                cx,
                            );
                        });
                    }
                }
            });
        },
    )
    .w_full()
    .h(px(28.))
}
