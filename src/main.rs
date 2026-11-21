//! Snip Shelf — a history of everything you copy, snip or record on your Mac.
//! Click any card and it goes back on the clipboard, ready for Command+V.

mod icon;
mod mac;
mod model;
mod watch;

use eframe::egui::{
    self, Align, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Frame, Layout, Margin,
    RichText, Sense, Stroke, TextureHandle, TextureOptions, UiBuilder, Vec2,
};
use model::{Item, Kind, Settings};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use watch::Incoming;

const BG: Color32 = Color32::from_rgb(0x12, 0x14, 0x19);
const PANEL: Color32 = Color32::from_rgb(0x17, 0x1A, 0x21);
const CARD: Color32 = Color32::from_rgb(0x1F, 0x23, 0x2C);
const CARD_HOVER: Color32 = Color32::from_rgb(0x27, 0x2C, 0x37);
const BORDER: Color32 = Color32::from_rgb(0x2C, 0x31, 0x3D);
const TEXT: Color32 = Color32::from_rgb(0xE8, 0xEA, 0xF0);
const MUTED: Color32 = Color32::from_rgb(0x8B, 0x92, 0xA3);
const ACCENT: Color32 = Color32::from_rgb(0x7C, 0x6C, 0xFF);
const C_TEXT: Color32 = Color32::from_rgb(0x4C, 0x9A, 0xFF);
const C_IMAGE: Color32 = Color32::from_rgb(0x3D, 0xD6, 0x8C);
const C_VIDEO: Color32 = Color32::from_rgb(0xFF, 0x6B, 0x5A);
const C_FILE: Color32 = Color32::from_rgb(0xF5, 0xB7, 0x41);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Filter {
    All,
    Text,
    Pictures,
    Videos,
    Pinned,
}

struct Shot {
    path: PathBuf,
    frames: u32,
    requested: bool,
    done: Arc<AtomicBool>,
}

struct App {
    items: Vec<Item>,
    settings: Settings,
    tx: Sender<Incoming>,
    rx: Receiver<Incoming>,
    ignore: Arc<AtomicIsize>,
    auto_copy: Arc<AtomicBool>,
    thumbs: HashMap<u64, TextureHandle>,
    thumb_pending: HashSet<u64>,
    thumb_failed: HashSet<u64>,
    filter: Filter,
    query: String,
    selected: usize,
    toast: Option<(String, f64)>,
    flash: Option<(u64, f64)>,
    hide_at: Option<f64>,
    dirty: bool,
    shot: Option<Shot>,
    sample_mode: bool,
    hotkey_ok: bool,
    #[cfg(target_os = "macos")]
    _hotkeys: Option<global_hotkey::GlobalHotKeyManager>,
    #[cfg(target_os = "macos")]
    _tray: Option<tray_icon::TrayIcon>,
}

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let candidates = [
        ("arial", "/System/Library/Fonts/Supplemental/Arial.ttf", true),
        ("arial_unicode", "/System/Library/Fonts/Supplemental/Arial Unicode.ttf", false),
        ("apple_symbols", "/System/Library/Fonts/Apple Symbols.ttf", false),
    ];
    for (name, path, primary) in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert(name.to_owned(), Arc::new(FontData::from_owned(bytes)));
            let fam = fonts.families.entry(FontFamily::Proportional).or_default();
            if primary {
                fam.insert(0, name.to_owned());
            } else {
                fam.push(name.to_owned());
            }
            if !primary {
                fonts.families.entry(FontFamily::Monospace).or_default().push(name.to_owned());
            }
        }
    }
    ctx.set_fonts(fonts);
}

fn setup_style(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BG;
    visuals.window_fill = PANEL;
    visuals.extreme_bg_color = Color32::from_rgb(0x0E, 0x10, 0x14);
    visuals.override_text_color = Some(TEXT);
    visuals.selection.bg_fill = ACCENT;
    visuals.widgets.inactive.bg_fill = CARD;
    visuals.widgets.inactive.weak_bg_fill = CARD;
    visuals.widgets.hovered.weak_bg_fill = CARD_HOVER;
    visuals.widgets.hovered.bg_fill = CARD_HOVER;
    visuals.widgets.inactive.corner_radius = CornerRadius::same(8);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(8);
    visuals.widgets.active.corner_radius = CornerRadius::same(8);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    ctx.set_theme(egui::Theme::Dark);
    ctx.set_visuals_of(egui::Theme::Dark, visuals);
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = Vec2::new(8.0, 8.0);
        s.spacing.button_padding = Vec2::new(12.0, 6.0);
    });
}

