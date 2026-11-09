//! Event-driven regressions on GPUI's TestPlatform; no desktop input or HWNDs.
use super::*;
use crate::input::TextInput;
use crate::menu::FileMenu;
use core::prelude::v1::test;

struct Harness {
    cx: HeadlessAppContext,
    window: WindowHandle<Backleaf>,
    view: Entity<Backleaf>,
    fixture: test_support::Fixture,
}
impl Harness {
    fn invalidate_directory(&mut self) {
        let view = self.view.clone();
        self.cx
            .update_window(self.window.into(), |_, window, cx| {
                view.update(cx, |this, cx| {
                    this.watch.retry = None;
                    this.watch.dirty = true;
                    this.sync_watcher(window, cx);
                })
            })
            .unwrap();
    }
    fn new(names: &[&str], generated: Option<usize>) -> Self {
        let fixture = test_support::Fixture::new("gpui-headless");
        let directory = fixture.0.join("files");
        std::fs::create_dir(&directory).unwrap();
        for name in names {
            std::fs::write(directory.join(name), format!("contents of {name}")).unwrap();
        }
        let options = Options {
            fixture: generated,
            path: Some(directory),
            output: None,
            seconds: 12,
            scroll: false,
            flow: 0,
            reduced_motion: true,
            width: 1120.,
            height: 720.,
            theme: Some(false),
            selection_smoke: false,
            marquee_smoke: false,
            navigation_fixture: 0,
            state_dir: fixture.0.join("state"),
            index_roots: Some(vec![]),
            restored: None,
            restore_error: None,
            elevate_request: None,
            elevation_restore: None,
        };
        let mut cx = HeadlessAppContext::with_asset_source(
            Arc::new(NoopTextSystem),
            Arc::new(paint::Assets),
        );
        cx.allow_parking();
        cx.update(input::bind_keys);
        let window = cx
            .open_window(size(px(1120.), px(720.)), |window, cx| {
                cx.new(|cx| Backleaf::new(options, window, cx))
            })
            .unwrap();
        let view = window.root(&mut cx).unwrap();
        let mut h = Self {
            cx,
            window,
            view,
            fixture,
        };
        h.cx.update_window(window.into(), |_, window, cx| {
            window.activate_window();
            let focus = h.view.read(cx).focus.clone();
            window.focus(&focus, cx);
        })
        .unwrap();
        h.wait("initial directory/cache", |this, _| {
            this.source.count() == generated.unwrap_or(names.len())
                && !this.cache.is_empty()
                && (generated.is_some() || !this.watch_stale())
        });
        h
    }
    fn read<R>(&self, f: impl FnOnce(&Backleaf, &App) -> R) -> R {
        self.view.read_with(&self.cx, f)
    }
    fn frame(&mut self) {
        self.cx.run_until_parked();
        let windows = self.cx.update(|cx| cx.windows());
        for handle in windows {
            let _ = self.cx.update_window(handle, |_, window, cx| {
                window.simulate_next_frame(cx);
                window.refresh();
                window.draw(cx).clear(cx);
            });
        }
        self.cx.run_until_parked();
    }
    fn wait(&mut self, what: &str, condition: impl Fn(&Backleaf, &App) -> bool) {
        // Hosted Windows runners can briefly starve the executor while a
        // rename/context-menu round trip waits on the filesystem; keep the
        // virtual clock cadence but allow enough real time for the async work.
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            self.frame();
            if self.read(&condition) {
                return;
            }
            self.cx.advance_clock(Duration::from_millis(10));
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {what}; {}",
                self.diagnostics()
            );
            std::thread::sleep(Duration::from_millis(3));
        }
    }
    fn diagnostics(&self) -> String {
        self.read(|this, cx| format!("path={:?} count={} cache={} selection={} editor={:?} search=({:?},{},{}) stale={} active={} detail_pending={} error={:?} windows={}", this.path, this.source.count(),this.cache.len(),this.selection.count(),this.files.editor.as_ref().map(|e|(e.input.read(cx).value(), e.error.clone())),this.search.query,this.search.count,this.search.done,this.watch_stale(),this.navigation.window_active,this.detail_pending,this.error,cx.windows().len()))
    }
    fn event(&mut self, handle: AnyWindowHandle, event: PlatformInput) {
        self.cx
            .update_window(handle, |_, window, cx| {
                window.dispatch_event(event, cx);
            })
            .unwrap();
        self.frame();
    }
    fn click(&mut self, x: f32, y: f32, button: MouseButton) {
        self.cx
            .update_window(self.window.into(), |_, window, _| window.activate_window())
            .unwrap();
        let position = point(px(x), px(y));
        self.event(
            self.window.into(),
            MouseMoveEvent {
                position,
                ..Default::default()
            }
            .to_platform_input(),
        );
        self.event(
            self.window.into(),
            MouseDownEvent {
                position,
                button,
                click_count: 1,
                ..Default::default()
            }
            .to_platform_input(),
        );
        self.event(
            self.window.into(),
            MouseUpEvent {
                position,
                button,
                click_count: 1,
                ..Default::default()
            }
            .to_platform_input(),
        );
    }
    fn key_in(&mut self, handle: AnyWindowHandle, key: &str) {
        self.cx
            .update_window(handle, |_, window, cx| {
                window.dispatch_keystroke(
                    Keystroke::parse_with_os(key, OperatingSystem::Windows).unwrap(),
                    cx,
                );
            })
            .unwrap();
        self.frame();
    }
    fn key(&mut self, key: &str) {
        self.key_in(self.window.into(), key);
    }
    fn text(&mut self, text: &str) {
        for c in text.chars() {
            self.key(&c.to_string());
        }
    }
    fn focused(&mut self, input: &Entity<TextInput>) -> bool {
        self.cx
            .update_window(self.window.into(), |_, window, cx| {
                input.focus_handle(cx).is_focused(window)
            })
            .unwrap()
    }
    fn grid_focused(&mut self) -> bool {
        self.cx
            .update_window(self.window.into(), |_, window, cx| {
                self.view.read(cx).focus.is_focused(window)
            })
            .unwrap()
    }
    fn alert(&mut self) {
        let error = self.read(|this, _| {
            this.files
                .editor
                .as_ref()
                .and_then(|e| e.error.clone())
                .expect("error retained")
        });
        let painted = self
            .cx
            .update_window(self.window.into(), |_, window, _| {
                let scale = window.scale_factor();
                window.painted_quads().iter().any(|quad| {
                    (quad.bounds.size.width.0 / scale - 280.).abs() < 1.
                        && quad.bounds.size.height.0 / scale > 24.
                        && (quad.corner_radii.top_left.0 / scale - 8.).abs() < 1.
                })
            })
            .unwrap();
        assert!(painted, "error toast surface was not painted: {error}");
    }
    fn popup(&mut self) -> AnyWindowHandle {
        let windows = self.cx.update(|cx| cx.windows());
        assert_eq!(
            windows.len(),
            2,
            "one independent popup: {}",
            self.diagnostics()
        );
        *windows.iter().find(|w| **w != self.window.into()).unwrap()
    }
    fn shell_menu(&mut self) -> Entity<FileMenu> {
        let menu = self.read(|this, _| {
            let menu: &Entity<FileMenu> = this.files.menu.as_ref().expect("open menu");
            menu.clone()
        });
        self.cx.update(|cx| {
            menu.update(cx, |model, cx| {
                model.attach_shell(Ok(crate::shell_menu::Session::headless_fixture(4242)), cx)
            })
        });
        self.frame();
        menu
    }
    fn shell_rows(
        &mut self,
        menu: &Entity<FileMenu>,
        parent: u32,
        rows: Vec<crate::shell_menu::Item>,
    ) {
        self.cx.update(|cx| {
            menu.update(cx, |model, cx| {
                model.shell_notice(
                    crate::shell_menu::Notice {
                        session: 4242,
                        parent,
                        result: Ok(rows),
                    },
                    cx,
                )
            })
        });
        self.frame();
    }
    fn window_bounds(&mut self, handle: AnyWindowHandle) -> Bounds<Pixels> {
        self.cx
            .update_window(handle, |_, window, _| window.bounds())
            .unwrap()
    }
    fn first_icon(&self) -> (f32, f32) {
        self.read(|this, _| {
            (
                layout::content_left(this.sidebar_width()) + this.cell_width / 2.,
                layout::content_top()
                    + layout::LAYOUT.file.visual_padding_top
                    + layout::LAYOUT.grid.icon_size / 2.,
            )
        })
    }
    fn file(&self, name: &str) -> PathBuf {
        self.fixture.0.join("files").join(name)
    }
    fn editor(&self) -> Entity<TextInput> {
        self.read(|this, _| {
            this.files
                .editor
                .as_ref()
                .expect("editor opened")
                .input
                .clone()
        })
    }
    fn properties(&mut self) -> WindowHandle<crate::properties::Properties> {
        self.wait("independent properties window", |_, cx| {
            cx.windows()
                .iter()
                .any(|handle| handle.downcast::<crate::properties::Properties>().is_some())
        });
        self.cx.update(|cx| {
            cx.windows()
                .into_iter()
                .find_map(|handle| handle.downcast::<crate::properties::Properties>())
                .unwrap()
        })
    }
    fn properties_ready(&mut self, handle: WindowHandle<crate::properties::Properties>) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            self.frame();
            if handle
                .read_with(&self.cx, |view, _| view.snapshot.done)
                .unwrap()
            {
                return;
            }
            self.cx.advance_clock(Duration::from_millis(20));
            assert!(Instant::now() < deadline, "properties scan timed out");
            std::thread::sleep(Duration::from_millis(3));
        }
    }
    fn rename(&mut self) {
        let (x, y) = self.first_icon();
        self.click(x, y, MouseButton::Left);
        self.key("f2");
        self.wait("rename input", |this, _| this.files.editor.is_some());
        let input = self.editor();
        assert!(self.focused(&input));
    }
}

