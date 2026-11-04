//! Geometry shared with the editable OpenPencil specification.
use serde::Deserialize;
use std::sync::LazyLock;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Grid {
    pub minimum_cell_width: f32,
    pub cell_height: f32,
    pub column_gap: f32,
    pub row_gap: f32,
    pub icon_size: f32,
    pub name_line_height: f32,
    pub maximum_name_lines: usize,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct File {
    pub visual_padding_x: f32,
    pub visual_padding_top: f32,
    pub visual_padding_bottom: f32,
    pub icon_name_gap: f32,
    pub name_inset_x: f32,
    pub name_inset_y: f32,
    pub radius: f32,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Control {
    pub hit_size: f32,
    pub icon_size: f32,
    pub radius: f32,
    pub focus_width: f32,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Panel {
    pub padding_x: f32,
    pub padding_y: f32,
    pub section_gap: f32,
    pub radius: f32,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Floating {
    pub padding_x: f32,
    pub padding_y: f32,
    pub radius: f32,
    pub row_height: f32,
    pub row_padding_x: f32,
    pub row_radius: f32,
    pub group_break_height: f32,
}
#[derive(Deserialize)]
pub struct Layout {
    pub grid: Grid,
    pub file: File,
    pub control: Control,
    pub panel: Panel,
    pub floating: Floating,
}
pub static LAYOUT: LazyLock<Layout> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../design/layout-system.json"))
        .expect("editable layout specification")
});
pub fn row_step() -> f32 {
    LAYOUT.grid.cell_height + LAYOUT.grid.row_gap
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chrome {
    pub bar_height: f32,
    pub control_size: f32,
    pub control_gap: f32,
    pub vertical_inset: f32,
    pub path_height: f32,
    pub crumb_height: f32,
    pub caption_width: f32,
    pub caption_gap: f32,
    pub minimum_drag_width: f32,
    pub search_width: f32,
    pub search_minimum_width: f32,
    pub search_duration_ms: u64,
    pub search_close_duration_ms: u64,
    pub control_radius: f32,
    pub hover_shadow_opacity: [f32; 2],
    pub focus_shadow_opacity: [f32; 2],
}
pub static CHROME: LazyLock<Chrome> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../design/chrome-system.json"))
        .expect("editable chrome specification")
});
pub fn content_top() -> f32 {
    CHROME.bar_height + 16.
}
pub fn grid_reserved_height() -> f32 {
    CHROME.bar_height + 32.
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scroll {
    pub rail_width: f32,
    pub gutter: f32,
    pub thumb_width: f32,
    pub active_thumb_width: f32,
    pub minimum_thumb_height: f32,
    pub inset_y: f32,
    pub rail_inset: f32,
    pub thumb_opacity: f32,
    pub active_thumb_opacity: f32,
    pub status_height: f32,
    pub status_padding_x: f32,
    pub status_radius: f32,
    pub status_font_size: f32,
    pub status_line_height: f32,
    pub sidebar_row_height: f32,
    pub sidebar_row_gap: f32,
    pub sidebar_padding_y: f32,
}
pub static SCROLL: LazyLock<Scroll> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../design/scroll-system.json"))
        .expect("editable scroll specification")
});