/// A made-up picture for the sample cards in the check screenshot.
fn sample_picture(video: bool) -> egui::ColorImage {
    let (w, h) = (520usize, 300usize);
    let mut px = vec![Color32::BLACK; w * h];
    for y in 0..h {
        for x in 0..w {
            let fx = x as f32 / w as f32;
            let fy = y as f32 / h as f32;
            let c = if video {
                Color32::from_rgb((30.0 + 60.0 * fx) as u8, (24.0 + 30.0 * fy) as u8, (60.0 + 90.0 * (1.0 - fx)) as u8)
            } else {
                Color32::from_rgb((70.0 + 90.0 * fy) as u8, (120.0 + 80.0 * fx) as u8, (200.0 - 40.0 * fy) as u8)
            };
            px[y * w + x] = c;
        }
    }
    // A pretend window with a title bar and some lines of content.
    let rect = |px: &mut Vec<Color32>, x0: usize, y0: usize, x1: usize, y1: usize, c: Color32| {
        for y in y0..y1.min(h) {
            for x in x0..x1.min(w) {
                px[y * w + x] = c;
            }
        }
    };
    rect(&mut px, 60, 40, 460, 260, Color32::from_rgb(0xF4, 0xF5, 0xF8));
    rect(&mut px, 60, 40, 460, 66, Color32::from_rgb(0xDD, 0xDF, 0xE6));
    for (i, c) in [(0, Color32::from_rgb(255, 95, 87)), (1, Color32::from_rgb(254, 188, 46)), (2, Color32::from_rgb(40, 200, 64))] {
        rect(&mut px, 72 + i * 16, 48, 82 + i * 16, 58, c);
    }
    for i in 0..6 {
        let len = [300, 340, 220, 320, 260, 180][i];
        rect(&mut px, 84, 86 + i * 26, 84 + len, 96 + i * 26, Color32::from_rgb(0xC5, 0xC9, 0xD3));
    }
    rect(&mut px, 330, 200, 440, 240, if video { C_VIDEO } else { ACCENT });
    egui::ColorImage::new([w, h], px)
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, shot: Option<PathBuf>) -> Self {
        setup_fonts(&cc.egui_ctx);
        setup_style(&cc.egui_ctx);
        let ctx = cc.egui_ctx.clone();
        let (tx, rx) = channel();
        let sample_mode = shot.is_some();
        let settings = if sample_mode { Settings::default() } else { model::load_settings() };
        let ignore = Arc::new(AtomicIsize::new(-1));
        let auto_copy = Arc::new(AtomicBool::new(settings.auto_copy_new_captures));
        let items = if sample_mode { model::sample_items() } else { model::load_history() };

        let mut app = App {
            items,
            settings,
            tx: tx.clone(),
            rx,
            ignore: ignore.clone(),
            auto_copy: auto_copy.clone(),
            thumbs: HashMap::new(),
            thumb_pending: HashSet::new(),
            thumb_failed: HashSet::new(),
            filter: Filter::All,
            query: String::new(),
            selected: 0,
            toast: None,
            flash: None,
            hide_at: None,
            dirty: false,
            shot: shot.map(|path| Shot { path, frames: 0, requested: false, done: Arc::new(AtomicBool::new(false)) }),
            sample_mode,
            hotkey_ok: false,
            #[cfg(target_os = "macos")]
            _hotkeys: None,
            #[cfg(target_os = "macos")]
            _tray: None,
        };

        if sample_mode {
            for it in app.items.clone() {
                if it.kind == Kind::Image || it.kind == Kind::Video {
                    let img = sample_picture(it.kind == Kind::Video);
                    let tex = ctx.load_texture(format!("thumb-{}", it.id), img, TextureOptions::LINEAR);
                    app.thumbs.insert(it.id, tex);
                }
            }
            app.toast = Some(("Text copied — press ⌘V to paste".into(), f64::MAX));
        } else {
            watch::start_clipboard_watcher(tx.clone(), ctx.clone(), ignore);
            watch::start_folder_watcher(tx.clone(), ctx.clone(), auto_copy);
            app.register_hotkey(&ctx);
            app.setup_tray(&ctx);
        }
        app
    }

    /// The little Snip Shelf icon in the top-right corner of the screen (menu bar).
    #[cfg(target_os = "macos")]
    fn setup_tray(&mut self, ctx: &egui::Context) {
        use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
        use tray_icon::{Icon, TrayIconBuilder};
        let menu = Menu::new();
        let items = [
            MenuItem::with_id("open", "Open Snip Shelf", true, None),
            MenuItem::with_id("latest", "Copy latest item again", true, None),
            MenuItem::with_id("snip", "Snip area", true, None),
            MenuItem::with_id("record", "Record screen", true, None),
            MenuItem::with_id("folder", "Show saved pictures & videos", true, None),
            MenuItem::with_id("quit", "Quit Snip Shelf", true, None),
        ];
        let sep1 = PredefinedMenuItem::separator();
        let sep2 = PredefinedMenuItem::separator();
        let _ = menu.append_items(&[&items[0], &items[1], &sep1, &items[2], &items[3], &items[4], &sep2, &items[5]]);
        let size = 44;
        let Ok(icon) = Icon::from_rgba(icon::tray_icon(size), size, size) else { return };
        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_icon_templated(icon)
            .with_tooltip("Snip Shelf — your copy history")
            .build();
        if let Ok(tray) = tray {
            self._tray = Some(tray);
            let tx = self.tx.clone();
            let ctx2 = ctx.clone();
            MenuEvent::set_event_handler(Some(move |ev: MenuEvent| {
                if ev.id.0 == "open" {
                    mac::show_app();
                }
                let _ = tx.send(Incoming::Menu(ev.id.0.clone()));
                ctx2.request_repaint();
            }));
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn setup_tray(&mut self, _ctx: &egui::Context) {}

    /// Copy this app into the Applications folder in your home folder, so ⌘Space finds it.
    fn add_to_applications(&mut self, ctx: &egui::Context) {
        let msg = match current_bundle() {
            Some(bundle) => {
                let home = dirs::home_dir().unwrap_or_default();
                let apps = home.join("Applications");
                let dest = apps.join("Snip Shelf.app");
                if bundle == dest {
                    "Snip Shelf is already in your Applications folder".to_string()
                } else {
                    let _ = std::fs::create_dir_all(&apps);
                    let _ = std::fs::remove_dir_all(&dest);
                    let ok = std::process::Command::new("ditto")
                        .arg(&bundle)
                        .arg(&dest)
                        .status()
                        .map(|s| s.success())
                        .unwrap_or(false);
                    if ok {
                        let _ = std::process::Command::new("mdimport").arg(&dest).status();
                        "Added! Press ⌘Space and type Snip Shelf".to_string()
                    } else {
                        "Couldn't copy the app to Applications".to_string()
                    }
                }
            }
            None => "Open the Snip Shelf app (not the bare program) to add it".to_string(),
        };
        self.toast = Some((msg, Self::now(ctx)));
    }

    #[cfg(target_os = "macos")]
    fn register_hotkey(&mut self, ctx: &egui::Context) {
        use global_hotkey::hotkey::{Code, HotKey, Modifiers};
        use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
        if let Ok(manager) = GlobalHotKeyManager::new() {
            let hk = HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyV);
            if manager.register(hk).is_ok() {
                self.hotkey_ok = true;
                let ctx2 = ctx.clone();
                let tx = self.tx.clone();
                GlobalHotKeyEvent::set_event_handler(Some(move |ev: GlobalHotKeyEvent| {
                    if ev.state == HotKeyState::Pressed {
                        mac::show_app();
                        let _ = tx.send(Incoming::Toast("Click an item, then press ⌘V".into()));
                        ctx2.request_repaint();
                    }
                }));
            }
            self._hotkeys = Some(manager);
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn register_hotkey(&mut self, _ctx: &egui::Context) {}

    fn now(ctx: &egui::Context) -> f64 {
        ctx.input(|i| i.time)
    }

    fn move_to_top(&mut self, idx: usize) {
        let mut it = self.items.remove(idx);
        it.created = model::now_secs();
        self.items.insert(0, it);
        self.dirty = true;
    }

    fn add_item(&mut self, item: Item) {
        self.items.insert(0, item);
        self.selected = 0;
        self.trim();
        self.dirty = true;
    }

    fn trim(&mut self) {
        let max = self.settings.max_items.max(10);
        let captures = model::captures_dir();
        while self.items.iter().filter(|i| !i.pinned).count() > max {
            if let Some(pos) = self.items.iter().rposition(|i| !i.pinned) {
                let it = self.items.remove(pos);
                self.forget_files(&it, &captures);
            } else {
                break;
            }
        }
    }

    fn forget_files(&mut self, it: &Item, captures: &std::path::Path) {
        self.thumbs.remove(&it.id);
        if let Some(p) = &it.path {
            if model::is_inside(p, captures) && it.kind == Kind::Image {
                let _ = std::fs::remove_file(p);
            }
        }
    }

    fn delete(&mut self, id: u64) {
        if let Some(pos) = self.items.iter().position(|i| i.id == id) {
            let it = self.items.remove(pos);
            let captures = model::captures_dir();
            self.forget_files(&it, &captures);
            self.dirty = true;
        }
    }

    fn clear_unpinned(&mut self) {
        let captures = model::captures_dir();
        let gone: Vec<Item> = self.items.iter().filter(|i| !i.pinned).cloned().collect();
        self.items.retain(|i| i.pinned);
        for it in gone {
            self.forget_files(&it, &captures);
        }
        self.selected = 0;
        self.dirty = true;
    }

    fn copy(&mut self, ctx: &egui::Context, id: u64) {
        let Some(item) = self.items.iter().find(|i| i.id == id).cloned() else { return };
        if self.sample_mode {
            return;
        }
        watch::copy_item(item, self.tx.clone(), ctx.clone(), self.ignore.clone());
        let now = Self::now(ctx);
        self.flash = Some((id, now));
        if self.settings.hide_after_copy {
            self.hide_at = Some(now + 0.45);
        }
    }

    fn handle_incoming(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Incoming::Text(t) => {
                    if let Some(idx) = self.items.iter().position(|i| i.kind == Kind::Text && i.text.as_deref() == Some(&t)) {
                        self.move_to_top(idx);
                    } else {
                        self.add_item(Item {
                            id: model::new_id(),
                            kind: Kind::Text,
                            hash: watch::text_hash(&t),
                            text: Some(t),
                            path: None,
                            source: "Copied".into(),
                            created: model::now_secs(),
                            pinned: false,
                            width: 0,
                            height: 0,
                        });
                    }
                }
                Incoming::Image { path, width, height, hash, source } => {
                    if let Some(idx) = self.items.iter().position(|i| i.kind == Kind::Image && i.hash == hash && hash != 0) {
                        let _ = std::fs::remove_file(&path);
                        self.move_to_top(idx);
                    } else {
                        self.add_item(Item {
                            id: model::new_id(),
                            kind: Kind::Image,
                            text: None,
                            path: Some(path),
                            source,
                            created: model::now_secs(),
                            pinned: false,
                            width,
                            height,
                            hash,
                        });
                    }
                }
                Incoming::FileRef { path, kind, source, auto_copy } => {
                    let path = keep_copy(path, kind, &source);
                    let existing = self.items.iter().position(|i| i.path.as_ref() == Some(&path));
                    let id = if let Some(idx) = existing {
                        self.move_to_top(idx);
                        self.items[0].id
                    } else {
                        let id = model::new_id();
                        self.add_item(Item {
                            id,
                            kind,
                            text: None,
                            path: Some(path),
                            source: source.clone(),
                            created: model::now_secs(),
                            pinned: false,
                            width: 0,
                            height: 0,
                            hash: 0,
                        });
                        id
                    };
                    if auto_copy {
                        if let Some(item) = self.items.iter().find(|i| i.id == id).cloned() {
                            watch::copy_item(item, self.tx.clone(), ctx.clone(), self.ignore.clone());
                        }
                    } else {
                        self.toast = Some((format!("New {} saved to your history", source.to_lowercase()), Self::now(ctx)));
                    }
                }
                Incoming::Thumb { id, image } => {
                    let tex = ctx.load_texture(format!("thumb-{id}"), image, TextureOptions::LINEAR);
                    self.thumbs.insert(id, tex);
                }
                Incoming::ThumbFailed { id } => {
                    self.thumb_failed.insert(id);
                }
                Incoming::Toast(t) => {
                    self.toast = Some((t, Self::now(ctx)));
                }
                Incoming::Menu(id) => match id.as_str() {
                    "open" => {
                        mac::show_app();
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    }
                    "latest" => {
                        if let Some(item) = self.items.first().cloned() {
                            watch::copy_item(item, self.tx.clone(), ctx.clone(), self.ignore.clone());
                        }
                    }
                    "snip" => {
                        mac::hide_app();
                        watch::run_capture(false, self.tx.clone(), ctx.clone());
                    }
                    "record" => {
                        mac::hide_app();
                        watch::run_capture(true, self.tx.clone(), ctx.clone());
                    }
                    "folder" => {
                        let _ = std::process::Command::new("open").arg(model::captures_dir()).spawn();
                    }
                    "quit" => {
                        model::save_history(&self.items);
                        std::process::exit(0);
                    }
                    _ => {}
                },
            }
        }
    }

    fn visible(&self) -> Vec<usize> {
        self.items
            .iter()
            .enumerate()
            .filter(|(_, i)| match self.filter {
                Filter::All => true,
                Filter::Text => i.kind == Kind::Text,
                Filter::Pictures => i.kind == Kind::Image,
                Filter::Videos => i.kind == Kind::Video,
                Filter::Pinned => i.pinned,
            })
            .filter(|(_, i)| i.matches(&self.query))
            .map(|(n, _)| n)
            .collect()
    }

    fn handle_screenshot(&mut self, ctx: &egui::Context) {
        let Some(shot) = self.shot.as_mut() else { return };
        shot.frames += 1;
        ctx.request_repaint();
        if shot.frames >= 120 && !shot.requested {
            shot.requested = true;
            let path = shot.path.clone();
            let done = shot.done.clone();
            let ctx2 = ctx.clone();
            ctx.request_screenshot(move |image| {
                let [w, h] = image.size;
                let mut bytes = Vec::with_capacity(w * h * 4);
                for p in &image.pixels {
                    bytes.extend_from_slice(&p.to_srgba_unmultiplied());
                }
                if let Some(img) = image::RgbaImage::from_raw(w as u32, h as u32, bytes) {
                    if let Some(parent) = path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let _ = img.save(&path);
                }
                done.store(true, Ordering::SeqCst);
                ctx2.request_repaint();
            });
        }
        if shot.done.load(Ordering::SeqCst) || shot.frames > 900 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn keyboard(&mut self, ctx: &egui::Context, visible: &[usize]) {
        if visible.is_empty() {
            return;
        }
        let (down, up, enter, esc) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::ArrowDown),
                i.key_pressed(egui::Key::ArrowUp),
                i.key_pressed(egui::Key::Enter),
                i.key_pressed(egui::Key::Escape),
            )
        });
        if down {
            self.selected = (self.selected + 1).min(visible.len() - 1);
        }
        if up {
            self.selected = self.selected.saturating_sub(1);
        }
        self.selected = self.selected.min(visible.len() - 1);
        if enter {
            let id = self.items[visible[self.selected]].id;
            self.copy(ctx, id);
        }
        if esc && !self.sample_mode {
            mac::hide_app();
        }
    }
}

