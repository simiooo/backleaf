use super::*;
impl Backleaf {
    pub fn sync_ole(&mut self, window: &Window, _: &mut Context<Self>) {
        self.ole_target = None;
        let Some(site) = &self.ole else { return };
        let Some(pointer) = site.pointer() else {
            return;
        };
        let position = point(
            px(pointer.x as f32 / window.scale_factor()),
            px(pointer.y as f32 / window.scale_factor()),
        );
        let over_task = (self.panel || self.files.open)
            && f32::from(position.x) >= f32::from(window.viewport_size().width) - 424.
            && f32::from(position.y) >= f32::from(window.viewport_size().height) - 640.;
        let editable = over_task
            || self.navigation.open
            || self.watch_stale()
            || self.files.editor.is_some()
            || self.search.open
            || self.view_editing()
            || self.archive.draft.is_some()
            || self.files.close_guard
            || self.files.menu.is_some()
            || self.navigation.menu.is_some();
        let destination = if editable || f32::from(position.y) < layout::CHROME.bar_height {
            None
        } else if f32::from(position.x) < self.sidebar_width() {
            Some(ole::Destination::Pin(
                self.quick_before(self.quick_slot(position)),
                self.navigation.config_revision,
            ))
        } else if let Some(id) = self.hit_file(position, window) {
            self.cache.get(&id).filter(|i| i.directory).map(|i| {
                self.ole_target = Some(id);
                ole::Destination::Folder(self.path.join(&i.os_name))
            })
        } else {
            let bounds = Bounds::new(
                point(
                    px(self.sidebar_width() + layout::SURFACE.margin_x),
                    px(layout::CHROME.bar_height + layout::SURFACE.margin_y),
                ),
                size(
                    px((f32::from(window.viewport_size().width)
                        - self.sidebar_width()
                        - 2. * layout::SURFACE.margin_x)
                        .max(0.)),
                    px((f32::from(window.viewport_size().height)
                        - layout::CHROME.bar_height
                        - 2. * layout::SURFACE.margin_y)
                        .max(0.)),
                ),
            );
            if bounds.contains(&position) {
                Some(ole::Destination::Folder(self.path.clone()))
            } else {
                None
            }
        };
        site.destination(destination);
    }
    pub fn ole_overlay(&self, window: &Window) -> AnyElement {
        let Some(site) = &self.ole else {
            return div().into_any_element();
        };
        let Some(pointer) = site.pointer() else {
            return div().into_any_element();
        };
        div()
            .absolute()
            .left(px((pointer.x as f32 / window.scale_factor() + 14.).min(
                (f32::from(window.viewport_size().width) - 280.).max(0.),
            )))
            .top(px((pointer.y as f32 / window.scale_factor() + 18.).min(
                (f32::from(window.viewport_size().height) - 64.).max(0.),
            )))
            .max_w(px(264.))
            .px(px(12.))
            .py(px(16.))
            .rounded(px(12.))
            .bg(self.colors.color(ColorRole::Floating))
            .shadow(self.colors.control_shadows(false))
            .child(div().truncate().child(site.label()))
            .into_any_element()
    }
}
