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
        let deadline = Instant::now() + Duration::from_secs(10);
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