fn paint_kind_icon(painter: &egui::Painter, rect: egui::Rect, kind: Kind) {
    let color = kind_color(kind);
    painter.rect_filled(rect, CornerRadius::same(9), color.gamma_multiply(0.18));
    let c = rect.center();
    let s = Stroke::new(1.8, color);
    match kind {
        Kind::Text => {
            for (i, len) in [12.0, 9.0, 12.0].iter().enumerate() {
                let y = c.y - 5.0 + i as f32 * 5.0;
                painter.line_segment([egui::pos2(c.x - 6.0, y), egui::pos2(c.x - 6.0 + len, y)], s);
            }
        }
        Kind::Image => {
            let r = egui::Rect::from_center_size(c, Vec2::new(16.0, 13.0));
            painter.rect_stroke(r, CornerRadius::same(2), s, egui::StrokeKind::Middle);
            painter.circle_filled(egui::pos2(r.left() + 4.5, r.top() + 4.0), 1.6, color);
            painter.line_segment([egui::pos2(r.left() + 1.0, r.bottom() - 1.5), egui::pos2(c.x, c.y)], s);
            painter.line_segment([egui::pos2(c.x, c.y), egui::pos2(r.right() - 1.0, r.bottom() - 2.0)], s);
        }
        Kind::Video => {
            let pts = vec![egui::pos2(c.x - 4.0, c.y - 6.0), egui::pos2(c.x + 6.0, c.y), egui::pos2(c.x - 4.0, c.y + 6.0)];
            painter.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
        }
        Kind::File => {
            let r = egui::Rect::from_center_size(c, Vec2::new(12.0, 15.0));
            painter.rect_stroke(r, CornerRadius::same(2), s, egui::StrokeKind::Middle);
        }
    }
}

