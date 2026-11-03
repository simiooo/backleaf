use gpui::{prelude::*, *};
use std::{borrow::Cow, cell::RefCell, collections::HashMap, rc::Rc};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Default)]
pub struct GridTextCache {
    pub lines: HashMap<usize, Rc<Vec<WrappedLine>>>,
    pub geometry: (u32, u32),
    pub builds: u64,
    pub peak: usize,
}
pub type SharedGridTextCache = Rc<RefCell<GridTextCache>>;
/// Use GPUI's own Unicode wrapping/ellipsis once at the final cell width, then reuse the glyph layout.
pub fn grid_name(id: usize, label: String, cache: SharedGridTextCache) -> impl IntoElement {
    canvas(
        move |bounds, window, cx| {
            let mut cache = cache.borrow_mut();
            if let std::collections::hash_map::Entry::Vacant(entry) = cache.lines.entry(id) {
                let text: SharedString = label.into();
                let font = font("Microsoft YaHei UI");
                let font_size = px(12.);
                let runs = [TextRun {
                    len: text.len(),
                    font: font.clone(),
                    color: rgb(0x2e3439).into(),
                    ..Default::default()
                }];
                let mut wrapper = cx.text_system().line_wrapper(font, font_size);
                let (text, runs) = wrapper.truncate_wrapped_line(
                    text,
                    bounds.size.width,
                    2,
                    "…",
                    &runs,
                    TruncateFrom::End,
                );
                let lines = window
                    .text_system()
                    .shape_text(text, font_size, &runs, Some(bounds.size.width), Some(2))
                    .unwrap_or_default();
                entry.insert(Rc::new(lines.into_vec()));
                cache.builds += 1;
                cache.peak = cache.peak.max(cache.lines.len());
            }
            cache.lines[&id].clone()
        },
        move |bounds, lines, window, cx| {
            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                let mut origin = bounds.origin;
                for line in lines.iter() {
                    let _ =
                        line.paint(origin, px(17.), TextAlign::Center, Some(bounds), window, cx);
                    origin.y += line.size(px(17.)).height;
                }
            });
        },
    )
    .w_full()
    .h(px(34.))
    .flex_none()
}

pub struct Assets;
impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        let data = match path.strip_suffix(".svg").unwrap_or(path) {
            "folder" => {
                r##"<svg width="96" height="96" viewBox="0 0 48 48" xmlns="http://www.w3.org/2000/svg"><path d="M5 13a3 3 0 0 1 3-3h10l4 5h18a3 3 0 0 1 3 3v19a3 3 0 0 1-3 3H8a3 3 0 0 1-3-3Z" fill="#dfe5df" stroke="#84958b" stroke-width="1.2"/><path d="M5 19h38" stroke="#a8b7ad" stroke-width="1"/></svg>"##
            }
            "file" => {
                r##"<svg width="96" height="96" viewBox="0 0 48 48" xmlns="http://www.w3.org/2000/svg"><path d="M12 5h16l10 10v26a2 2 0 0 1-2 2H12a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2Z" fill="#fafbfc" stroke="#a6aeb4" stroke-width="1.2"/><path d="M28 5v10h10M17 23h14M17 28h14M17 33h9" fill="none" stroke="#a6aeb4" stroke-width="1.2"/></svg>"##
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
pub fn icon(name: &'static str) -> Svg {
    svg().path(name).size(px(18.)).text_color(rgb(0x606970))
}

pub struct Tip(pub String);
impl Render for Tip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .max_w(px(520.))
            .px(px(8.))
            .py(px(12.))
            .bg(rgb(0xffffff))
            .border_1()
            .border_color(rgb(0xd6dbdf))
            .rounded(px(4.))
            .text_size(px(12.))
            .text_color(rgb(0x2e3439))
            .child(self.0.clone())
    }
}
pub fn tooltip(text: String) -> impl Fn(&mut Window, &mut App) -> AnyView {
    move |_, cx| cx.new(|_| Tip(text.clone())).into()
}

#[derive(Default)]
pub struct FlowCache {
    lines: HashMap<(String, u32), Vec<ShapedLine>>,
    pub misses: u64,
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
fn fit(text: &str, width: f32, window: &mut Window) -> String {
    if f32::from(line(text.to_owned().into(), 0x31566f, window).width()) <= width {
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
        if f32::from(line(candidate.into(), 0x31566f, window).width()) <= width {
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
) -> impl IntoElement {
    canvas(
        move |bounds, window, _| {
            let width = f32::from(bounds.size.width).max(1.);
            let key = (name.clone(), width.round() as u32);
            let mut cache = cache.borrow_mut();
            if !cache.lines.contains_key(&key) {
                if cache.lines.len() >= 16 {
                    cache.lines.clear();
                }
                let text: SharedString = fit(&name, width, window).into();
                let colors = [0x31566f, 0x385c75, 0x3e627b, 0x456881, 0x4b6e86];
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
