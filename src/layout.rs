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
pub struct Layout {
    pub grid: Grid,
    pub file: File,
    pub control: Control,
    pub panel: Panel,
}
pub static LAYOUT: LazyLock<Layout> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../design/layout-system.json"))
        .expect("editable layout specification")
});
pub fn row_step() -> f32 {
    LAYOUT.grid.cell_height + LAYOUT.grid.row_gap
}