fn kind_color(kind: Kind) -> Color32 {
    match kind {
        Kind::Text => C_TEXT,
        Kind::Image => C_IMAGE,
        Kind::Video => C_VIDEO,
        Kind::File => C_FILE,
    }
}

fn chip(ui: &mut egui::Ui, text: &str, color: Color32) {
    Frame::new()
        .fill(color.gamma_multiply(0.16))
        .corner_radius(CornerRadius::same(5))
        .inner_margin(Margin::symmetric(6, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(10.5).color(color).strong());
        });
}

fn action_button(ui: &mut egui::Ui, label: &str, sub: &str, color: Color32, width: f32, video: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 54.0), Sense::click());
    let p = ui.painter();
    let fill = if resp.hovered() { CARD_HOVER } else { CARD };
    p.rect(rect, CornerRadius::same(12), fill, Stroke::new(1.0, BORDER), egui::StrokeKind::Inside);
    let ic = egui::pos2(rect.left() + 28.0, rect.center().y);
    p.circle_filled(ic, 15.0, color.gamma_multiply(0.2));
    if video {
        p.circle_filled(ic, 6.5, color);
    } else {
        // A dashed selection square for "snip".
        let r = egui::Rect::from_center_size(ic, Vec2::splat(13.0));
        let s = Stroke::new(1.8, color);
        for (a, b) in [
            (r.left_top(), r.right_top()),
            (r.right_top(), r.right_bottom()),
            (r.right_bottom(), r.left_bottom()),
            (r.left_bottom(), r.left_top()),
        ] {
            p.extend(egui::Shape::dashed_line(&[a, b], s, 3.0, 2.0));
        }
    }
    p.text(
        egui::pos2(rect.left() + 52.0, rect.top() + 17.0),
        egui::Align2::LEFT_CENTER,
        label,
        FontId::proportional(14.5),
        TEXT,
    );
    p.text(
        egui::pos2(rect.left() + 52.0, rect.top() + 36.0),
        egui::Align2::LEFT_CENTER,
        sub,
        FontId::proportional(11.5),
        MUTED,
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_incoming(ctx);
        let now = Self::now(ctx);

        if let Some(t) = self.hide_at {
            if now >= t {
                self.hide_at = None;
                mac::hide_app();
            } else {
                ctx.request_repaint();
            }
        }

        // Ask for thumbnails of pictures and videos that don't have one yet.
        if !self.sample_mode {
            let need: Vec<(u64, PathBuf, Kind)> = self
                .items
                .iter()
                .filter(|i| matches!(i.kind, Kind::Image | Kind::Video))
                .filter(|i| !self.thumbs.contains_key(&i.id) && !self.thumb_pending.contains(&i.id))
                .filter_map(|i| i.path.clone().map(|p| (i.id, p, i.kind)))
                .take(6)
                .collect();
            for (id, p, k) in need {
                self.thumb_pending.insert(id);
                watch::start_thumbnail(self.tx.clone(), ctx.clone(), id, p, k);
            }
        }

        if self.dirty && !self.sample_mode {
            model::save_history(&self.items);
            self.dirty = false;
        }
        self.handle_screenshot(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = &ui.ctx().clone();
        let now = Self::now(ctx);
        if !self.sample_mode && ctx.input(|i| i.viewport().close_requested()) {
            // Closing the window keeps Snip Shelf running in the menu bar, still recording history.
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            mac::hide_app();
        }
        let visible = self.visible();
        self.keyboard(ctx, &visible);

        // ---------- Header ----------
        egui::Panel::top("header")
            .frame(Frame::new().fill(PANEL).inner_margin(Margin { left: 16, right: 16, top: 14, bottom: 12 }))
            .show_separator_line(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::hover());
                    ui.painter().rect_filled(r, CornerRadius::same(9), ACCENT);
                    let inner = r.shrink(8.0);
                    ui.painter().rect_stroke(inner, CornerRadius::same(2), Stroke::new(1.8, Color32::WHITE), egui::StrokeKind::Middle);
                    ui.painter().line_segment(
                        [egui::pos2(inner.left() + 3.0, inner.center().y), egui::pos2(inner.right() - 3.0, inner.center().y)],
                        Stroke::new(1.8, Color32::WHITE),
                    );
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        ui.label(RichText::new("Snip Shelf").size(18.0).strong().color(TEXT));
                        ui.label(RichText::new("Click any item, then press ⌘V to paste it").size(11.5).color(MUTED));
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.menu_button(RichText::new("Settings").size(12.5), |ui| {
                            ui.set_min_width(250.0);
                            let mut changed = false;
                            changed |= ui
                                .checkbox(&mut self.settings.hide_after_copy, "Hide this window after I click an item")
                                .changed();
                            if ui
                                .checkbox(&mut self.settings.auto_copy_new_captures, "Copy new screenshots/recordings right away")
                                .changed()
                            {
                                self.auto_copy.store(self.settings.auto_copy_new_captures, Ordering::SeqCst);
                                changed = true;
                            }
                            ui.horizontal(|ui| {
                                ui.label("Keep up to");
                                changed |= ui
                                    .add(egui::DragValue::new(&mut self.settings.max_items).range(20..=2000).speed(5))
                                    .changed();
                                ui.label("items");
                            });
                            ui.separator();
                            ui.label(
                                RichText::new(format!("Watching screenshots in:\n{}", watch::screenshot_folder().display()))
                                    .size(11.0)
                                    .color(MUTED),
                            );
                            ui.label(
                                RichText::new(
                                    "Everything stays on this Mac — nothing is uploaded.\nSnips, recordings and copied pictures are saved in\nSnip Shelf's own folder. Pinned items stay forever,\neven after you quit or restart.",
                                )
                                .size(11.0)
                                .color(MUTED),
                            );
                            if ui.button("Show saved pictures & videos").clicked() {
                                let _ = std::process::Command::new("open").arg(model::captures_dir()).spawn();
                            }
                            ui.separator();
                            if ui.button("Add to Applications (open with ⌘Space)").clicked() && !self.sample_mode {
                                self.add_to_applications(ctx);
                                ui.close();
                            }
                            if changed && !self.sample_mode {
                                model::save_settings(&self.settings);
                            }
                        });
                    });
                });
                ui.add_space(12.0);
                let w = (ui.available_width() - 10.0) / 2.0;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    if action_button(ui, "Snip area", "Pick any part of the screen", C_IMAGE, w, false).clicked()
                        && !self.sample_mode
                    {
                        mac::hide_app();
                        watch::run_capture(false, self.tx.clone(), ctx.clone());
                    }
                    if action_button(ui, "Record screen", "Stop it from the menu bar", C_VIDEO, w, true).clicked()
                        && !self.sample_mode
                    {
                        mac::hide_app();
                        watch::run_capture(true, self.tx.clone(), ctx.clone());
                    }
                });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.query)
                            .hint_text("Search your history…")
                            .desired_width(ui.available_width())
                            .margin(Margin::symmetric(10, 7)),
                    );
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let count = |k: Option<Kind>, pinned: bool| {
                        self.items
                            .iter()
                            .filter(|i| if pinned { i.pinned } else { k.map(|k| i.kind == k).unwrap_or(true) })
                            .count()
                    };
                    let tabs = [
                        (Filter::All, "All", count(None, false)),
                        (Filter::Text, "Text", count(Some(Kind::Text), false)),
                        (Filter::Pictures, "Pictures", count(Some(Kind::Image), false)),
                        (Filter::Videos, "Videos", count(Some(Kind::Video), false)),
                        (Filter::Pinned, "Pinned", count(None, true)),
                    ];
                    for (f, label, n) in tabs {
                        let on = self.filter == f;
                        let text = RichText::new(format!("{label}  {n}"))
                            .size(12.0)
                            .color(if on { Color32::WHITE } else { MUTED });
                        let b = egui::Button::new(text)
                            .fill(if on { ACCENT } else { CARD })
                            .stroke(if on { Stroke::NONE } else { Stroke::new(1.0, BORDER) })
                            .corner_radius(CornerRadius::same(14));
                        if ui.add(b).clicked() {
                            self.filter = f;
                            self.selected = 0;
                        }
                    }
                });
            });

        // ---------- Footer ----------
        egui::Panel::bottom("footer")
            .frame(Frame::new().fill(PANEL).inner_margin(Margin::symmetric(16, 9)))
            .show_separator_line(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let hint = if self.hotkey_ok || self.sample_mode {
                        "Control+Shift+V opens Snip Shelf  ·  ↑↓ Enter"
                    } else {
                        "↑↓ + Enter to pick  ·  Esc hides"
                    };
                    ui.label(RichText::new(hint).size(11.5).color(MUTED));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add(egui::Button::new(RichText::new("Clear unpinned").size(11.5)).corner_radius(CornerRadius::same(6)))
                            .clicked()
                            && !self.sample_mode
                        {
                            self.clear_unpinned();
                        }
                    });
                });
            });

        // ---------- History list ----------
        let mut to_copy: Option<u64> = None;
        let mut to_delete: Option<u64> = None;
        let mut to_pin: Option<u64> = None;
        let mut to_reveal: Option<PathBuf> = None;
        let latest_id = self.items.first().map(|i| i.id);
        egui::CentralPanel::default()
            .frame(Frame::new().fill(BG).inner_margin(Margin { left: 14, right: 14, top: 10, bottom: 6 }))
            .show(ui, |ui| {
                if visible.is_empty() {
                    ui.add_space(60.0);
                    ui.vertical_centered(|ui| {
                        let msg = if self.items.is_empty() {
                            "Nothing here yet"
                        } else {
                            "Nothing matches"
                        };
                        ui.label(RichText::new(msg).size(17.0).strong());
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(
                                "Copy some text (⌘C), take a screenshot (⌘⇧4)\nor use Snip / Record above — it shows up here.",
                            )
                            .size(12.5)
                            .color(MUTED),
                        );
                    });
                    return;
                }
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 8.0;
                    for (row, &idx) in visible.iter().enumerate() {
                        let item = &self.items[idx];
                        let selected = row == self.selected;
                        let flashing = self.flash.map(|(id, t)| id == item.id && now - t < 1.2).unwrap_or(false);
                        let resp = ui
                            .scope_builder(UiBuilder::new().sense(Sense::click()), |ui| {
                                let hovered = ui.response().hovered();
                                let fill = if flashing {
                                    ACCENT.gamma_multiply(0.25)
                                } else if hovered {
                                    CARD_HOVER
                                } else {
                                    CARD
                                };
                                let stroke = if selected || flashing {
                                    Stroke::new(1.5, ACCENT)
                                } else {
                                    Stroke::new(1.0, BORDER)
                                };
                                Frame::new()
                                    .fill(fill)
                                    .stroke(stroke)
                                    .corner_radius(CornerRadius::same(12))
                                    .inner_margin(Margin::same(12))
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.horizontal_top(|ui| {
                                            let (r, _) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::hover());
                                            paint_kind_icon(ui.painter(), r, item.kind);
                                            ui.vertical(|ui| {
                                                ui.spacing_mut().item_spacing.y = 6.0;
                                                // Top line: source, time and badges + actions.
                                                ui.horizontal(|ui| {
                                                    ui.spacing_mut().item_spacing.x = 6.0;
                                                    ui.label(
                                                        RichText::new(&item.source).size(12.0).strong().color(kind_color(item.kind)),
                                                    );
                                                    ui.label(RichText::new(format!("· {}", model::time_ago(item.created))).size(11.5).color(MUTED));
                                                    if Some(item.id) == latest_id {
                                                        chip(ui, "LATEST", ACCENT);
                                                    }
                                                    if item.pinned {
                                                        chip(ui, "PINNED", C_FILE);
                                                    }
                                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                                        if hovered || item.pinned || selected {
                                                            ui.spacing_mut().button_padding = Vec2::new(7.0, 2.0);
                                                            if ui
                                                                .add(egui::Button::new(RichText::new("Delete").size(11.0).color(MUTED)).frame(false))
                                                                .on_hover_text("Remove from history")
                                                                .clicked()
                                                            {
                                                                to_delete = Some(item.id);
                                                            }
                                                            if item.kind != Kind::Text
                                                                && ui
                                                                    .add(egui::Button::new(RichText::new("Show in Finder").size(11.0).color(MUTED)).frame(false))
                                                                    .on_hover_text("See where this file is saved on your Mac")
                                                                    .clicked()
                                                            {
                                                                to_reveal = item.path.clone();
                                                            }
                                                            let pin_label = if item.pinned { "Unpin" } else { "Pin" };
                                                            if ui
                                                                .add(egui::Button::new(RichText::new(pin_label).size(11.0).color(MUTED)).frame(false))
                                                                .on_hover_text("Pinned items are never cleared")
                                                                .clicked()
                                                            {
                                                                to_pin = Some(item.id);
                                                            }
                                                        }
                                                    });
                                                });
                                                match item.kind {
                                                    Kind::Text => {
                                                        let t = item.text.clone().unwrap_or_default();
                                                        let mut preview: String = t.chars().take(260).collect();
                                                        if t.chars().count() > 260 {
                                                            preview.push('…');
                                                        }
                                                        ui.add(
                                                            egui::Label::new(RichText::new(preview).size(13.5).color(TEXT))
                                                                .wrap()
                                                                .selectable(false),
                                                        );
                                                    }
                                                    _ => {
                                                        if item.file_missing() && !self.sample_mode {
                                                            ui.label(RichText::new("The file was moved or deleted").size(12.5).color(MUTED));
                                                        } else if let Some(tex) = self.thumbs.get(&item.id) {
                                                            let maxw = ui.available_width();
                                                            let size = tex.size_vec2();
                                                            let scale = (maxw / size.x).min(150.0 / size.y).min(1.0);
                                                            let img_size = size * scale;
                                                            let (r, _) = ui.allocate_exact_size(img_size, Sense::hover());
                                                            egui::Image::new((tex.id(), img_size))
                                                                .corner_radius(CornerRadius::same(8))
                                                                .paint_at(ui, r);
                                                            if item.kind == Kind::Video {
                                                                let c = r.center();
                                                                ui.painter().circle_filled(c, 18.0, Color32::from_black_alpha(150));
                                                                let pts = vec![
                                                                    egui::pos2(c.x - 5.0, c.y - 8.0),
                                                                    egui::pos2(c.x + 9.0, c.y),
                                                                    egui::pos2(c.x - 5.0, c.y + 8.0),
                                                                ];
                                                                ui.painter().add(egui::Shape::convex_polygon(pts, Color32::WHITE, Stroke::NONE));
                                                            }
                                                        } else if item.kind == Kind::File || self.thumb_failed.contains(&item.id) {
                                                            // no picture for plain files or unreadable ones
                                                        } else {
                                                            ui.label(RichText::new("Making preview…").size(12.0).color(MUTED));
                                                        }
                                                        let name = if self.sample_mode {
                                                            match item.kind {
                                                                Kind::Video => "Screen Recording.mov".to_string(),
                                                                _ => "Screenshot.png".to_string(),
                                                            }
                                                        } else {
                                                            item.title()
                                                        };
                                                        ui.label(RichText::new(name).size(11.5).color(MUTED));
                                                    }
                                                }
                                            });
                                        });
                                    });
                            })
                            .response;
                        let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                        if resp.clicked() && to_delete.is_none() && to_pin.is_none() && to_reveal.is_none() {
                            to_copy = Some(item.id);
                            self.selected = row;
                        }
                        if selected && ctx.input(|i| i.key_pressed(egui::Key::ArrowDown) || i.key_pressed(egui::Key::ArrowUp)) {
                            resp.scroll_to_me(None);
                        }
                    }
                });
            });

        if let Some(id) = to_delete {
            self.delete(id);
        }
        if let Some(p) = to_reveal {
            if !self.sample_mode {
                let _ = std::process::Command::new("open").arg("-R").arg(p).spawn();
            }
        }
        if let Some(id) = to_pin {
            if let Some(it) = self.items.iter_mut().find(|i| i.id == id) {
                it.pinned = !it.pinned;
                self.dirty = true;
            }
        }
        if let Some(id) = to_copy {
            self.copy(ctx, id);
        }

        // ---------- Toast ----------
        if let Some((msg, t)) = self.toast.clone() {
            let age = if t == f64::MAX { 0.0 } else { now - t };
            if age < 3.0 {
                #[allow(deprecated)]
                egui::Area::new(egui::Id::new("toast"))
                    .anchor(egui::Align2::CENTER_BOTTOM, Vec2::new(0.0, -50.0))
                    .order(egui::Order::Foreground)
                    .interactable(false)
                    .show(ui, |ui| {
                        Frame::new()
                            .fill(Color32::from_rgb(0x2A, 0x2F, 0x3B))
                            .stroke(Stroke::new(1.0, ACCENT.gamma_multiply(0.6)))
                            .corner_radius(CornerRadius::same(18))
                            .inner_margin(Margin::symmetric(16, 9))
                            .show(ui, |ui| {
                                ui.label(RichText::new(msg).size(13.0).color(TEXT).strong());
                            });
                    });
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
            } else {
                self.toast = None;
            }
        }

        if self.flash.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        // Keep "2 min ago" labels fresh and notice new items even when nothing else happens.
        ctx.request_repaint_after(std::time::Duration::from_secs(5));

    }
}