#[test]
fn directory_refresh_never_clears_the_grid_and_remaps_selection_by_identity() {
    let mut h = Harness::new(&["keep.txt", "rename.txt", "replace.txt"], None);
    let (directory, generation, handle) =
        h.read(|this, _| (this.path.clone(), this.generation, this.scroll.0.clone()));
    h.view.update(&mut h.cx, |this, cx| {
        this.selection.all(3);
        cx.notify();
    });
    std::fs::rename(directory.join("rename.txt"), directory.join("renamed.txt")).unwrap();
    std::fs::rename(
        directory.join("replace.txt"),
        h.fixture.0.join("old-replace.txt"),
    )
    .unwrap();
    std::fs::write(directory.join("replace.txt"), "different file identity").unwrap();
    std::fs::write(directory.join("new.txt"), "new").unwrap();
    h.invalidate_directory();
    h.read(|this, _| {
        assert_eq!(this.source.count(), 3);
        assert_eq!(this.cache.len(), 3);
        assert_eq!(this.selection.count(), 3);
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    while h.read(|this, _| this.generation == generation) {
        h.frame();
        h.read(|this, _| {
            assert!(!this.cache.is_empty());
            assert!(Rc::ptr_eq(&this.scroll.0, &handle));
        });
        assert!(Instant::now() < deadline);
        h.cx.advance_clock(Duration::from_millis(20));
        std::thread::sleep(Duration::from_millis(3));
    }
    h.read(|this, _| {
        assert_eq!(this.source.count(), 4);
        assert_eq!(this.selection.count(), 2);
        let mut names: Vec<_> = this
            .cache
            .values()
            .filter(|i| this.selection.contains(i.id))
            .map(|i| i.name.as_str())
            .collect();
        names.sort();
        assert_eq!(names, vec!["keep.txt", "renamed.txt"]);
    });
    // A subsequent native notification must still be received after display generation changes.
    std::fs::write(directory.join("second.txt"), "second").unwrap();
    h.wait("second native watcher refresh", |this, _| {
        this.source.count() == 5 && this.watch.pending.is_none()
    });
}

#[test]
fn automatic_search_refresh_keeps_rows_until_the_new_page_is_ready() {
    let mut h = Harness::new(&["needle-old.txt"], None);
    h.key("ctrl-f");
    h.text("needle");
    h.key("enter");
    h.wait("first search page", |this, _| {
        this.search.done && this.search.rows.len() == 1
    });
    let (path, handle) = h.read(|this, _| (this.path.clone(), this.search.scroll.0.clone()));
    std::fs::write(path.join("needle-new.txt"), "new").unwrap();
    h.view.update(&mut h.cx, |this, _| {
        this.refresh_search();
        assert_eq!(this.search.count, 1);
        assert_eq!(this.search.rows.len(), 1);
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    while h.read(|this, _| this.search.count != 2 || this.search.refreshing) {
        h.frame();
        h.read(|this, _| {
            assert!(!this.search.rows.is_empty());
            assert!(Rc::ptr_eq(&handle, &this.search.scroll.0));
        });
        assert!(Instant::now() < deadline, "search refresh timed out");
        h.cx.advance_clock(Duration::from_millis(20));
        std::thread::sleep(Duration::from_millis(3));
    }
}

#[test]
fn indexed_search_cache_reuse_incremental_refresh_and_replaced_visible_identity() {
    let mut h = Harness::new(&["报告-original.txt", "other.txt"], None);
    h.read(|this, _| {
        this.index
            .as_ref()
            .unwrap()
            .send(index::Action::Add(this.path.clone()))
            .unwrap()
    });
    h.wait("indexed fixture", |this, _| {
        !this.index_status.building && this.index_status.count == 2
    });
    h.key("ctrl-f");
    h.view.update(&mut h.cx, |this, cx| {
        this.search.definition.indexed = true;
        this.start_search("报".into(), cx);
    });
    h.wait("cold indexed result", |this, _| {
        this.search.done && this.search.rows.len() == 1
    });
    let handle = h.read(|this, _| this.search.scroll.0.clone());
    h.view.update(&mut h.cx, |this, _| this.refresh_search());
    h.wait("warm reused page", |this, _| {
        !this.search.refreshing && this.search.done && this.search.scanned == 0
    });
    let old = h.file("报告-original.txt");
    let preserved = h.file("preserved.txt");
    let revision = h.read(|this, _| this.index_status.revision);
    std::fs::rename(&old, &preserved).unwrap();
    std::fs::write(&old, b"replacement identity").unwrap();
    // Before the index watcher has necessarily caught up, page validation must
    // reject the old identity and route through the existing refresh handoff.
    h.view.update(&mut h.cx, |this, _| {
        this.search.serial += 1;
        this.search
            .worker
            .page(this.search.token, this.search.serial, 0..32);
        assert_eq!(this.search.rows.len(), 1);
    });
    h.wait("watcher and replacement refresh", |this, _| {
        this.search.done
            && this.index_status.revision > revision
            && !this.search.refreshing
            && this
                .search
                .rows
                .values()
                .any(|r| r.name == "报告-original.txt")
    });
    h.read(|this, _| assert!(Rc::ptr_eq(&handle, &this.search.scroll.0)));
    let new = h.file("报告-new.txt");
    std::fs::write(&new, b"new").unwrap();
    h.wait("incremental cache membership", |this, _| {
        this.search.done
            && !this.search.refreshing
            && this.search.count == 2
            && this.search.rows.len() == 2
    });
    h.read(|this, _| assert!(Rc::ptr_eq(&handle, &this.search.scroll.0)));
}

#[test]
fn indexed_root_case_rename_rebuilds_without_duplicates_and_keeps_nonrecursive_updates() {
    let mut h = Harness::new(&["报告-original.txt", "other.txt"], None);
    h.read(|this, _| {
        this.index
            .as_ref()
            .unwrap()
            .send(index::Action::Add(this.path.clone()))
            .unwrap()
    });
    h.wait("root indexed", |this, _| {
        !this.index_status.building && this.index_status.count == 2
    });
    h.key("ctrl-f");
    h.view.update(&mut h.cx, |this, cx| {
        this.search.definition.indexed = true;
        this.search.definition.recursive = false;
        this.start_search("报".into(), cx);
    });
    h.wait("nonrecursive root search", |this, _| {
        this.search.done && this.search.rows.len() == 1
    });
    let old = h.read(|this, _| this.path.clone());
    let new = h.fixture.0.join("FILES");
    assert_eq!(old.parent().unwrap(), h.fixture.0.as_path());
    assert_eq!(new.parent().unwrap(), h.fixture.0.as_path());
    std::fs::rename(&old, &new).unwrap();
    std::fs::write(old.join("报告-added.txt"), b"new").unwrap();
    let actual = new.canonicalize().unwrap();
    h.wait("case alias root migrated and rebuilt", |this, _| {
        this.index_status.roots == [actual.clone()]
            && !this.index_status.building
            && this.search.done
            && !this.search.refreshing
            && this.search.count == 2
            && this.search.rows.len() == 2
    });
    std::fs::write(new.join("报告-later.txt"), b"later").unwrap();
    h.wait(
        "replacement watcher and nonrecursive membership",
        |this, _| {
            !this.index_status.building
                && this.search.done
                && !this.search.refreshing
                && this.search.count == 3
                && this.search.rows.len() == 3
        },
    );
}

#[test]
fn new_folder_completion_refreshes_in_place_and_enters_rename() {
    let mut h = Harness::new(&["original.txt"], None);
    let handle = h.read(|this, _| this.scroll.0.clone());
    h.key("ctrl-shift-n");
    h.wait("new folder rename after atomic refresh", |this, _| {
        this.source.count() == 2 && this.files.editor.is_some()
    });
    h.read(|this, _| {
        assert!(Rc::ptr_eq(&handle, &this.scroll.0));
        assert!(matches!(
            this.files.editor.as_ref().unwrap().kind,
            crate::app_operations::EditKind::Rename(_)
        ));
    });
}

#[test]
fn right_click_new_folder_keeps_filename_focused_and_blur_saves_it() {
    let mut h = Harness::new(&["original.txt"], None);
    h.click(650., 350., MouseButton::Right);
    let popup = h.popup();
    h.key_in(popup, "home");
    h.key_in(popup, "enter");
    h.wait("menu-created folder editor", |this, cx| {
        this.source.count() == 2 && this.files.editor.is_some() && cx.windows().len() == 1
    });
    let input = h.editor();
    assert!(h.focused(&input));
    h.text("命名后的目录");
    h.key("tab");
    h.wait("folder name committed on blur", |this, _| {
        this.files.editor.is_none()
    });
    assert!(h.file("命名后的目录").is_dir());
    assert!(!h.file("新建文件夹").exists());
    h.read(|this, _| assert!(!this.files.open));
}

#[test]
fn late_directory_notification_during_new_folder_locate_preserves_rename_intent() {
    let mut h = Harness::new(&["original.txt"], None);
    std::fs::create_dir(h.file("新建文件夹")).unwrap();
    h.invalidate_directory();
    h.wait("created folder snapshot", |this, _| {
        this.cache.values().any(|i| i.name == "新建文件夹")
    });
    h.view.update(&mut h.cx, |this, cx| {
        let id = this
            .cache
            .values()
            .find(|i| i.name == "新建文件夹")
            .unwrap()
            .id;
        this.refresh_directory(Some("新建文件夹".into()), true, cx);
        // The previous snapshot already dispatched its locate request. A late
        // filesystem notification now makes request_detail refuse that snapshot.
        this.watch.locate = None;
        this.watch.dirty = true;
        this.files.locate_rename = true;
        this.files.locate_token += 1;
        this.receive_operation(
            &Event::Located {
                generation: this.generation,
                token: this.files.locate_token,
                id: Some(id),
            },
            cx,
        );
    });
    h.wait("rename survives a later directory generation", |this, _| {
        this.files.editor.is_some()
    });
    let input = h.editor();
    assert!(h.focused(&input));
    h.text("实际名称");
    h.key("enter");
    h.wait("rename persisted", |this, _| this.files.editor.is_none());
    assert!(h.file("实际名称").is_dir());
}

#[test]
fn short_successful_operations_do_not_open_or_leave_a_task_panel() {
    let mut h = Harness::new(&["original.txt"], None);
    h.key("ctrl-shift-n");
    h.wait("name editor", |this, _| this.files.editor.is_some());
    h.read(|this, _| {
        assert!(!this.files.open);
        assert!(this.visible_operations().is_empty());
    });
    h.text("quiet-folder");
    h.key("enter");
    h.wait("quiet creation and rename complete", |this, _| {
        this.files.editor.is_none() && this.visible_operations().is_empty()
    });
    assert!(h.file("quiet-folder").is_dir());
    h.read(|this, _| assert!(!this.files.open));
}

#[test]
fn visible_image_cells_decode_thumbnails_lazily() {
    let mut h = Harness::new(&["keep.txt"], None);
    let directory = h.fixture.0.join("files");
    let photo = directory.join("photo.png");
    image::RgbaImage::from_fn(64, 32, |x, y| {
        image::Rgba([(x * 4) as u8, (y * 8) as u8, 200, 255])
    })
    .save(&photo)
    .unwrap();
    let text = directory.join("notes.txt");
    std::fs::write(&text, b"notes").unwrap();
    h.key("f5");
    h.wait("image and text entries listed", |this, _| {
        this.source.count() == 3 && !this.watch_stale()
    });
    h.wait("visible image preview decoded", |this, _| {
        this.image_previews.loaded(&photo)
    });
    assert!(!h.read(|this, _| this.image_previews.loaded(&text)));
}

#[test]
fn image_preview_cache_survives_unchanged_directory_refresh() {
    let mut h = Harness::new(&["keep.txt"], None);
    let directory = h.fixture.0.join("files");
    let photo = directory.join("photo.png");
    image::RgbaImage::from_pixel(32, 32, image::Rgba([10, 20, 30, 255]))
        .save(&photo)
        .unwrap();
    h.key("f5");
    h.wait("image entry and preview", |this, _| {
        this.source.count() == 2 && this.image_previews.loaded(&photo)
    });
    let generation = h.read(|this, _| this.generation);
    h.view.update(&mut h.cx, |this, cx| {
        this.refresh_directory(None, false, cx);
    });
    h.wait("unchanged directory refresh", |this, _| {
        this.generation > generation && !this.watch_stale()
    });
    h.frame();
    assert!(h.read(|this, _| this.image_previews.loaded(&photo)));
}

#[test]
fn advanced_filter_capacity_errors_are_field_scoped_and_units_parse() {
    let mut h = Harness::new(&["keep.txt"], None);
    h.key("ctrl-f");
    h.text("k");
    h.wait("active search", |this, _| this.search.started);
    let view = h.view.clone();
    h.cx.update_window(h.window.into(), |_, window, cx| {
        view.update(cx, |this, cx| this.toggle_view_filters(window, cx));
    })
    .unwrap();
    h.wait("filter form open", |this, _| {
        this.filter_count_for_test() == 4
    });
    h.view.update(&mut h.cx, |this, cx| {
        this.set_filter_value_for_test(2, "10XB", cx);
        assert!(!this.apply_filters_for_test(cx));
        assert!(this.filter_error_for_test(2).unwrap().contains("KB"));
        this.set_filter_value_for_test(2, "10MB", cx);
        this.set_filter_value_for_test(3, "1MB", cx);
        assert!(!this.apply_filters_for_test(cx));
        assert!(this.filter_error_for_test(3).unwrap().contains("最小"));
        this.set_filter_value_for_test(3, "20MB", cx);
        assert!(this.apply_filters_for_test(cx));
        assert_eq!(this.search.definition.min_bytes, Some(10 * 1024 * 1024));
        assert_eq!(this.search.definition.max_bytes, Some(20 * 1024 * 1024));
    });
}

#[test]
fn finished_non_retryable_operations_hide_badge_and_panel() {
    use crate::operations::service::{Operation, Phase};
    let mut h = Harness::new(&["original.txt"], None);
    h.view.update(&mut h.cx, |this, cx| {
        let id = this.files.hub.queued_for_test(Operation::Copy);
        this.files.open = true;
        assert_eq!(this.visible_operations().len(), 1);
        this.files
            .hub
            .finish_for_test(id, Phase::Failed, 1, 0, false);
        this.operations_changed(cx);
        assert!(this.visible_operations().is_empty());
        assert!(!this.files.open);

        let retry = this.files.hub.queued_for_test(Operation::Move);
        this.files
            .hub
            .finish_for_test(retry, Phase::Failed, 1, 1, false);
        assert_eq!(this.visible_operations().len(), 1);
    });
}

#[test]
fn new_folder_in_equivalent_display_path_uses_snapshot_destination_identity() {
    let mut h = Harness::new(&["original.txt"], None);
    h.view.update(&mut h.cx, |this, cx| {
        let alias = this.path.join("..").join("files");
        this.navigate(alias, false, cx);
    });
    h.wait("equivalent path snapshot", |this, _| {
        this.source.finished() && !this.watch_stale() && !this.cache.is_empty()
    });
    h.key("ctrl-shift-n");
    h.wait("new folder at an equivalent display path", |this, _| {
        this.files.editor.is_some()
    });
    let input = h.editor();
    assert!(h.focused(&input));
    h.text("alias-folder");
    h.key("enter");
    h.wait("alias rename committed", |this, _| {
        this.files.editor.is_none()
    });
    assert!(h.file("alias-folder").is_dir());
}

#[test]
fn slow_operations_reveal_once_without_reopening_after_escape() {
    let mut h = Harness::new(&["original.txt"], None);
    h.view.update(&mut h.cx, |this, cx| {
        let id = this
            .files
            .hub
            .queued_for_test(operations::service::Operation::Copy);
        this.show_operation(id, cx);
    });
    h.frame();
    assert!(!h.read(|this, _| this.files.open));
    h.cx.advance_clock(Duration::from_millis(599));
    h.frame();
    assert!(!h.read(|this, _| this.files.open));
    h.cx.advance_clock(Duration::from_millis(1));
    h.frame();
    assert!(h.read(|this, _| this.files.open));
    h.key("escape");
    assert!(!h.read(|this, _| this.files.open));
    h.cx.advance_clock(Duration::from_secs(2));
    h.frame();
    assert!(!h.read(|this, _| this.files.open));
}

#[test]
fn repeated_letters_cycle_and_wrap_without_intercepting_text_inputs() {
    let mut h = Harness::new(
        &["alpha.txt", "apple.txt", "apricot.txt", "banana.txt"],
        None,
    );
    let matches = h.read(|this, _| {
        this.cache
            .values()
            .filter(|i| i.name.starts_with('a'))
            .map(|i| i.id)
            .collect::<Vec<_>>()
    });
    h.key("a");
    h.wait("first letter match", |this, _| {
        this.selection.focus == Some(matches[0])
    });
    h.key("a");
    h.wait("second letter match", |this, _| {
        this.selection.focus == Some(matches[1])
    });
    h.cx.update_window(h.window.into(), |_, window, cx| {
        for _ in 0..2 {
            window.dispatch_keystroke(
                Keystroke::parse_with_os("a", OperatingSystem::Windows).unwrap(),
                cx,
            );
        }
    })
    .unwrap();
    h.wait("coalesced repetitions wrap", |this, _| {
        this.selection.focus == Some(matches[0]) && !this.typeahead.pending
    });
    h.key("escape");
    h.key("b");
    h.key("a");
    h.wait("multi-letter prefix", |this, _| {
        this.selection
            .focus
            .and_then(|id| this.cache.get(&id))
            .is_some_and(|i| i.name == "banana.txt")
    });
    h.key("ctrl-f");
    h.text("aa");
    h.read(|this, cx| assert_eq!(this.search.input.as_ref().unwrap().read(cx).value(), "aa"));
}

#[test]
fn pending_letter_activation_finishes_before_directory_refresh_commits() {
    let mut h = Harness::new(&["alpha.txt", "other.txt"], None);
    let (path, generation, id, token) = h.read(|this, _| {
        (
            this.path.clone(),
            this.generation,
            this.cache
                .values()
                .find(|i| i.name == "alpha.txt")
                .unwrap()
                .id,
            this.typeahead.token,
        )
    });
    h.view
        .update(&mut h.cx, |this, _| this.typeahead.pending = true);
    std::fs::write(path.join("new.txt"), "new").unwrap();
    h.invalidate_directory();
    h.read(|this, _| {
        assert_eq!(this.generation, generation);
        assert!(this.watch.pending.is_none());
    });
    h.view.update(&mut h.cx, |this, cx| {
        this.receive(
            Event::PrefixLocated {
                generation,
                token,
                id: Some(id),
            },
            cx,
        )
    });
    h.read(|this, _| assert_eq!(this.selection.focus, Some(id)));
    h.wait("refresh retains completed letter activation", |this, _| {
        this.source.count() == 3 && !this.watch_stale()
    });
    h.read(|this, _| assert_eq!(this.cache[&this.selection.focus.unwrap()].name, "alpha.txt"));
}

#[test]
fn selected_bytes_include_empty_files_and_never_recurse_into_folders() {
    let mut h = Harness::new(&["a.txt", "zero.dat"], None);
    let path = h.read(|this, _| this.path.clone());
    std::fs::write(path.join("zero.dat"), []).unwrap();
    std::fs::create_dir(path.join("folder")).unwrap();
    std::fs::write(path.join("folder/ignored.bin"), vec![1u8; 8192]).unwrap();
    h.invalidate_directory();
    h.wait("updated file sizes", |this, _| {
        !this.watch_stale() && this.source.count() == 3
    });
    h.view.update(&mut h.cx, |this, cx| {
        this.selection.all(3);
        cx.notify();
    });
    h.wait("file-only byte total", |this, _| {
        this.stats.result.as_ref().is_some_and(|r| {
            r.as_ref().is_ok_and(|t| {
                t.files == 2 && t.bytes == "contents of a.txt".len() as u128 && t.unknown == 0
            })
        })
    });
    let folder = h.read(|this, _| this.cache.values().find(|i| i.directory).unwrap().id);
    h.view.update(&mut h.cx, |this, cx| {
        this.selection.select(folder, false, false);
        cx.notify();
    });
    h.wait("folder-only selection has no fake zero bytes", |this, _| {
        this.stats
            .result
            .as_ref()
            .is_some_and(|r| r.as_ref().is_ok_and(|t| t.files == 0))
    });
    h.read(|this, _| assert!(this.stats.suffix().is_none()));
}

#[test]
fn volume_capacity_updates_keep_sidebar_geometry_and_ignore_stale_or_removed_rows() {
    let mut h = Harness::new(&[], Some(1000));
    let rows = (0..20)
        .map(|i| navigation::Location {
            title: format!("Disk {i}"),
            path: PathBuf::from(format!("{}:\\", (b'C' + i as u8) as char)),
            volume: true,
            entry: None,
            capacity: None,
        })
        .collect::<Vec<_>>();
    let path = rows[0].path.clone();
    h.view.update(&mut h.cx, |this, cx| {
        this.navigation.common.clear();
        this.accept_navigation(
            navigation::Notice::VolumeSnapshot {
                epoch: 0,
                result: Ok(rows),
            },
            cx,
        );
    });
    h.frame();
    let before =
        h.cx.update_window(h.window.into(), |_, window, cx| {
            h.view
                .read(cx)
                .scroll_geometry(app_scroll::Kind::Sidebar, window)
                .unwrap()
        })
        .unwrap();
    let capacity = crate::volume_capacity::Capacity {
        total: 1000,
        free: 250,
        available: 200,
    };
    h.view.update(&mut h.cx, |this, cx| {
        this.accept_navigation(
            navigation::Notice::Capacity {
                epoch: 1,
                path: path.clone(),
                title: None,
                value: Some(capacity),
            },
            cx,
        );
        assert!(this.navigation.volumes[0].capacity.is_none());
        this.accept_navigation(
            navigation::Notice::Capacity {
                epoch: 0,
                path: path.clone(),
                title: None,
                value: Some(capacity),
            },
            cx,
        );
    });
    h.frame();
    let after =
        h.cx.update_window(h.window.into(), |_, window, cx| {
            h.view
                .read(cx)
                .scroll_geometry(app_scroll::Kind::Sidebar, window)
                .unwrap()
        })
        .unwrap();
    assert_eq!(before.viewport, after.viewport);
    assert_eq!(before.maximum, after.maximum);
    h.view.update(&mut h.cx, |this, cx| {
        assert_eq!(this.navigation.volumes[0].capacity, Some(capacity));
        this.accept_navigation(
            navigation::Notice::VolumeSnapshot {
                epoch: 0,
                result: Ok(vec![]),
            },
            cx,
        );
        this.accept_navigation(
            navigation::Notice::Capacity {
                epoch: 0,
                path,
                title: None,
                value: Some(capacity),
            },
            cx,
        );
        assert!(this.navigation.volumes.is_empty());
    });
}

#[test]
fn directory_refresh_preserves_scroll_handle_and_visible_anchor() {
    let names: Vec<_> = (0..600).map(|i| format!("file-{i:04}.txt")).collect();
    let refs: Vec<_> = names.iter().map(String::as_str).collect();
    let mut h = Harness::new(&refs, None);
    let view = h.view.clone();
    h.cx.update_window(h.window.into(), |_, window, cx| {
        view.update(cx, |this, cx| {
            this.set_scroll_offset(app_scroll::Kind::Files, 1800., window, cx)
        })
    })
    .unwrap();
    h.wait("scrolled viewport", |this, _| {
        this.requested.start > 0
            && this
                .cache
                .contains_key(&((1800. / layout::row_step()).floor() as usize * this.columns))
    });
    let (handle, top_name, generation, path, offset) = h.read(|this, _| {
        let offset = this.origin_row as f64 * f64::from(layout::row_step())
            - f64::from(f32::from(this.scroll.0.borrow().base_handle.offset().y));
        let id = (offset / f64::from(layout::row_step())).floor() as usize * this.columns;
        (
            this.scroll.0.clone(),
            this.cache[&id].name.clone(),
            this.generation,
            this.path.clone(),
            offset,
        )
    });
    std::fs::write(path.join("new.txt"), "new").unwrap();
    h.invalidate_directory();
    h.wait("atomic viewport commit", |this, _| {
        this.generation != generation
    });
    h.read(|this, _| {
        assert!(Rc::ptr_eq(&handle, &this.scroll.0));
        let item = this
            .cache
            .values()
            .find(|i| i.name == top_name)
            .expect("visible anchor remains cached");
        let now = this.origin_row as f64 * f64::from(layout::row_step())
            - f64::from(f32::from(this.scroll.0.borrow().base_handle.offset().y));
        assert_eq!(
            (now / f64::from(layout::row_step())).floor() as usize,
            item.id / this.columns
        );
        assert!(
            (now % f64::from(layout::row_step()) - offset % f64::from(layout::row_step())).abs()
                < 0.5
        );
    });
}

#[test]
fn refresh_failure_keeps_old_rows_and_late_results_cannot_replace_navigation() {
    let mut h = Harness::new(&["original.txt"], None);
    let (path, generation) = h.read(|this, _| (this.path.clone(), this.generation));
    let away = h.fixture.0.join("away");
    std::fs::rename(&path, &away).unwrap();
    h.invalidate_directory();
    h.wait("failed refresh retains old snapshot", |this, _| {
        this.watch.pending.is_none()
    });
    h.read(|this, _| {
        assert_eq!(this.generation, generation);
        assert_eq!(this.source.count(), 1);
        assert_eq!(this.cache.values().next().unwrap().name, "original.txt");
    });
    std::fs::rename(&away, &path).unwrap();
    h.invalidate_directory();
    let pending = h.read(|this, _| this.watch.pending.as_ref().unwrap().generation);
    let target = h.fixture.0.join("target");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("target.txt"), "target").unwrap();
    h.view.update(&mut h.cx, |this, cx| {
        this.navigate(target.clone(), true, cx)
    });
    h.view.update(&mut h.cx, |this, cx| {
        this.receive(
            Event::Count {
                generation: pending,
                count: 999,
                done: true,
                error: None,
                denied: false,
            },
            cx,
        )
    });
    h.wait("new directory wins", |this, _| {
        this.path == target
            && this.source.finished()
            && this.cache.values().any(|i| i.name == "target.txt")
    });
    h.read(|this, _| {
        assert!(this.generation > pending);
        assert_eq!(this.source.count(), 1);
        assert!(this.watch.pending.is_none());
    });
}

fn extension_item(id: u32, submenu: bool) -> crate::shell_menu::Item {
    crate::shell_menu::Item {
        id,
        label: format!("应用命令 {id}"),
        hint: String::new(),
        enabled: true,
        checked: false,
        radio: false,
        submenu,
        separator: false,
        unavailable: false,
    }
}

#[test]
fn async_shell_discovery_and_large_submenu_never_resize_or_move_the_root() {
    let mut h = Harness::new(&["note.txt"], None);
    let (x, y) = h.first_icon();
    h.click(x, y, MouseButton::Right);
    let root = h.popup();
    let before = h.window_bounds(root);
    let menu = h.shell_menu();
    assert_eq!(h.window_bounds(root), before);
    h.shell_rows(
        &menu,
        0,
        (1..=50).map(|id| extension_item(id, false)).collect(),
    );
    assert_eq!(h.window_bounds(root), before);
    h.key_in(root, "end");
    h.key_in(root, "right");
    h.wait("independent applications column", |_, cx| {
        cx.windows().len() == 3
    });
    assert_eq!(h.window_bounds(root), before);
    for _ in 0..8 {
        h.frame();
        assert_eq!(h.window_bounds(root), before);
    }
    h.key_in(root, "left");
    h.wait("applications column closed", |_, cx| {
        cx.windows().len() == 2
    });
    assert_eq!(h.window_bounds(root), before);
    h.key_in(root, "escape");
    h.wait("entire menu closed", |this, cx| {
        this.files.menu.is_none() && cx.windows().len() == 1
    });
    assert!(h.grid_focused());
}

#[test]
fn dynamic_child_waits_for_rows_and_keeps_both_ancestors_stable() {
    let mut h = Harness::new(&["before.txt"], None);
    let (x, y) = h.first_icon();
    h.click(x, y, MouseButton::Right);
    let root = h.popup();
    let root_bounds = h.window_bounds(root);
    let menu = h.shell_menu();
    h.shell_rows(&menu, 0, vec![extension_item(11, true)]);
    h.key_in(root, "end");
    h.key_in(root, "right");
    h.wait("applications column", |_, cx| cx.windows().len() == 3);
    let owner = h.window.into();
    let app = h.cx.update(|cx| {
        cx.windows()
            .into_iter()
            .find(|w| *w != root && *w != owner)
            .unwrap()
    });
    let app_bounds = h.window_bounds(app);
    h.key_in(root, "home");
    h.key_in(root, "right");
    h.cx.advance_clock(Duration::from_millis(300));
    h.frame();
    assert_eq!(
        h.cx.update(|cx| cx.windows().len()),
        3,
        "a loading placeholder must not create a resizing child surface"
    );
    assert_eq!(h.window_bounds(root), root_bounds);
    assert_eq!(h.window_bounds(app), app_bounds);
    h.shell_rows(
        &menu,
        11,
        (100..130).map(|id| extension_item(id, false)).collect(),
    );
    h.wait("ready dynamic child", |_, cx| cx.windows().len() == 4);
    assert_eq!(h.window_bounds(root), root_bounds);
    assert_eq!(h.window_bounds(app), app_bounds);
    h.key_in(root, "f2");
    h.wait("owner rename after nested menus", |this, cx| {
        this.files.editor.is_some() && this.files.menu.is_none() && cx.windows().len() == 1
    });
    let editor = h.editor();
    assert!(h.focused(&editor));
    h.key("escape");
}

#[test]
fn language_shortcut_switches_chinese_and_english() {
    let mut h = Harness::new(&["note.txt"], None);
    h.key("ctrl-shift-l");
    assert_eq!(
        h.read(|_, _| crate::i18n::current()),
        crate::i18n::Language::En
    );
    assert_eq!(crate::i18n::tr("打开"), "Open");
    h.key("ctrl-shift-l");
    assert_eq!(
        h.read(|_, _| crate::i18n::current()),
        crate::i18n::Language::Zh
    );
    assert_eq!(crate::i18n::tr("打开"), "打开");
}

#[test]
fn content_results_scrollbar_tracks_overflow_and_pages_the_list() {
    let mut h = Harness::new(&["note.txt"], None);
    let view = h.view.clone();
    h.cx.update_window(h.window.into(), |_, _, cx| {
        view.update(cx, |this, cx| {
            this.search.open = true;
            this.search.content_mode = true;
            this.search.content_results = (0..200)
                .map(|index| crate::content_search::Found {
                    path: this.path.join(format!("note-{index}.txt")),
                    name: format!("note-{index}.txt"),
                    line: 1,
                    snippet: "line with needle".into(),
                    highlights: std::iter::once(10..16).collect(),
                    matches: 1,
                })
                .collect();
            cx.notify();
        })
    })
    .unwrap();
    h.frame();
    let geometry =
        h.cx.update_window(h.window.into(), |_, window, cx| {
            view.read_with(cx, |this, _| {
                this.scroll_geometry(crate::app_scroll::Kind::Content, window)
            })
        })
        .unwrap()
        .expect("content scrollbar for overflowing results");
    assert!(geometry.maximum > 0. && geometry.thumb < geometry.viewport as f32);
    let x = 1120.
        - crate::layout::SURFACE.margin_x
        - crate::layout::SCROLL.rail_inset
        - crate::layout::SCROLL.rail_width / 2.;
    let y = crate::layout::CHROME.bar_height + crate::layout::SURFACE.margin_y + 78. + 200.;
    h.click(x, y, MouseButton::Left);
    h.frame();
    assert!(
        h.cx.update_window(h.window.into(), |_, window, cx| {
            view.read_with(cx, |this, _| {
                this.scroll_geometry(crate::app_scroll::Kind::Content, window)
                    .is_some_and(|g| g.offset > 0.)
            })
        })
        .unwrap()
    );
}
#[test]
fn content_search_finds_text_with_context_and_highlights() {
    let mut h = Harness::new(&["note.txt"], None);
    h.key("ctrl-f");
    h.text("contents");
    h.wait("name search debounce", |this, _| this.search.started);
    let view = h.view.clone();
    h.cx.update_window(h.window.into(), |_, window, cx| {
        view.update(cx, |this, cx| this.toggle_content_mode(window, cx));
    })
    .unwrap();
    h.wait("content search result", |this, _| {
        this.search.content_done && !this.search.content_results.is_empty()
    });
    h.read(|this, _| {
        let found = &this.search.content_results[0];
        assert_eq!(found.name, "note.txt");
        assert!(found.snippet.contains("contents"));
        let range = found.highlights.first().expect("highlight").clone();
        assert_eq!(&found.snippet[range], "contents");
    });
}

#[test]
fn empty_async_shell_submenu_keeps_a_readable_empty_row() {
    let mut h = Harness::new(&["note.txt"], None);
    let (x, y) = h.first_icon();
    h.click(x, y, MouseButton::Right);
    let root = h.popup();
    let menu = h.shell_menu();
    h.shell_rows(&menu, 0, vec![extension_item(11, true)]);
    h.key_in(root, "end");
    h.key_in(root, "right");
    h.wait("applications column", |_, cx| cx.windows().len() == 3);
    h.key_in(root, "home");
    h.key_in(root, "right");
    h.cx.advance_clock(Duration::from_millis(300));
    h.frame();
    assert_eq!(
        h.cx.update(|cx| cx.windows().len()),
        3,
        "an unready async submenu must not open a blank surface"
    );
    h.shell_rows(&menu, 11, vec![]);
    h.wait("empty shell submenu", |_, cx| cx.windows().len() == 4);
    let height =
        h.cx.update(|cx| menu.read(cx).column_geometry(2).unwrap().height);
    assert!(
        height > 40.,
        "empty submenu must reserve a normal row instead of a 32px sliver: {height}"
    );
    h.key_in(root, "escape");
    h.wait("empty submenu dismissed", |this, cx| {
        this.files.menu.is_none() && cx.windows().len() == 1
    });
}

#[test]
fn late_dynamic_rows_do_not_revive_closed_columns_or_the_menu_family() {
    let mut h = Harness::new(&["note.txt"], None);
    let (x, y) = h.first_icon();
    h.click(x, y, MouseButton::Right);
    let root = h.popup();
    let menu = h.shell_menu();
    h.shell_rows(&menu, 0, vec![extension_item(11, true)]);
    h.key_in(root, "end");
    h.key_in(root, "right");
    h.wait("applications column", |_, cx| cx.windows().len() == 3);
    h.key_in(root, "home");
    h.key_in(root, "right");
    let owner = h.window.into();
    let app = h.cx.update(|cx| {
        cx.windows()
            .into_iter()
            .find(|w| *w != root && *w != owner)
            .unwrap()
    });
    h.event(
        app,
        MouseMoveEvent {
            position: point(px(100.), px(30.)),
            ..Default::default()
        }
        .to_platform_input(),
    );
    h.event(
        app,
        ScrollEvent {
            position: point(px(100.), px(30.)),
            delta: ScrollDelta::Pixels(point(px(0.), px(-32.))),
            ..Default::default()
        }
        .to_platform_input(),
    );
    h.shell_rows(&menu, 11, vec![extension_item(99, false)]);
    assert_eq!(h.cx.update(|cx| cx.windows().len()), 3);
    h.key_in(root, "escape");
    h.wait("dismiss family", |this, cx| {
        this.files.menu.is_none() && cx.windows().len() == 1
    });
    h.shell_rows(&menu, 11, vec![extension_item(100, false)]);
    h.shell_rows(&menu, 0, vec![extension_item(200, true)]);
    assert_eq!(h.cx.update(|cx| cx.windows().len()), 1);
    assert!(h.grid_focused());
}

#[gpui::test]
fn headless_platform_is_windowless(cx: &mut TestAppContext) {
    let (_, cx) = cx.add_window_view(|_, _| Empty);
    cx.update(|window, _| {
        assert!(raw_window_handle::HasWindowHandle::window_handle(window).is_err());
    });
}

#[test]
fn menu_family_focus_and_bottom_flip_keep_ancestors_stable() {
    let mut h = Harness::new(&["note.txt"], None);
    h.click(1080., 690., MouseButton::Right);
    let root = h.popup();
    let before = h.window_bounds(root);
    let menu = h.shell_menu();
    h.shell_rows(
        &menu,
        0,
        (1..=40).map(|id| extension_item(id, false)).collect(),
    );
    h.key_in(root, "end");
    h.key_in(root, "right");
    h.wait("bottom edge child", |_, cx| cx.windows().len() == 3);
    let owner = h.window.into();
    let child = h.cx.update(|cx| {
        cx.windows()
            .into_iter()
            .find(|w| *w != owner && *w != root)
            .unwrap()
    });
    let bounds = h.window_bounds(child);
    assert!(
        bounds.origin.x < before.origin.x,
        "submenu must flip independently"
    );
    assert!(
        f32::from(bounds.origin.y) >= 0. && f32::from(bounds.origin.y + bounds.size.height) <= 720.
    );
    assert_eq!(h.window_bounds(root), before);
    h.cx.update_window(child, |_, window, _| window.activate_window())
        .unwrap();
    h.frame();
    h.cx.advance_clock(Duration::from_millis(100));
    h.frame();
    assert_eq!(
        h.cx.update(|cx| cx.windows().len()),
        3,
        "family activation dismissed its parent"
    );
    assert_eq!(h.window_bounds(root), before);
    h.click(350., 300., MouseButton::Left);
    h.wait("outside activation dismisses entire family", |this, cx| {
        this.files.menu.is_none() && cx.windows().len() == 1
    });
    assert!(h.grid_focused());
}

#[test]
fn rename_blur_commits_real_file_and_preserves_bytes() {
    let mut h = Harness::new(&["before.txt"], None);
    let original = std::fs::read(h.file("before.txt")).unwrap();
    h.rename();
    h.text("after");
    // Tab exercises TextInput's actual Blur subscription, without the grid's
    // independent blank-click commit path masking a missing blur handler.
    h.key("tab");
    h.wait("rename blur completion", |this, _| {
        this.files.editor.is_none()
    });
    assert!(!h.file("before.txt").exists());
    assert_eq!(std::fs::read(h.file("after.txt")).unwrap(), original);
}

#[test]
fn rename_invalid_blur_preserves_draft_and_refocuses() {
    let mut h = Harness::new(&["before.txt"], None);
    h.rename();
    h.text("bad?");
    h.click(800., 400., MouseButton::Left);
    h.wait("rename error", |this, _| {
        this.files
            .editor
            .as_ref()
            .is_some_and(|e| e.error.is_some())
    });
    let input = h.editor();
    assert!(h.focused(&input));
    assert_eq!(h.read(|_, cx| input.read(cx).value()), "bad?.txt");
    assert!(h.file("before.txt").exists());
    h.alert();
    h.key("ctrl-a");
    h.text("recovered.txt");
    h.click(800., 400., MouseButton::Left);
    h.wait("rename recovery", |this, _| this.files.editor.is_none());
    assert!(h.file("recovered.txt").exists());
}

#[test]
fn rename_escape_cancels_without_filesystem_change() {
    let mut h = Harness::new(&["before.txt"], None);
    let original = std::fs::read(h.file("before.txt")).unwrap();
    h.rename();
    h.text("discarded");
    h.key("escape");
    assert!(h.read(|this, _| this.files.editor.is_none()));
    assert!(h.grid_focused());
    assert_eq!(std::fs::read(h.file("before.txt")).unwrap(), original);
    assert!(!h.file("discarded.txt").exists());
}

#[test]
fn rename_case_only_remains_supported_and_preserves_content() {
    let mut h = Harness::new(&["before.txt"], None);
    let bytes = std::fs::read(h.file("before.txt")).unwrap();
    h.rename();
    h.key("ctrl-a");
    h.text("BEFORE.txt");
    h.click(800., 400., MouseButton::Left);
    h.wait("case-only rename", |this, _| this.files.editor.is_none());
    assert_eq!(std::fs::read(h.file("BEFORE.txt")).unwrap(), bytes);
    assert_eq!(
        std::fs::read_dir(h.fixture.0.join("files"))
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect::<Vec<_>>(),
        vec![std::ffi::OsString::from("BEFORE.txt")]
    );
}

#[test]
fn rename_existing_destination_reports_error_without_overwrite() {
    let mut h = Harness::new(&["before.txt", "occupied.txt"], None);
    let original = std::fs::read(h.file("before.txt")).unwrap();
    let occupied = std::fs::read(h.file("occupied.txt")).unwrap();
    h.rename();
    h.key("ctrl-a");
    h.text("occupied.txt");
    h.click(800., 400., MouseButton::Left);
    h.wait("filesystem rename error", |this, cx| {
        this.files
            .editor
            .as_ref()
            .is_some_and(|e| e.error.is_some() && e.input.read(cx).enabled)
    });
    let input = h.editor();
    assert!(h.focused(&input));
    h.alert();
    assert_eq!(h.read(|_, cx| input.read(cx).value()), "occupied.txt");
    assert_eq!(std::fs::read(h.file("before.txt")).unwrap(), original);
    assert_eq!(std::fs::read(h.file("occupied.txt")).unwrap(), occupied);
}

#[test]
fn path_blur_error_restores_draft_then_navigates_on_correction() {
    let mut h = Harness::new(&["note.txt"], None);
    std::fs::create_dir(h.file("child")).unwrap();
    h.key("ctrl-l");
    let input = h.editor();
    assert!(h.focused(&input));
    h.text("missing-child");
    h.click(800., 400., MouseButton::Left);
    h.wait("path resolution error", |this, _| {
        this.files
            .editor
            .as_ref()
            .is_some_and(|e| e.error.is_some())
    });
    assert!(h.focused(&input));
    h.alert();
    assert_eq!(h.read(|_, cx| input.read(cx).value()), "missing-child");
    assert_eq!(
        h.read(|this, _| this.path.clone()),
        h.fixture.0.join("files")
    );
    h.key("ctrl-a");
    h.text("child");
    h.click(800., 400., MouseButton::Left);
    h.wait("corrected path navigation", |this, _| {
        this.files.editor.is_none() && this.path.file_name().is_some_and(|n| n == "child")
    });
    assert_eq!(h.read(|this, _| this.path.clone()), h.file("child"));
}

#[test]
fn search_unicode_input_submit_and_escape_use_real_input_routing() {
    let mut h = Harness::new(&["中文报告.txt", "alpha.txt", "beta.txt"], None);
    h.key("ctrl-f");
    let input = h.read(|this, _| this.search.input.clone().expect("search input"));
    assert!(h.focused(&input));
    h.text("中文");
    assert_eq!(h.read(|_, cx| input.read(cx).value()), "中文");
    assert_eq!(h.read(|this, _| this.selection.count()), 0);
    h.key("enter");
    h.wait("Unicode search results", |this, _| {
        this.search.done && this.search.count == 1 && !this.search.rows.is_empty()
    });
    assert!(h.read(|this, _| {
        this.search
            .rows
            .values()
            .all(|r| r.path.file_name().is_some_and(|n| n == "中文报告.txt"))
    }));
    h.key("escape");
    assert!(!h.read(|this, _| this.search.open));
    assert!(h.grid_focused());
}

#[test]
fn search_debounce_executes_latest_query_after_250ms() {
    let mut h = Harness::new(&["alpha.txt", "beta.txt"], None);
    h.key("ctrl-f");
    h.text("alpha");
    h.key("ctrl-a");
    h.text("beta");
    h.cx.advance_clock(Duration::from_millis(249));
    h.frame();
    assert!(!h.read(|this, _| this.search.started));
    h.cx.advance_clock(Duration::from_millis(1));
    h.wait("latest debounced query", |this, _| {
        this.search.done
            && this.search.query == "beta"
            && this.search.count == 1
            && !this.search.rows.is_empty()
    });
    assert!(h.read(|this, _| {
        this.search
            .rows
            .values()
            .all(|r| r.path.file_name().is_some_and(|n| n == "beta.txt"))
    }));
}

#[test]
fn advanced_filters_require_nonempty_query() {
    let mut h = Harness::new(&["alpha.txt", "beta.txt"], None);
    h.key("ctrl-f");
    let view = h.view.clone();
    h.cx.update_window(h.window.into(), |_, window, cx| {
        view.update(cx, |this, cx| this.toggle_view_filters(window, cx));
    })
    .unwrap();
    h.frame();
    assert!(!h.read(|this, _| this.view_filters_open()));
    assert!(!h.read(|this, _| this.search.started));
    assert_eq!(h.read(|this, _| this.search.count), 0);
}

#[test]
fn empty_search_query_does_not_keep_search_surface_open() {
    let mut h = Harness::new(&["alpha.txt", "beta.txt"], None);
    h.key("ctrl-f");
    assert!(h.read(|this, _| this.search.open));
    assert!(
        h.read(|this, cx| !this.search_popover_visible(cx)),
        "empty search opened the results popover"
    );
    h.click(800., 400., MouseButton::Left);
    h.wait("empty search click-away", |this, _| !this.search.open);
    assert!(h.grid_focused());
    h.key("ctrl-f");
    let input = h.read(|this, _| this.search.input.clone().expect("search input"));
    h.text("alpha");
    h.wait("alpha search", |this, _| {
        this.search.done && this.search.count == 1 && this.search.query == "alpha"
    });
    assert!(h.read(|this, cx| this.search_popover_visible(cx)));
    h.key("ctrl-a");
    h.key("backspace");
    h.wait("cleared search exits", |this, _| !this.search.open);
    assert!(h.grid_focused());
    assert!(h.read(|_, cx| input.read(cx).value().is_empty()));
}

#[test]
fn automatic_search_uses_covered_index_and_falls_back_after_removal() {
    let mut h = Harness::new(&["自动索引报告.txt", "other.txt"], None);
    h.read(|this, _| {
        this.index
            .as_ref()
            .unwrap()
            .send(index::Action::Add(this.path.clone()))
            .unwrap()
    });
    h.wait("indexed fixture", |this, _| {
        !this.index_status.building && this.index_status.count == 2
    });
    h.key("ctrl-f");
    h.text("自动索引");
    h.wait("automatic indexed result", |this, _| {
        this.search.done && this.search.rows.len() == 1 && this.search.worker.used_index()
    });
    let canonical = h.read(|this, _| this.path.canonicalize().unwrap());
    h.read(move |this, _| {
        this.index
            .as_ref()
            .unwrap()
            .send(index::Action::Remove(canonical))
            .unwrap()
    });
    h.wait("index root removed", |this, _| {
        this.index_status.roots.is_empty() && !this.index_status.building
    });
    h.view.update(&mut h.cx, |this, cx| {
        this.start_search("自动索引".into(), cx);
    });
    h.wait("fallback live result", |this, _| {
        this.search.done && this.search.rows.len() == 1 && !this.search.worker.used_index()
    });
}

#[test]
fn titlebar_press_dismisses_path_tree_and_search_but_keeps_search_field_presses() {
    let mut h = Harness::new(&["note.txt"], None);
    let view = h.view.clone();
    h.cx.update_window(h.window.into(), |_, window, cx| {
        view.update(cx, |this, cx| {
            this.navigation.rows.clear();
            this.show_path_tree(true, Some(window), cx);
        })
    })
    .unwrap();
    h.frame();
    assert!(h.read(|this, _| this.navigation.open));
    h.click(820., 24., MouseButton::Left);
    assert!(!h.read(|this, _| this.navigation.open));
    assert!(h.grid_focused());

    h.key("ctrl-f");
    assert!(h.read(|this, _| this.search.open));
    h.click(800., 24., MouseButton::Left);
    assert!(h.read(|this, _| this.search.open));
    h.click(84., 24., MouseButton::Left);
    assert!(!h.read(|this, _| this.search.open));
    assert!(h.grid_focused());
}

#[test]
fn independent_menu_escape_restores_grid_focus_and_selection() {
    let mut h = Harness::new(&["note.txt"], None);
    let (x, y) = h.first_icon();
    h.click(x, y, MouseButton::Right);
    let popup = h.popup();
    h.cx.update_window(popup, |_, window, _| {
        assert!(raw_window_handle::HasWindowHandle::window_handle(window).is_err())
    })
    .unwrap();
    h.key_in(popup, "escape");
    h.wait("popup dismissal", |this, cx| {
        this.files.menu.is_none() && cx.windows().len() == 1
    });
    assert!(h.grid_focused());
    assert!(h.read(|this, _| this.selection.count() == 1 && this.selection.contains(0)));
}

#[test]
fn independent_menu_owner_activation_dismisses_popup_and_restores_focus() {
    let mut h = Harness::new(&["note.txt"], None);
    let (x, y) = h.first_icon();
    h.click(x, y, MouseButton::Right);
    h.popup();
    h.click(800., 400., MouseButton::Left);
    h.wait("menu owner activation", |this, cx| {
        this.files.menu.is_none() && cx.windows().len() == 1
    });
    assert!(h.grid_focused());
}

#[test]
fn independent_menu_rename_returns_action_to_owner_and_focuses_editor() {
    let mut h = Harness::new(&["before.txt"], None);
    let (x, y) = h.first_icon();
    h.click(x, y, MouseButton::Right);
    let popup = h.popup();
    h.key_in(popup, "f2");
    h.wait("menu rename editor", |this, cx| {
        this.files.editor.is_some() && this.files.menu.is_none() && cx.windows().len() == 1
    });
    let input = h.editor();
    assert!(h.focused(&input));
    h.text("after-menu");
    h.click(800., 400., MouseButton::Left);
    h.wait("menu rename completion", |this, _| {
        this.files.editor.is_none()
    });
    assert!(h.file("after-menu.txt").exists());
    assert!(!h.file("before.txt").exists());
}

#[test]
fn rename_blur_then_other_file_context_menu_completes_both_actions() {
    let mut h = Harness::new(&["before.txt", "next.txt"], None);
    h.rename();
    h.text("after");
    let (x, y) = h.first_icon();
    let step = h.read(|this, _| this.cell_width + layout::LAYOUT.grid.column_gap);
    h.click(x + step, y, MouseButton::Right);
    h.wait("deferred other-file menu", |this, _| {
        this.files.editor.is_none() && this.files.menu.is_some()
    });
    assert!(h.file("after.txt").exists());
    assert!(!h.file("before.txt").exists());
    assert_eq!(
        h.read(|this, _| this
            .selection
            .focus
            .and_then(|id| this.cache.get(&id))
            .map(|i| i.name.clone())),
        Some("next.txt".into())
    );
    let popup = h.popup();
    h.key_in(popup, "escape");
    assert!(h.grid_focused());
}

#[test]
fn closing_owner_removes_independent_popup_without_reading_owner_entity() {
    let mut h = Harness::new(&["note.txt"], None);
    let (x, y) = h.first_icon();
    h.click(x, y, MouseButton::Right);
    h.popup();
    h.cx.update_window(h.window.into(), |_, window, _| window.remove_window())
        .unwrap();
    h.frame();
    assert!(
        h.cx.update(|cx| cx.windows().is_empty()),
        "popup survived its owner"
    );
}

#[test]
fn marquee_edge_scroll_release_commits_contiguous_selection_and_stops() {
    let mut h = Harness::new(&[], Some(100_000));
    let left = h.read(|this, _| layout::content_left(this.sidebar_width()) - 4.);
    let start = point(px(left), px(layout::content_top() + 8.));
    h.event(
        h.window.into(),
        MouseDownEvent {
            position: start,
            button: MouseButton::Left,
            click_count: 1,
            ..Default::default()
        }
        .to_platform_input(),
    );
    let end = point(px(1092.), px(layout::content_bottom(720.)));
    h.event(
        h.window.into(),
        MouseMoveEvent {
            position: end,
            pressed_button: Some(MouseButton::Left),
            ..Default::default()
        }
        .to_platform_input(),
    );
    h.wait("marquee auto-scroll", |this, _| {
        -f32::from(this.scroll.0.borrow().base_handle.offset().y) > 700.
    });
    h.event(
        h.window.into(),
        MouseUpEvent {
            position: end,
            button: MouseButton::Left,
            click_count: 1,
            ..Default::default()
        }
        .to_platform_input(),
    );
    h.wait("selection worker commit", |this, _| this.marquee.is_none());
    let (count, columns, offset) = h.read(|this, _| {
        (
            this.selection.count(),
            this.columns,
            this.scroll.0.borrow().base_handle.offset().y,
        )
    });
    assert!(count > columns * 5);
    assert_eq!(count % columns, 0);
    assert!(
        h.read(|this, _| (0..count).all(|id| this.selection.contains(id))
            && !this.selection.contains(count))
    );
    for _ in 0..8 {
        h.frame();
    }
    assert_eq!(
        h.read(|this, _| this.scroll.0.borrow().base_handle.offset().y),
        offset
    );
    assert!(h.read(|this, _| this.cache.len() <= 256 && this.metrics.borrow().max_clocks <= 1));
}

#[test]
fn additive_marquee_escape_restores_original_selection() {
    let mut h = Harness::new(&[], Some(100_000));
    let (x, y) = h.first_icon();
    h.click(x, y, MouseButton::Left);
    let left = h.read(|this, _| layout::content_left(this.sidebar_width()) - 4.);
    let modifiers = Modifiers {
        control: true,
        ..Default::default()
    };
    h.event(
        h.window.into(),
        MouseDownEvent {
            position: point(px(left), px(layout::content_top() + 8.)),
            button: MouseButton::Left,
            click_count: 1,
            modifiers,
            ..Default::default()
        }
        .to_platform_input(),
    );
    h.event(
        h.window.into(),
        MouseMoveEvent {
            position: point(px(1092.), px(420.)),
            pressed_button: Some(MouseButton::Left),
            modifiers,
        }
        .to_platform_input(),
    );
    h.wait("additive preview", |this, _| {
        this.marquee
            .as_ref()
            .is_some_and(|m| m.active && m.accepted == m.token)
    });
    assert!(h.read(|this, _| this.selection.count() > 1));
    h.key("escape");
    assert!(h.read(|this, _| this.marquee.is_none()
        && this.selection.count() == 1
        && this.selection.contains(0)));
    assert!(h.grid_focused());
}

#[test]
fn file_properties_open_separately_keep_geometry_and_preserve_full_long_path() {
    let name = format!("完整文件名-{}.txt", "long-name-".repeat(15));
    let mut h = Harness::new(&[&name], None);
    let geometry = h.read(|this, _| (this.columns, this.cell_width));
    h.key("home");
    h.key("alt-enter");
    let properties = h.properties();
    h.properties_ready(properties);
    let path = properties
        .read_with(&h.cx, |view, _| view.displayed_path())
        .unwrap();
    assert_eq!(path, navigation::display_path(&h.file(&name)));
    let bytes = properties
        .read_with(&h.cx, |view, _| view.snapshot.bytes)
        .unwrap();
    assert_eq!(bytes, format!("contents of {name}").len() as u128);
    assert_eq!(h.read(|this, _| (this.columns, this.cell_width)), geometry);
    assert_eq!(h.cx.update(|cx| cx.windows().len()), 2);
    h.key_in(properties.into(), "ctrl-w");
    assert_eq!(h.cx.update(|cx| cx.windows().len()), 1);
}

#[test]
fn folder_properties_count_nested_files_without_resizing_or_duplicating_windows() {
    let mut h = Harness::new(&["original.txt"], None);
    let folder = h.file("资料");
    std::fs::create_dir_all(folder.join("nested/empty")).unwrap();
    std::fs::write(folder.join("a"), b"abc").unwrap();
    std::fs::write(folder.join("nested/b"), b"12345").unwrap();
    h.invalidate_directory();
    h.wait("new folder snapshot", |this, _| {
        this.source.count() == 2 && !this.watch_stale()
    });
    h.view.update(&mut h.cx, |this, cx| {
        let id = this.cache.values().find(|item| item.directory).unwrap().id;
        this.selection.select(id, false, false);
        cx.notify();
    });
    let geometry = h.read(|this, _| (this.columns, this.cell_width));
    h.key("alt-enter");
    let properties = h.properties();
    h.properties_ready(properties);
    let snapshot = properties
        .read_with(&h.cx, |view, _| view.snapshot.clone())
        .unwrap();
    assert_eq!(
        (snapshot.bytes, snapshot.files, snapshot.folders),
        (8, 2, 2)
    );
    assert!(snapshot.error.is_none());
    h.cx.update_window(h.window.into(), |_, window, cx| {
        window.activate_window();
        let focus = h.view.read(cx).focus.clone();
        window.focus(&focus, cx);
    })
    .unwrap();
    h.key("alt-enter");
    h.wait("repeat details request", |this, _| !this.detail_pending);
    assert_eq!(h.cx.update(|cx| cx.windows().len()), 2);
    assert_eq!(h.read(|this, _| (this.columns, this.cell_width)), geometry);
    h.key_in(properties.into(), "escape");
}

#[test]
fn properties_refresh_rechecks_identity_and_close_cancels_the_reader() {
    let mut h = Harness::new(&["item.txt"], None);
    h.key("home");
    h.key("alt-enter");
    let properties = h.properties();
    h.properties_ready(properties);
    std::fs::write(h.file("item.txt"), b"updated").unwrap();
    h.key_in(properties.into(), "f5");
    h.properties_ready(properties);
    assert_eq!(
        properties
            .read_with(&h.cx, |view, _| view.snapshot.bytes)
            .unwrap(),
        7
    );
    std::fs::rename(h.file("item.txt"), h.file("old.txt")).unwrap();
    std::fs::write(h.file("item.txt"), b"replacement").unwrap();
    h.key_in(properties.into(), "f5");
    h.properties_ready(properties);
    assert!(
        properties
            .read_with(&h.cx, |view, _| view
                .snapshot
                .error
                .as_deref()
                .is_some_and(|e| e.contains("改变")))
            .unwrap()
    );
    let canceled = properties
        .read_with(&h.cx, |view, _| view.cancellation_probe())
        .unwrap();
    assert!(!canceled());
    h.key_in(properties.into(), "escape");
    assert!(canceled());
    assert_eq!(h.cx.update(|cx| cx.windows().len()), 1);
}

#[test]
fn properties_scrollbar_tracks_short_window_overflow_and_hides_when_it_fits() {
    let mut h = Harness::new(&["item.txt"], None);
    h.key("home");
    h.key("alt-enter");
    let properties = h.properties();
    h.properties_ready(properties);
    h.cx.update_window(properties.into(), |_, window, cx| {
        window.resize(size(px(340.), px(360.)));
        window.bounds_changed(cx);
        window.refresh();
    })
    .unwrap();
    h.frame();
    h.frame();
    let geometry = properties
        .read_with(&h.cx, |view, _| view.scroll_geometry())
        .unwrap()
        .expect("overflow scrollbar");
    assert!(geometry.maximum > 0. && geometry.thumb < geometry.viewport as f32);
    h.event(
        properties.into(),
        MouseDownEvent {
            position: point(px(333.), px(340.)),
            button: MouseButton::Left,
            click_count: 1,
            ..Default::default()
        }
        .to_platform_input(),
    );
    h.event(
        properties.into(),
        MouseUpEvent {
            position: point(px(333.), px(340.)),
            button: MouseButton::Left,
            click_count: 1,
            ..Default::default()
        }
        .to_platform_input(),
    );
    assert!(
        properties
            .read_with(&h.cx, |view, _| view.scroll_geometry().unwrap().offset > 0.)
            .unwrap()
    );
    h.cx.update_window(properties.into(), |_, window, cx| {
        window.resize(size(px(600.), px(900.)));
        window.bounds_changed(cx);
        window.refresh();
    })
    .unwrap();
    h.frame();
    h.frame();
    assert!(
        properties
            .read_with(&h.cx, |view, _| view.scroll_geometry().is_none())
            .unwrap()
    );
    h.key_in(properties.into(), "escape");
}

// ---- administrator handoff: prompt, one-shot request, elevated window ----

impl Harness {
    fn open_second_window(&mut self, options: Options) -> WindowHandle<Backleaf> {
        let owner: AnyWindowHandle = self.window.into();
        self.cx.update(|cx| open_backleaf_window(options, cx));
        self.frame();
        self.cx
            .update(|cx| cx.windows())
            .into_iter()
            .filter(|handle| *handle != owner)
            .find_map(|handle| handle.downcast::<Backleaf>())
            .expect("second Backleaf window")
    }
    fn read_second<R>(
        &self,
        handle: WindowHandle<Backleaf>,
        read: impl FnOnce(&Backleaf, &App) -> R,
    ) -> R {
        handle
            .read_with(&self.cx, read)
            .expect("second Backleaf window alive")
    }
    fn wait_second(
        &mut self,
        handle: WindowHandle<Backleaf>,
        what: &str,
        condition: impl Fn(&Backleaf, &App) -> bool,
    ) {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            self.frame();
            if self.read_second(handle, |view, app| condition(view, app)) {
                return;
            }
            self.cx.advance_clock(Duration::from_millis(10));
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(3));
        }
    }
    /// Deliver the worker event the directory snapshot produces for
    /// ERROR_ACCESS_DENIED without weakening any real ACL.
    fn denied_directory_load(&mut self, error: &str) {
        let generation = self.read(|this, _| this.generation);
        self.view.update(&mut self.cx, |this, cx| {
            this.receive(
                Event::Count {
                    generation,
                    count: 0,
                    done: true,
                    error: Some(error.to_owned()),
                    denied: true,
                },
                cx,
            )
        });
    }
}

fn windows_command_line_arguments(arguments: &str) -> Vec<String> {
    use windows::Win32::{
        Foundation::{HLOCAL, LocalFree},
        UI::Shell::CommandLineToArgvW,
    };
    use windows::core::PCWSTR;
    let command_line: Vec<u16> = format!("backleaf.exe {arguments}")
        .encode_utf16()
        .chain(Some(0))
        .collect();
    unsafe {
        let mut count = 0i32;
        let argv = CommandLineToArgvW(PCWSTR(command_line.as_ptr()), &mut count);
        assert!(!argv.is_null(), "CommandLineToArgvW failed");
        let mut parsed = Vec::with_capacity(count as usize);
        for index in 0..count {
            let argument = *argv.add(index as usize);
            parsed.push(argument.to_string().expect("argument is utf-16"));
        }
        let _ = LocalFree(Some(HLOCAL(argv.cast())));
        parsed
    }
}

#[test]
fn headless_elevation_prompt_is_narrowed_to_access_denied_and_cancel_keeps_window() {
    crate::i18n::set(crate::i18n::Language::Zh);
    let mut h = Harness::new(&["alpha.txt", "beta.txt"], None);
    let (path, count, cache_len) =
        h.read(|this, _| (this.path.clone(), this.source.count(), this.cache.len()));
    let elevation_dir = h.fixture.0.join("state").join("elevation");
    // A sharing violation or any other I/O failure is not a privilege denial,
    // even when its localized text mentions access.
    h.view.update(&mut h.cx, |this, cx| {
        this.receive(
            Event::Count {
                generation: this.generation,
                count: 0,
                done: true,
                error: Some("拒绝访问：另一个程序正在使用此文件".into()),
                denied: false,
            },
            cx,
        )
    });
    h.read(|this, _| {
        assert!(this.elevation.prompt.is_none());
        assert_eq!(
            this.error.as_deref(),
            Some("拒绝访问：另一个程序正在使用此文件")
        );
        assert_eq!(this.path, path);
        assert_eq!(this.source.count(), count);
    });
    assert!(!elevation_dir.exists());
    // ERROR_ACCESS_DENIED raises the modal and keeps the old list untouched.
    h.denied_directory_load("拒绝访问");
    h.frame();
    h.read(|this, _| {
        let prompt = this
            .elevation
            .prompt
            .as_ref()
            .expect("administrator prompt");
        assert_eq!(prompt.path, path);
        assert_eq!(this.error.as_deref(), Some("此位置需要管理员权限"));
        assert_eq!(this.source.count(), count);
        assert_eq!(this.cache.len(), cache_len);
        assert!(!this.elevation.pending);
    });
    let painted =
        h.cx.update_window(h.window.into(), |_, window, _| {
            let scale = window.scale_factor();
            window.painted_quads().iter().any(|quad| {
                (quad.bounds.size.width.0 / scale - 480.).abs() < 1.
                    && quad.bounds.size.height.0 / scale > 100.
            })
        })
        .unwrap();
    assert!(painted, "administrator modal surface must be painted");
    // Escape dismisses the prompt, keeps the status reason and writes nothing.
    h.key("escape");
    h.read(|this, _| {
        assert!(this.elevation.prompt.is_none());
        assert_eq!(this.error.as_deref(), Some("此位置需要管理员权限"));
        assert_eq!(this.path, path);
        assert_eq!(this.source.count(), count);
    });
    assert!(!elevation_dir.exists());
    // English copy rendered by this path is exact, not a silent fallback.
    crate::i18n::set(crate::i18n::Language::En);
    h.denied_directory_load("Access is denied.");
    h.read(|this, _| {
        assert!(
            this.elevation.prompt.is_some(),
            "the English prompt is modal as well"
        );
        assert_eq!(
            this.error.as_deref(),
            Some("This location requires administrator permission")
        );
    });
    assert_eq!(crate::i18n::tr("取消"), "Cancel");
    assert_eq!(
        crate::i18n::tr("需要管理员权限"),
        "Administrator permission required"
    );
    assert_eq!(
        crate::i18n::tr("以管理员身份重新打开"),
        "Reopen as administrator"
    );
    assert_eq!(
        crate::i18n::tr(
            "此位置需要管理员权限才能访问。Windows 会请求你的同意，并在新窗口中以管理员身份打开同一位置。"
        ),
        "This location requires administrator permission. Windows will ask for your consent and reopen the same location in an elevated window."
    );
    // A cancelled or failed UAC launch keeps the prompt for a retry. The cancel
    // reason is shown through the transient navigation toast, while the status
    // line keeps the permission reason.
    h.view.update(&mut h.cx, |this, cx| {
        this.accept_elevation_launch(Err(crate::i18n::tr_owned("已取消管理员权限请求")), cx)
    });
    h.read(|this, _| {
        assert!(this.elevation.prompt.is_some());
        assert!(!this.elevation.pending);
        assert_eq!(
            this.error.as_deref(),
            Some("This location requires administrator permission")
        );
    });
    h.key("escape");
    h.read(|this, _| {
        assert!(this.elevation.prompt.is_none());
        assert_eq!(
            this.error.as_deref(),
            Some("This location requires administrator permission")
        );
    });
    // An elevated window that is refused again must show the real error, not
    // leave the list blank behind a prompt that cannot acquire more access.
    h.view.update(&mut h.cx, |this, cx| {
        this.elevation.elevated = true;
        this.elevation.banner = true;
        cx.notify();
    });
    h.frame();
    h.denied_directory_load("拒绝访问（管理员令牌仍被拒绝）");
    h.read(|this, _| {
        assert!(this.elevation.prompt.is_none());
        assert_eq!(
            this.error.as_deref(),
            Some("拒绝访问（管理员令牌仍被拒绝）")
        );
    });
    h.view
        .update(&mut h.cx, |this, cx| this.dismiss_elevation_banner(cx));
    h.read(|this, _| assert!(!this.elevation.banner));
    crate::i18n::set(crate::i18n::Language::Zh);
    assert!(
        !elevation_dir.exists(),
        "no handoff may be written without explicit confirmation"
    );
}

#[test]
fn headless_elevation_confirm_writes_lossless_one_shot_handoff_without_uac() {
    crate::i18n::set(crate::i18n::Language::Zh);
    let mut h = Harness::new(&["first.txt", "second.txt"], None);
    let path = h.read(|this, _| this.path.clone());
    let selected = h.read(|this, _| {
        let item = this.cache.values().max_by_key(|item| item.id).unwrap();
        (item.id, item.name.clone())
    });
    h.view.update(&mut h.cx, |this, cx| {
        this.selection.select(selected.0, false, false);
        this.keyboard_focus = true;
        cx.notify();
    });
    let selection_before = h.read(|this, _| this.selection.count());
    let (window_width, window_height) =
        h.cx.update_window(h.window.into(), |_, window, _| {
            let bounds = window.bounds();
            (f32::from(bounds.size.width), f32::from(bounds.size.height))
        })
        .unwrap();
    h.denied_directory_load("拒绝访问");
    h.key("enter");
    h.wait("elevation confirmation", |this, _| {
        this.elevation.prompt.is_none()
            && !this.elevation.pending
            && this.error.as_deref() == Some("已打开管理员窗口")
    });
    let directory = h.fixture.0.join("state").join("elevation");
    let requests = std::fs::read_dir(&directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect::<Vec<_>>();
    assert_eq!(requests.len(), 1, "exactly one one-shot handoff request");
    let request = crate::elevation::read_request(&requests[0]).expect("valid handoff request");
    assert_eq!(request.version, crate::elevation::REQUEST_VERSION);
    assert_eq!(request.state.version, 1);
    assert_eq!(
        request.state.path,
        crate::file_view::WinPath::new(&path),
        "the handoff keeps the exact UTF-16 path spelling"
    );
    assert_eq!(request.state.search, None);
    assert_eq!(request.selection.len(), 1);
    assert_eq!(
        request.selection[0].path(),
        path.canonicalize().unwrap().join(&selected.1),
        "captured selection keeps the snapshot's canonical parent and exact name"
    );
    assert!((request.state.bounds[2] - window_width).abs() < 1.);
    assert!((request.state.bounds[3] - window_height).abs() < 1.);
    // The original normal window is unchanged and keeps its rows and selection.
    h.read(|this, _| {
        assert_eq!(this.path, path);
        assert_eq!(this.selection.count(), selection_before);
        assert!(this.elevation.prompt.is_none());
        assert_eq!(this.error.as_deref(), Some("已打开管理员窗口"));
    });
    // cfg(test) never calls ShellExecuteExW("runas"): the request file is still
    // there for this test to read instead of being consumed by an elevated process.
    assert!(crate::elevation::launch_elevated(&requests[0], &h.fixture.0.join("state")).is_ok());
    assert!(
        requests[0].is_file(),
        "test builds must not consume the request through a real UAC launch"
    );
    std::fs::remove_file(&requests[0]).unwrap();
}

#[test]
fn headless_elevated_window_restores_handoff_path_geometry_and_selection() {
    crate::i18n::set(crate::i18n::Language::Zh);
    let names = (0..120)
        .map(|index| format!("file_{index:03}.txt"))
        .collect::<Vec<_>>();
    let borrowed = names.iter().map(String::as_str).collect::<Vec<_>>();
    let mut h = Harness::new(&borrowed, None);
    let (path, count) = h.read(|this, _| (this.path.clone(), this.source.count()));
    assert_eq!(count, 120);
    let selected = h.read(|this, _| {
        this.cache
            .values()
            .max_by_key(|item| item.id)
            .map(|item| item.name.clone())
            .unwrap()
    });
    let mut definition = crate::file_view::FileView::new(&path);
    definition.query = "file_0".into();
    let state = crate::window_state::State {
        version: 1,
        path: crate::file_view::WinPath::new(&path),
        search: Some(crate::window_state::SearchState {
            definition,
            saved: None,
        }),
        bounds: [40., 50., 880., 620.],
        maximized: false,
    };
    let request = crate::elevation::Request::new(
        state.clone(),
        vec![crate::file_view::WinPath::new(&path.join(&selected))],
        0,
    );
    let handoff_dir = h.fixture.0.join("elevated-state");
    let request_path = crate::elevation::write_request(&handoff_dir, &request).unwrap();
    let request = crate::elevation::read_request(&request_path).unwrap();
    std::fs::remove_file(&request_path).unwrap();
    // The elevated startup path in `main` turns the request into these options.
    let mut options = h.read(|this, _| this.options.clone());
    options.path = Some(request.state.path.path());
    options.width = request.state.bounds[2].max(560.);
    options.height = request.state.bounds[3].max(440.);
    options.state_dir = handoff_dir;
    options.restored = Some(crate::window_state::Restored {
        id: format!("elevation-verify-{}", std::process::id()),
        revision: 0,
        state: request.state.clone(),
    });
    options.elevation_restore = Some(request.restore());
    let elevated = h.open_second_window(options);
    h.wait_second(elevated, "elevated window enumeration", |view, _| {
        view.source.count() == 120 && view.source.finished() && !view.cache.is_empty()
    });
    h.wait_second(elevated, "restored search intent", |view, _| {
        view.search.open && view.search.query == "file_0"
    });
    h.wait_second(elevated, "restored selection and scroll", |view, _| {
        view.selection.count() == 1
            && view.selection.focus.is_some_and(|id| {
                view.cache
                    .get(&id)
                    .is_some_and(|item| item.name == selected)
            })
            && -f32::from(view.scroll.0.borrow().base_handle.offset().y) > 0.
    });
    h.read_second(elevated, |view, _app| {
        assert_eq!(view.path, path);
        assert!(view.search.open, "handoff search definition reopens");
        assert_eq!(view.search.query, "file_0");
        assert!(view.restore.restored);
        let handoff_state = view.restore.last.as_ref().expect("handoff state");
        assert_eq!(handoff_state.path, state.path);
        assert_eq!(handoff_state.bounds, [40., 50., 880., 620.]);
        assert!(!handoff_state.maximized);
    });
    let bounds =
        h.cx.update_window(elevated.into(), |_, window, _| {
            let bounds = window.bounds();
            (
                f32::from(bounds.origin.x),
                f32::from(bounds.origin.y),
                f32::from(bounds.size.width),
                f32::from(bounds.size.height),
            )
        })
        .unwrap();
    assert!(
        (bounds.0 - 40.).abs() < 0.01
            && (bounds.1 - 50.).abs() < 0.01
            && (bounds.2 - 880.).abs() < 0.01
            && (bounds.3 - 620.).abs() < 0.01,
        "elevated window restores the handoff geometry, got {bounds:?}"
    );
    // The normal window that asked for elevation is untouched.
    h.read(|this, _| {
        assert_eq!(this.path, path);
        assert_eq!(this.source.count(), 120);
        assert!(this.elevation.restore.is_none());
        assert!(this.elevation.prompt.is_none());
    });
}

#[test]
fn headless_elevation_handoff_validation_and_windows_argument_quoting() {
    use crate::{elevation, file_view::WinPath, window_state::State};
    use std::os::windows::ffi::OsStrExt;

    crate::i18n::set(crate::i18n::Language::Zh);
    let fixture = test_support::Fixture::new("elevation-handoff");
    // Unpaired UTF-16 surrogates are valid NTFS path units; the handoff must not
    // replace them with U+FFFD on the way through JSON.
    let units = [0x4E2D_u16, 0x6587, 0xD800, 0x0061, 0x0042, 0x5C_u16];
    let state = State {
        version: 1,
        path: WinPath(units.to_vec()),
        search: None,
        bounds: [10., 20., 900., 600.],
        maximized: false,
    };
    let request = elevation::Request::new(state, Vec::new(), 3);
    request
        .validate()
        .expect("lossless surrogate path is a valid handoff path");
    let path = elevation::write_request(&fixture.0, &request).unwrap();
    let decoded = elevation::read_request(&path).unwrap();
    assert_eq!(
        decoded.state.path.0, units,
        "JSON keeps every UTF-16 code unit"
    );
    assert_eq!(
        decoded
            .state
            .path
            .path()
            .as_os_str()
            .encode_wide()
            .collect::<Vec<_>>(),
        units
    );
    assert_eq!(decoded.scroll_row, 3);
    // Malformed, unreadable and out-of-schema requests are rejected.
    let malformed = fixture.0.join("malformed.json");
    std::fs::write(&malformed, b"{not a request").unwrap();
    assert_eq!(
        elevation::read_request(&malformed).unwrap_err(),
        "无法读取管理员请求"
    );
    assert_eq!(
        elevation::read_request(&fixture.0.join("missing.json")).unwrap_err(),
        "无法读取管理员请求"
    );
    let mut bad = decoded.clone();
    bad.state.bounds = [0., 0., f32::NAN, 1.];
    assert_eq!(bad.validate().unwrap_err(), "无法读取管理员请求");
    bad.state.bounds = [0., 0., 1., 1.];
    bad.state.path = WinPath(Vec::new());
    assert!(bad.validate().is_err());
    bad.state.path = WinPath(vec![0x43, 0]);
    assert!(bad.validate().is_err());
    bad.state.path = WinPath(vec![0x43]);
    bad.version = elevation::REQUEST_VERSION + 1;
    assert_eq!(bad.validate().unwrap_err(), "管理员请求版本不兼容");
    bad.version = elevation::REQUEST_VERSION;
    bad.state.version = 2;
    assert_eq!(bad.validate().unwrap_err(), "管理员请求版本不兼容");
    bad.state.version = 1;
    bad.selection =
        vec![WinPath(vec![0x41, 0x2E, 0x74, 0x78, 0x74]); elevation::MAX_RESTORED_SELECTION + 1];
    assert!(bad.validate().is_err());
    bad.selection = vec![WinPath(vec![0x41, 0])];
    assert!(bad.validate().is_err());
    // Quoting is verified with the real Windows command-line parser, the same
    // one ShellExecuteEx applies before the elevated process sees its argv.
    let state_dir = fixture.0.join("state dir with spaces");
    let request_path = state_dir.join("request-1.json");
    let arguments = elevation::request_arguments(&request_path, &state_dir);
    assert_eq!(
        windows_command_line_arguments(&arguments),
        vec![
            "backleaf.exe".to_owned(),
            "--elevate-request".to_owned(),
            request_path.to_string_lossy().into_owned(),
            "--state-dir".to_owned(),
            state_dir.to_string_lossy().into_owned(),
        ]
    );
    let trailing = elevation::request_arguments(
        std::path::Path::new(r"C:\state dir\request.json"),
        std::path::Path::new(r"C:\state dir\"),
    );
    let parsed = windows_command_line_arguments(&trailing);
    assert_eq!(parsed[2], r"C:\state dir\request.json");
    assert_eq!(
        parsed[4], r"C:\state dir\",
        "a state directory ending in a backslash survives quoting"
    );
    // Test builds must return before ShellExecuteExW and keep the request.
    assert!(elevation::launch_elevated(&path, &fixture.0).is_ok());
    assert!(
        path.is_file(),
        "cfg(test) launch must not consume the request"
    );
    // Sharing violations, privilege errors and plain error text are classified
    // as narrowly as the elevated retry requires.
    assert!(!elevation::io_access_denied(
        &std::io::Error::from_raw_os_error(32)
    ));
    assert!(elevation::io_access_denied(
        &std::io::Error::from_raw_os_error(5)
    ));
    assert!(!elevation::hresult_access_denied(0x8007_0020u32 as i32));
    assert!(!elevation::hresult_access_denied(0x8007_0033u32 as i32));
    assert!(!elevation::error_access_denied(&anyhow::Error::new(
        std::io::Error::from_raw_os_error(32)
    )));
    assert!(elevation::error_access_denied(&anyhow::Error::new(
        std::io::Error::from_raw_os_error(5)
    )));
    std::fs::remove_file(&path).unwrap();
}
