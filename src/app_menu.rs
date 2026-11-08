use super::*;
use crate::{
    default_app, layout,
    paint::{icon, tooltip},
    theme::Role as ColorRole,
};
use gpui::{prelude::*, *};

impl Backleaf {
    pub fn app_menu_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors;
        let label = crate::i18n::tr("更多");
        div()
            .id("app-menu-button")
            .w(px(layout::CHROME.caption_width))
            .h(px(layout::CHROME.control_size))
            .rounded(px(layout::CHROME.control_radius))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .role(Role::Button)
            .aria_label(label)
            .aria_expanded(self.app_menu_open)
            .tooltip(tooltip(label.to_owned(), colors))
            .focusable()
            .tab_stop(true)
            .focus(move |d| {
                d.bg(colors.color(ColorRole::Floating))
                    .shadow(colors.control_shadows(true))
            })
            .hover(move |d| {
                d.bg(colors.color(ColorRole::Floating))
                    .shadow(colors.control_shadows(false))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.app_menu_was_open = this.app_menu_open;
                    cx.stop_propagation();
                }),
            )
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if paint::activation_key(event) {
                    this.app_menu_was_open = this.app_menu_open;
                    this.app_menu_open = !this.app_menu_was_open;
                    if this.app_menu_open {
                        this.close_path_tree();
                    }
                    window.focus(&this.focus, cx);
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .on_click(cx.listener(|this, _, _, cx| {
                this.app_menu_open = !this.app_menu_was_open;
                if this.app_menu_open {
                    this.close_path_tree();
                }
                cx.stop_propagation();
                cx.notify();
            }))
            .child(icon("overflow", colors))
            .into_any_element()
    }
    pub fn app_menu_overlay(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors;
        let support = default_app::support();
        let supported = support == default_app::Support::Installed;
        let hint = match support {
            default_app::Support::Installed => crate::i18n::tr("在 Windows 设置中确认默认应用"),
            default_app::Support::Portable => crate::i18n::tr("便携版不写系统默认项"),
            default_app::Support::Development => crate::i18n::tr("开发版不写系统默认项"),
        };
        let label = crate::i18n::tr("设为默认");
        let menu_width = 280.;
        let group_width = 4. * layout::CHROME.caption_width + 3. * layout::CHROME.caption_gap;
        let left = (f32::from(window.viewport_size().width) - 8. - group_width - menu_width
            + layout::CHROME.caption_width)
            .max(8.);
        div()
            .id("app-menu")
            .absolute()
            .top(px(layout::CHROME.bar_height + 4.))
            .left(px(left))
            .w(px(menu_width))
            .max_w(px((f32::from(window.viewport_size().width) - 16.).max(160.)))
            .p(px(layout::LAYOUT.floating.padding_x))
            .py(px(layout::LAYOUT.floating.padding_y))
            .rounded(px(layout::LAYOUT.floating.radius))
            .bg(colors.color(ColorRole::Floating))
            .border_1()
            .border_color(colors.color(ColorRole::Border))
            .shadow(vec![colors.shadow()])
            .occlude()
            .role(Role::Menu)
            .aria_label(crate::i18n::tr("应用菜单"))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down_out(cx.listener(|this, event: &MouseDownEvent, _, cx| {
                if event.button == MouseButton::Left
                    && f32::from(event.position.y) > layout::CHROME.bar_height
                {
                    this.app_menu_open = false;
                    cx.notify();
                }
            }))
            .child(
                div()
                    .id("app-menu-default")
                    .h(px(layout::LAYOUT.floating.row_height))
                    .px(px(layout::LAYOUT.floating.row_padding_x))
                    .rounded(px(layout::LAYOUT.floating.row_radius))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .role(Role::MenuItem)
                    .aria_label(label)
                    .when(supported, |d| {
                        d.hover(move |d| d.bg(colors.color(ColorRole::Hover)))
                    })
                    .when(!supported, |d| {
                        d.text_color(colors.color(ColorRole::Disabled))
                    })
                    .when(supported, |d| {
                        d.on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                            if paint::activation_key(event) {
                                this.activate_app_menu_default(window, cx);
                                cx.stop_propagation();
                            }
                        }))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.activate_app_menu_default(window, cx);
                            cx.stop_propagation();
                        }))
                    })
                    .child(div().flex_1().truncate().child(label))
                    .when(supported, |d| {
                        d.child(
                            div()
                                .text_size(px(11.))
                                .text_color(colors.color(ColorRole::Secondary))
                                .child("Windows"),
                        )
                    }),
            )
            .child(
                div()
                    .px(px(layout::LAYOUT.floating.row_padding_x + 2.))
                    .pt(px(4.))
                    .text_size(px(11.))
                    .text_color(colors.color(ColorRole::Secondary))
                    .child(hint),
            )
            .into_any_element()
    }
    fn activate_app_menu_default(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if !default_app::supported() {
            return;
        }
        self.app_menu_open = false;
        cx.notify();
        if let Err(error) = default_app::open_settings() {
            self.navigation_error(error, cx);
        }
    }
}