/// The Snip Shelf.app this program runs from, if it runs from one.
fn current_bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    exe.ancestors()
        .find(|p| p.extension().map(|e| e == "app").unwrap_or(false))
        .map(|p| p.to_path_buf())
}

/// Keep our own copy of screenshots that macOS saved on the Desktop, so the history
/// still works if the Desktop file is deleted. Recordings are big, so they stay where they are.
fn keep_copy(path: PathBuf, kind: Kind, source: &str) -> PathBuf {
    if kind != Kind::Image || source != "Screenshot" {
        return path;
    }
    let captures = model::captures_dir();
    if model::is_inside(&path, &captures) {
        return path;
    }
    let Some(name) = path.file_name() else { return path };
    let dest = captures.join(name);
    if std::fs::copy(&path, &dest).is_ok() {
        dest
    } else {
        path
    }
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    // Used by the packaging script to draw the app icon.
    if let Some(i) = args.iter().position(|a| a == "--write-icon") {
        if let Some(out) = args.get(i + 1) {
            let n = 1024;
            if let Some(img) = image::RgbaImage::from_raw(n, n, icon::app_icon(n)) {
                let _ = img.save(out);
            }
        }
        return Ok(());
    }
    let shot = args
        .iter()
        .position(|a| a == "--screenshot")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from);

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Snip Shelf")
        .with_inner_size([460.0, 720.0])
        .with_min_inner_size([360.0, 420.0])
        .with_icon(Arc::new(egui::IconData { rgba: icon::app_icon(256), width: 256, height: 256 }));
    if shot.is_some() {
        viewport = viewport.with_active(false);
        #[cfg(target_os = "windows")]
        {
            viewport = viewport.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
    }
    let mut options = eframe::NativeOptions { viewport, ..Default::default() };
    if shot.is_some() {
        #[cfg(target_os = "macos")]
        {
            options.event_loop_builder = Some(Box::new(|builder| {
                use winit::platform::macos::EventLoopBuilderExtMacOS;
                builder.with_activate_ignoring_other_apps(false);
            }));
        }
    }
    eframe::run_native("Snip Shelf", options, Box::new(move |cc| Ok(Box::new(App::new(cc, shot)))))
}
