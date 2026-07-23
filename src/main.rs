#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use gweditor::save::facts::{parse_facts, Fact};
use gweditor::save::inventory::{find_inventories, Inventory, Item};
use gweditor::save::SaveFile;

const CINZEL_TTF: &[u8] = include_bytes!("../assets/Cinzel.ttf");
const ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");

// The Witcher 3 menu palette: aged parchment on dark leather, antique gold
// trim, blood-red accents, sword steel for values.
const BG: egui::Color32 = egui::Color32::from_rgb(0x12, 0x0e, 0x0b);
const CARD: egui::Color32 = egui::Color32::from_rgb(0x20, 0x19, 0x13);
const CARD_HOVER: egui::Color32 = egui::Color32::from_rgb(0x2a, 0x21, 0x18);
const STRIPE: egui::Color32 = egui::Color32::from_rgb(0x1a, 0x14, 0x10);
const INK: egui::Color32 = egui::Color32::from_rgb(0x0d, 0x0a, 0x08);
const TEXT: egui::Color32 = egui::Color32::from_rgb(0xd6, 0xcb, 0xb8);
const TEXT_BRIGHT: egui::Color32 = egui::Color32::from_rgb(0xf0, 0xe6, 0xd2);
const WEAK: egui::Color32 = egui::Color32::from_rgb(0x8d, 0x81, 0x71);
const GOLD: egui::Color32 = egui::Color32::from_rgb(0xc8, 0xa4, 0x4d);
const GOLD_DIM: egui::Color32 = egui::Color32::from_rgb(0x6e, 0x57, 0x2b);
const BORDER: egui::Color32 = egui::Color32::from_rgb(0x3a, 0x2f, 0x20);
const RED: egui::Color32 = egui::Color32::from_rgb(0xb3, 0x24, 0x2f);
const RED_DARK: egui::Color32 = egui::Color32::from_rgb(0x45, 0x10, 0x15);
const STEEL: egui::Color32 = egui::Color32::from_rgb(0xb9, 0xc0, 0xc7);
const GREEN: egui::Color32 = egui::Color32::from_rgb(0x7f, 0xa6, 0x50);

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1160.0, 760.0])
            .with_min_inner_size([900.0, 560.0])
            .with_app_id("gweditor")
            .with_icon(Arc::new(load_icon()))
            .with_title("Gweditor — Witcher 3 Save Editor"),
        ..Default::default()
    };
    eframe::run_native(
        "Gweditor",
        options,
        Box::new(|cc| {
            setup_style(&cc.egui_ctx);
            let mut app = App::default();
            match std::env::var("GWEDITOR_TAB").as_deref() {
                Ok("world") => app.tab = Tab::World,
                Ok("inventories") => app.tab = Tab::Inventories,
                _ => {}
            }
            match std::env::var("GWEDITOR_BAG").as_deref() {
                Ok("gear") => app.bag = Bag::Gear,
                Ok("quick") => app.bag = Bag::QuickAccess,
                Ok("alchemy") => app.bag = Bag::Alchemy,
                Ok("ingredients") => app.bag = Bag::Ingredients,
                Ok("quest") => app.bag = Bag::Quest,
                Ok("books") => app.bag = Bag::Books,
                Ok("gwent") => app.bag = Bag::Gwent,
                Ok("other") => app.bag = Bag::Other,
                _ => {}
            }
            if let Some(path) = std::env::args().nth(1) {
                app.open(PathBuf::from(path));
            }
            Ok(Box::new(app))
        }),
    )
}

fn load_icon() -> egui::IconData {
    let img = image::load_from_memory(ICON_PNG)
        .expect("bundled icon is valid")
        .to_rgba8();
    let (width, height) = img.dimensions();
    egui::IconData {
        rgba: img.into_raw(),
        width,
        height,
    }
}

/// Cinzel has no true lowercase — minuscules render as small capitals,
/// which is exactly the look of the game's menus.
fn cinzel() -> egui::FontFamily {
    egui::FontFamily::Name("cinzel".into())
}

fn serif(size: f32) -> egui::FontId {
    egui::FontId::new(size, cinzel())
}

fn setup_style(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "cinzel".to_owned(),
        egui::FontData::from_static(CINZEL_TTF).into(),
    );
    let mut display = vec!["cinzel".to_owned()];
    display.extend(
        fonts.families[&egui::FontFamily::Proportional]
            .iter()
            .cloned(),
    );
    fonts.families.insert(cinzel(), display);
    ctx.set_fonts(fonts);

    use egui::{FontFamily, FontId, TextStyle};
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Heading, FontId::new(26.0, cinzel())),
            (TextStyle::Body, FontId::new(14.5, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
            (TextStyle::Button, FontId::new(14.5, FontFamily::Proportional)),
            (TextStyle::Small, FontId::new(11.5, FontFamily::Proportional)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(14.0, 5.0);
        style.visuals = themed_visuals();
    });
}

fn themed_visuals() -> egui::Visuals {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = BG;
    v.extreme_bg_color = INK;
    v.faint_bg_color = STRIPE;

    v.widgets.noninteractive.bg_fill = BG;
    v.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, TEXT);
    v.widgets.noninteractive.bg_stroke =
        egui::Stroke::new(1.0, egui::Color32::from_rgb(0x2e, 0x25, 0x1b));

    v.widgets.inactive.weak_bg_fill = CARD;
    v.widgets.inactive.bg_fill = CARD;
    v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, BORDER);
    v.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, TEXT);

    v.widgets.hovered.weak_bg_fill = CARD_HOVER;
    v.widgets.hovered.bg_fill = CARD_HOVER;
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, GOLD_DIM);
    v.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, TEXT_BRIGHT);

    v.widgets.active.weak_bg_fill = RED_DARK;
    v.widgets.active.bg_fill = RED_DARK;
    v.widgets.active.bg_stroke = egui::Stroke::new(1.0, RED);
    v.widgets.active.fg_stroke = egui::Stroke::new(1.0, TEXT_BRIGHT);

    v.widgets.open.weak_bg_fill = CARD_HOVER;
    v.widgets.open.bg_fill = CARD_HOVER;
    v.widgets.open.bg_stroke = egui::Stroke::new(1.0, GOLD_DIM);

    v.selection.bg_fill = RED_DARK;
    v.selection.stroke = egui::Stroke::new(1.0, RED);
    v.hyperlink_color = GOLD;
    v
}

/// Draw the wolf-school medallion: gold ring, crossed steel and silver
/// swords, red core with a slow pulse.
fn draw_medallion(painter: &egui::Painter, center: egui::Pos2, r: f32, time: f64) {
    use egui::{pos2, vec2, Color32, Stroke};

    let pulse = (0.5 + 0.5 * (time * 1.8).sin()) as f32;
    for i in 0..4 {
        let t = i as f32 / 4.0;
        painter.circle_filled(
            center,
            r * (0.55 + 0.4 * t),
            Color32::from_rgba_unmultiplied(
                0xb3,
                0x24,
                0x2f,
                ((1.0 - t) * 14.0 * (0.4 + 0.6 * pulse)) as u8,
            ),
        );
    }
    painter.circle_filled(center, r, Color32::from_rgb(0x18, 0x12, 0x0d));
    painter.circle_stroke(center, r, Stroke::new(r * 0.10, GOLD_DIM));
    painter.circle_stroke(center, r * 0.985, Stroke::new(1.2, GOLD));
    painter.circle_stroke(center, r * 0.80, Stroke::new(1.0, GOLD_DIM));

    for (dir_x, steel) in [(1.0f32, STEEL), (-1.0f32, Color32::from_rgb(0xd9, 0xdd, 0xe2))] {
        let d = vec2(
            dir_x * std::f32::consts::FRAC_1_SQRT_2,
            -std::f32::consts::FRAC_1_SQRT_2,
        );
        let p = vec2(-d.y, d.x);
        let at = |along: f32, side: f32| -> egui::Pos2 {
            pos2(
                center.x + d.x * r * along + p.x * r * side,
                center.y + d.y * r * along + p.y * r * side,
            )
        };
        painter.line_segment([at(-0.30, 0.0), at(0.72, 0.0)], Stroke::new(r * 0.085, steel));
        painter.line_segment(
            [at(-0.28, 0.0), at(0.66, 0.0)],
            Stroke::new(r * 0.02, Color32::from_rgb(0x6d, 0x74, 0x7b)),
        );
        painter.line_segment(
            [at(-0.33, -0.16), at(-0.33, 0.16)],
            Stroke::new(r * 0.06, GOLD),
        );
        painter.line_segment(
            [at(-0.36, 0.0), at(-0.56, 0.0)],
            Stroke::new(r * 0.06, Color32::from_rgb(0x4a, 0x30, 0x1c)),
        );
        painter.circle_filled(at(-0.62, 0.0), r * 0.055, GOLD);
    }

    painter.circle_filled(center, r * 0.10, RED);
    painter.circle_stroke(center, r * 0.10, Stroke::new(1.0, GOLD_DIM));
}

/// A thin gold rule with a small diamond at the center — the ornament the
/// game uses to close off its menu headers.
fn ornament_divider(ui: &mut egui::Ui) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 9.0), egui::Sense::hover());
    let painter = ui.painter();
    let y = rect.center().y;
    let cx = rect.center().x;
    let half_gap = 10.0;
    painter.line_segment(
        [
            egui::pos2(rect.left() + 4.0, y),
            egui::pos2(cx - half_gap, y),
        ],
        egui::Stroke::new(1.0, GOLD_DIM),
    );
    painter.line_segment(
        [
            egui::pos2(cx + half_gap, y),
            egui::pos2(rect.right() - 4.0, y),
        ],
        egui::Stroke::new(1.0, GOLD_DIM),
    );
    let d = 3.5;
    painter.add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(cx, y - d),
            egui::pos2(cx + d, y),
            egui::pos2(cx, y + d),
            egui::pos2(cx - d, y),
        ],
        GOLD,
        egui::Stroke::NONE,
    ));
}

// ---------------------------------------------------------------- app state

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Geralt,
    Inventories,
    World,
}

/// Sub-tabs of the Geralt screen, mirroring the game's inventory bags.
#[derive(PartialEq, Clone, Copy, Debug)]
enum Bag {
    All,
    Gear,
    QuickAccess,
    Alchemy,
    Ingredients,
    Quest,
    Books,
    Gwent,
    Other,
}

const BAGS: [(Bag, &str); 9] = [
    (Bag::All, "All"),
    (Bag::Gear, "Weapons & Armour"),
    (Bag::QuickAccess, "Quick Access"),
    (Bag::Alchemy, "Alchemy"),
    (Bag::Ingredients, "Ingredients"),
    (Bag::Quest, "Quest Items"),
    (Bag::Books, "Books & Letters"),
    (Bag::Gwent, "Gwent Cards"),
    (Bag::Other, "Other"),
];

const BOMBS: [&str; 9] = [
    "grapeshot",
    "samum",
    "dancing star",
    "devil's puffball",
    "devils puffball",
    "dragon's dream",
    "dragons dream",
    "moon dust",
    "northern wind",
];

fn any(hay: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| hay.contains(n))
}

/// True when `word` appears as a whole word ("bodkin bolt" matches "bolt",
/// "thunderbolt" does not).
fn word(hay: &str, word: &str) -> bool {
    hay.split(|c: char| !c.is_ascii_alphanumeric())
        .any(|w| w == word)
}

/// Base names of the game's potions, which carry no "potion" in their id.
const POTIONS: [&str; 12] = [
    "swallow",
    "thunderbolt",
    "tawny owl",
    "blizzard",
    "full moon",
    "golden oriole",
    "maribor forest",
    "petri",
    "white honey",
    "white raffard",
    "killer whale",
    "trial potion",
];

/// Best-effort mapping of an item to the bag the game would show it in.
/// Works off the item id, durability and stack size; unknowns land in Other.
fn bag_of(item: &Item) -> Bag {
    let n = item.name.to_lowercase();

    if n.contains("gwint") || n.contains("gwent") {
        return Bag::Gwent;
    }
    if any(&n, &["schematic", "recipe", "diagram"]) {
        return Bag::Ingredients;
    }
    if any(&n, &["book", "letter", "note", "journal", "diary", "manuscript", "lore_", "painting", "map "]) {
        return Bag::Books;
    }
    // Body parts, haircuts and other engine-side pseudo items.
    if any(&n, &["body ", "head_", "hair", "fists", "preview", "underwear"]) {
        return Bag::Other;
    }
    if any(&n, &POTIONS)
        || any(&n, &["potion", "decoction", "elixir", "philt", "mutagen", "bomb", " oil"])
        || n.ends_with("oil")
        || any(&n, &BOMBS)
    {
        return Bag::Alchemy;
    }
    if item.durability >= 0.0
        || word(&n, "bolt")
        || any(
            &n,
            &[
                "sword", "armor", "armour", "boots", "gloves", "pants", "trousers", "gauntlet",
                "shirt", "jacket", "crossbow", "scabbard", "saddle", "blinders", "horse bag",
                "trophy",
            ],
        )
    {
        return Bag::Gear;
    }
    if any(
        &n,
        &[
            "milk", "bread", "water", "meat", "fish", "cheese", "ale", "beer", "wine", "mead",
            "vodka", "spirit", "juice", "honey", "apple", "pear", "potato", "soup", "roast",
            "jerky", "sandwich", "pie", "tart", "bun", "porridge", "grilled", "baked", "torch",
            "food",
        ],
    ) {
        return Bag::QuickAccess;
    }
    if n == "crowns" || n == "orens" || n == "florens" {
        return Bag::Other;
    }
    // Quest and scripting items use lowercase ids with underscores.
    if n.contains('_') || n.starts_with('q') && n[1..2].chars().all(|c| c.is_ascii_digit()) {
        return Bag::Quest;
    }
    if item.quantity > 1 {
        return Bag::Ingredients;
    }
    Bag::Other
}

#[derive(PartialEq)]
enum Severity {
    Info,
    Success,
    Error,
}

struct Loaded {
    path: PathBuf,
    save: SaveFile,
    inventories: Vec<Inventory>,
    /// Index of Geralt's own inventory within `inventories`, if identified.
    player_inv: Option<usize>,
    facts: Vec<Fact>,
    dirty: bool,
}

struct App {
    loaded: Option<Loaded>,
    tab: Tab,
    bag: Bag,
    status: (Severity, String),
    inv_selected: usize,
    inv_filter: String,
    geralt_filter: String,
    fact_filter: String,
}

impl Default for App {
    fn default() -> Self {
        App {
            loaded: None,
            tab: Tab::Geralt,
            bag: Bag::All,
            status: (Severity::Info, "No save loaded.".to_string()),
            inv_selected: 0,
            inv_filter: String::new(),
            geralt_filter: String::new(),
            fact_filter: String::new(),
        }
    }
}

/// Default Witcher 3 save locations to start the open dialog in.
fn default_save_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)?;
    let candidates = [
        home.join(".steam/steam/steamapps/compatdata/292030/pfx/drive_c/users/steamuser/Documents/The Witcher 3/gamesaves"),
        home.join("Documents/The Witcher 3/gamesaves"),
    ];
    candidates.into_iter().find(|p| p.is_dir())
}

impl App {
    fn open_dialog(&mut self) {
        let mut dialog = rfd::FileDialog::new().add_filter("Witcher 3 saves", &["sav"]);
        if let Some(dir) = self
            .loaded
            .as_ref()
            .and_then(|l| l.path.parent().map(|p| p.to_path_buf()))
            .or_else(default_save_dir)
        {
            dialog = dialog.set_directory(dir);
        }
        if let Some(path) = dialog.pick_file() {
            self.open(path);
        }
    }

    fn open(&mut self, path: PathBuf) {
        match SaveFile::load(&path) {
            Ok(save) => {
                let inventories = find_inventories(&save);
                let player_inv = inventories
                    .iter()
                    .position(|i| i.label.contains("PLAYER") || i.label.contains("GERALT"));
                let facts = parse_facts(&save).unwrap_or_default();
                self.status = (
                    Severity::Success,
                    format!(
                        "Loaded {} — {} inventories, {} facts.",
                        path.file_name().unwrap_or_default().to_string_lossy(),
                        inventories.len(),
                        facts.len(),
                    ),
                );
                self.inv_selected = 0;
                self.loaded = Some(Loaded {
                    path,
                    save,
                    inventories,
                    player_inv,
                    facts,
                    dirty: false,
                });
            }
            Err(e) => self.status = (Severity::Error, format!("Failed to load: {e:#}")),
        }
    }

    fn save_to(&mut self, path: PathBuf) {
        let Some(loaded) = &mut self.loaded else { return };
        // Keep a one-time backup when overwriting the loaded file.
        if path == loaded.path {
            let bak = path.with_extension("sav.gwbak");
            if !bak.exists() {
                if let Err(e) = std::fs::copy(&path, &bak) {
                    self.status =
                        (Severity::Error, format!("Backup failed, save aborted: {e}"));
                    return;
                }
            }
        }
        match loaded.save.save(&path) {
            Ok(()) => {
                loaded.dirty = false;
                loaded.path = path.clone();
                self.status = (Severity::Success, format!("Saved {}", path.display()));
            }
            Err(e) => self.status = (Severity::Error, format!("Save failed: {e:#}")),
        }
    }

    fn save_as_dialog(&mut self) {
        let Some(loaded) = &self.loaded else { return };
        let mut dialog = rfd::FileDialog::new().add_filter("Witcher 3 saves", &["sav"]);
        if let Some(dir) = loaded.path.parent() {
            dialog = dialog.set_directory(dir);
        }
        if let Some(name) = loaded.path.file_name() {
            dialog = dialog.set_file_name(name.to_string_lossy());
        }
        if let Some(path) = dialog.save_file() {
            self.save_to(path);
        }
    }
}

// ---------------------------------------------------------------- widgets

fn action_button(ui: &mut egui::Ui, label: &str, enabled: bool) -> bool {
    ui.add_enabled(
        enabled,
        egui::Button::new(egui::RichText::new(label).font(serif(14.0)).color(GOLD)),
    )
    .clicked()
}

/// A main-menu tab: small-caps serif with a red diamond marking the active
/// entry, the way the game's panels do it.
fn tab_item(ui: &mut egui::Ui, label: &str, active: bool) -> bool {
    let color = if active { TEXT_BRIGHT } else { WEAK };
    let resp = ui
        .add(
            egui::Label::new(egui::RichText::new(label).font(serif(17.0)).color(color))
                .sense(egui::Sense::click()),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if active {
        let rect = resp.rect;
        let c = egui::pos2(rect.left() - 10.0, rect.center().y + 1.0);
        let d = 3.5;
        ui.painter().add(egui::Shape::convex_polygon(
            vec![
                egui::pos2(c.x, c.y - d),
                egui::pos2(c.x + d, c.y),
                egui::pos2(c.x, c.y + d),
                egui::pos2(c.x - d, c.y),
            ],
            RED,
            egui::Stroke::NONE,
        ));
    }
    resp.clicked()
}

/// A bag sub-tab: quieter than the main tabs, count in the label.
fn bag_item(ui: &mut egui::Ui, label: &str, count: usize, active: bool) -> bool {
    let text = format!("{label} ({count})");
    let color = if active { GOLD } else { WEAK };
    let resp = ui
        .add(
            egui::Label::new(egui::RichText::new(text).font(serif(12.5)).color(color))
                .sense(egui::Sense::click()),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    resp.clicked()
}

fn column_title(ui: &mut egui::Ui, title: &str) {
    ui.label(egui::RichText::new(title).font(serif(12.5)).color(GOLD));
}

// ---------------------------------------------------------------- main ui

impl eframe::App for App {
    fn ui(&mut self, root_ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root_ui.ctx().clone();
        // Keep the medallion glow breathing.
        ctx.request_repaint_after(std::time::Duration::from_millis(50));
        let time = ctx.input(|i| i.time);

        egui::Panel::top("header").show(root_ui, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(56.0, 56.0), egui::Sense::hover());
                draw_medallion(ui.painter(), rect.center(), 26.0, time);
                ui.add_space(6.0);
                ui.vertical(|ui| {
                    ui.add_space(2.0);
                    ui.label(
                        egui::RichText::new("Gweditor")
                            .font(serif(27.0))
                            .color(TEXT_BRIGHT),
                    );
                    ui.label(
                        egui::RichText::new("a Witcher 3 save editor")
                            .italics()
                            .color(WEAK),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let has_file = self.loaded.is_some();
                    if action_button(ui, "Save as…", has_file) {
                        self.save_as_dialog();
                    }
                    if action_button(ui, "Save", has_file) {
                        if let Some(path) = self.loaded.as_ref().map(|l| l.path.clone()) {
                            self.save_to(path);
                        }
                    }
                    if action_button(ui, "Open…", true) {
                        self.open_dialog();
                    }
                    if let Some(l) = &self.loaded {
                        if l.dirty {
                            ui.label(egui::RichText::new("● unsaved").color(RED).small());
                        }
                        ui.label(
                            egui::RichText::new(
                                l.path.file_name().unwrap_or_default().to_string_lossy(),
                            )
                            .color(WEAK)
                            .small(),
                        );
                    }
                });
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_space(78.0);
                if tab_item(ui, "Geralt", self.tab == Tab::Geralt) {
                    self.tab = Tab::Geralt;
                }
                ui.add_space(26.0);
                if tab_item(ui, "Inventories", self.tab == Tab::Inventories) {
                    self.tab = Tab::Inventories;
                }
                ui.add_space(26.0);
                if tab_item(ui, "World", self.tab == Tab::World) {
                    self.tab = Tab::World;
                }
            });
            ornament_divider(ui);
        });

        egui::Panel::bottom("status").show(root_ui, |ui| {
            ui.add_space(3.0);
            let (sev, msg) = &self.status;
            let color = match sev {
                Severity::Info => WEAK,
                Severity::Success => GREEN,
                Severity::Error => RED,
            };
            ui.horizontal(|ui| {
                ui.colored_label(color, msg);
                if let Some(l) = &self.loaded {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(format!(
                                "format v{}.{} · {} names",
                                l.save.versions[0],
                                l.save.versions[1],
                                l.save.names.len()
                            ))
                            .small()
                            .color(WEAK),
                        );
                    });
                }
            });
            ui.add_space(3.0);
        });

        egui::CentralPanel::default().show(root_ui, |ui| {
            if self.loaded.is_none() {
                self.empty_state(ui, time);
                return;
            }
            match self.tab {
                Tab::Geralt => self.geralt_tab(ui),
                Tab::Inventories => self.inventories_tab(ui),
                Tab::World => self.world_tab(ui),
            }
        });
    }
}

impl App {
    fn empty_state(&mut self, ui: &mut egui::Ui, time: f64) {
        let leftover = ui.available_height();
        ui.add_space((leftover * 0.22).max(12.0));
        ui.vertical_centered(|ui| {
            let (rect, _) =
                ui.allocate_exact_size(egui::vec2(140.0, 140.0), egui::Sense::hover());
            draw_medallion(ui.painter(), rect.center(), 62.0, time);
            ui.add_space(10.0);
            ui.label(
                egui::RichText::new("No save loaded")
                    .font(serif(20.0))
                    .color(TEXT),
            );
            ui.label(
                egui::RichText::new("Wind's howling. Open a .sav file to begin.")
                    .color(WEAK)
                    .italics(),
            );
            ui.add_space(12.0);
            let open = egui::Button::new(
                egui::RichText::new("Open save…")
                    .font(serif(16.0))
                    .color(TEXT_BRIGHT),
            )
            .fill(RED_DARK)
            .stroke(egui::Stroke::new(1.0, RED));
            if ui.add_sized([200.0, 38.0], open).clicked() {
                self.open_dialog();
            }
        });
    }

    fn geralt_tab(&mut self, ui: &mut egui::Ui) {
        let mut status: Option<(Severity, String)> = None;
        let Some(loaded) = &mut self.loaded else { return };
        let Some(pi) = loaded.player_inv else {
            ui.label("Could not identify Geralt's inventory in this save.");
            return;
        };

        // Bag sub-tabs with counts.
        let bags: Vec<Bag> = loaded.inventories[pi].items.iter().map(bag_of).collect();
        ui.add_space(2.0);
        ui.horizontal_wrapped(|ui| {
            for (bag, label) in BAGS {
                let count = if bag == Bag::All {
                    bags.len()
                } else {
                    bags.iter().filter(|b| **b == bag).count()
                };
                if bag_item(ui, label, count, self.bag == bag) {
                    self.bag = bag;
                }
                ui.add_space(8.0);
            }
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            column_title(ui, "Filter");
            ui.add(egui::TextEdit::singleline(&mut self.geralt_filter).desired_width(240.0));
        });
        ui.add_space(6.0);

        let filter = self.geralt_filter.to_lowercase();
        let inv = &mut loaded.inventories[pi];
        let rows: Vec<usize> = inv
            .items
            .iter()
            .enumerate()
            .filter(|(i, it)| {
                (self.bag == Bag::All || bags[*i] == self.bag)
                    && (filter.is_empty() || it.name.to_lowercase().contains(&filter))
            })
            .map(|(i, _)| i)
            .collect();

        let patches = item_table(ui, "geralt_items", inv, &rows);
        apply_patches(loaded, patches, &mut status);
        if let Some(s) = status {
            self.status = s;
        }
    }

    fn inventories_tab(&mut self, ui: &mut egui::Ui) {
        let mut status: Option<(Severity, String)> = None;
        let Some(loaded) = &mut self.loaded else { return };
        // Everything except Geralt's own inventory: horse, stashes,
        // merchants, containers, NPCs.
        let others: Vec<usize> = (0..loaded.inventories.len())
            .filter(|i| Some(*i) != loaded.player_inv)
            .collect();
        if others.is_empty() {
            ui.label("No other inventories found in this save.");
            return;
        }
        self.inv_selected = self.inv_selected.min(others.len() - 1);

        ui.add_space(2.0);
        ui.horizontal(|ui| {
            column_title(ui, "Inventory");
            let current = inv_title(
                &loaded.inventories[others[self.inv_selected]],
                self.inv_selected,
            );
            egui::ComboBox::from_id_salt("inv_select")
                .width(430.0)
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for (k, &i) in others.iter().enumerate() {
                        ui.selectable_value(
                            &mut self.inv_selected,
                            k,
                            inv_title(&loaded.inventories[i], k),
                        );
                    }
                });
            ui.separator();
            column_title(ui, "Filter");
            ui.add(egui::TextEdit::singleline(&mut self.inv_filter).desired_width(220.0));
        });
        ui.add_space(6.0);

        let filter = self.inv_filter.to_lowercase();
        let inv = &mut loaded.inventories[others[self.inv_selected]];
        let rows: Vec<usize> = inv
            .items
            .iter()
            .enumerate()
            .filter(|(_, it)| filter.is_empty() || it.name.to_lowercase().contains(&filter))
            .map(|(i, _)| i)
            .collect();

        let patches = item_table(ui, "npc_items", inv, &rows);
        apply_patches(loaded, patches, &mut status);
        if let Some(s) = status {
            self.status = s;
        }
    }

    fn world_tab(&mut self, ui: &mut egui::Ui) {
        let mut status: Option<(Severity, String)> = None;
        let Some(loaded) = &mut self.loaded else { return };
        if loaded.facts.is_empty() {
            ui.label("No facts database found (or it failed to parse) in this save.");
            return;
        }
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!(
                    "{} world facts & quest flags",
                    loaded.facts.len()
                ))
                .color(WEAK),
            );
            ui.separator();
            column_title(ui, "Filter");
            ui.add(egui::TextEdit::singleline(&mut self.fact_filter).desired_width(280.0));
        });
        ui.add_space(6.0);

        let filter = self.fact_filter.to_lowercase();
        // One row per fact entry; facts without entries get one read-only row.
        let rows: Vec<(usize, Option<usize>)> = loaded
            .facts
            .iter()
            .enumerate()
            .filter(|(_, f)| filter.is_empty() || f.name.to_lowercase().contains(&filter))
            .flat_map(|(i, f)| {
                if f.entries.is_empty() {
                    vec![(i, None)]
                } else {
                    (0..f.entries.len()).map(|e| (i, Some(e))).collect()
                }
            })
            .collect();

        let mut patches: Vec<(usize, Vec<u8>)> = Vec::new();
        TableBuilder::new(ui)
            .striped(true)
            .column(Column::remainder().at_least(420.0))
            .column(Column::exact(84.0))
            .column(Column::exact(120.0))
            .column(Column::exact(100.0))
            .header(24.0, |mut header| {
                for title in ["Fact", "Value", "Set at (game s)", "Expires"] {
                    header.col(|ui| column_title(ui, title));
                }
            })
            .body(|body| {
                body.rows(22.0, rows.len(), |mut row| {
                    let (fi, ei) = rows[row.index()];
                    let fact = &mut loaded.facts[fi];
                    row.col(|ui| {
                        if ei.map(|e| e > 0).unwrap_or(false) {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} [{}]",
                                    fact.name,
                                    ei.unwrap()
                                ))
                                .color(TEXT),
                            );
                        } else {
                            ui.label(egui::RichText::new(&fact.name).color(TEXT));
                        }
                    });
                    match ei {
                        Some(e) => {
                            let entry = &mut fact.entries[e];
                            row.col(|ui| {
                                let mut v = entry.value;
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut v)
                                            .range(i16::MIN..=i16::MAX),
                                    )
                                    .changed()
                                {
                                    entry.value = v;
                                    patches.push((entry.value_off, v.to_le_bytes().to_vec()));
                                }
                            });
                            row.col(|ui| {
                                ui.label(
                                    egui::RichText::new(format!("{:.0}", entry.time))
                                        .color(STEEL),
                                );
                            });
                            row.col(|ui| {
                                let (txt, color) = if entry.expiry < 0.0 {
                                    ("never".to_string(), WEAK)
                                } else {
                                    (format!("{:.0}", entry.expiry), STEEL)
                                };
                                ui.label(egui::RichText::new(txt).color(color));
                            });
                        }
                        None => {
                            row.col(|ui| {
                                ui.label(egui::RichText::new("—").color(WEAK));
                            });
                            row.col(|_| {});
                            row.col(|_| {});
                        }
                    }
                });
            });

        apply_patches(loaded, patches, &mut status);
        if let Some(s) = status {
            self.status = s;
        }
    }
}

/// Render the shared item table and collect any edits as byte patches.
fn item_table(
    ui: &mut egui::Ui,
    id_salt: &str,
    inv: &mut Inventory,
    rows: &[usize],
) -> Vec<(usize, Vec<u8>)> {
    let mut patches: Vec<(usize, Vec<u8>)> = Vec::new();
    ui.push_id(id_salt, |ui| {
        TableBuilder::new(ui)
            .striped(true)
            .column(Column::remainder().at_least(250.0))
            .column(Column::exact(92.0))
            .column(Column::exact(92.0))
            .column(Column::auto().at_least(150.0))
            .column(Column::remainder())
            .header(24.0, |mut header| {
                for title in ["Item", "Quantity", "Durability", "Enchant / Dye", "Modifiers"] {
                    header.col(|ui| column_title(ui, title));
                }
            })
            .body(|body| {
                body.rows(22.0, rows.len(), |mut row| {
                    let item = &mut inv.items[rows[row.index()]];
                    row.col(|ui| {
                        let color = if !item.enchantment.is_empty() {
                            GOLD
                        } else {
                            TEXT
                        };
                        ui.label(egui::RichText::new(&item.name).color(color));
                    });
                    row.col(|ui| {
                        let mut qty = item.quantity;
                        if ui
                            .add(egui::DragValue::new(&mut qty).range(0..=u16::MAX))
                            .changed()
                        {
                            item.quantity = qty;
                            patches.push((item.quantity_off, qty.to_le_bytes().to_vec()));
                        }
                    });
                    row.col(|ui| {
                        if item.durability >= 0.0 {
                            let mut dur = item.durability;
                            if ui
                                .add(
                                    egui::DragValue::new(&mut dur)
                                        .range(0.0..=100.0)
                                        .speed(1.0),
                                )
                                .changed()
                            {
                                item.durability = dur;
                                patches
                                    .push((item.durability_off, dur.to_le_bytes().to_vec()));
                            }
                        } else {
                            ui.label(egui::RichText::new("—").color(WEAK));
                        }
                    });
                    row.col(|ui| {
                        let mut parts = Vec::new();
                        if !item.enchantment.is_empty() {
                            parts.push(item.enchantment[0].clone());
                        }
                        if let Some((d, _)) = &item.dye {
                            parts.push(d.clone());
                        }
                        ui.label(egui::RichText::new(parts.join(", ")).color(STEEL));
                    });
                    row.col(|ui| {
                        ui.label(
                            egui::RichText::new(mods_summary(&item.mods))
                                .small()
                                .color(WEAK),
                        );
                    });
                });
            });
    });
    patches
}

fn apply_patches(
    loaded: &mut Loaded,
    patches: Vec<(usize, Vec<u8>)>,
    status: &mut Option<(Severity, String)>,
) {
    for (off, bytes) in patches {
        match loaded.save.patch(off, &bytes) {
            Ok(()) => loaded.dirty = true,
            Err(e) => *status = Some((Severity::Error, format!("Edit failed: {e}"))),
        }
    }
}

/// Collapse repeated modifier names into "name ×n".
fn mods_summary(mods: &[(String, u32)]) -> String {
    let mut counts: BTreeMap<&str, (usize, u32)> = BTreeMap::new();
    for (name, value) in mods {
        let e = counts.entry(name).or_insert((0, *value));
        e.0 += 1;
    }
    counts
        .iter()
        .map(|(name, (n, value))| {
            let base = if *value == 1 {
                (*name).to_string()
            } else {
                format!("{name}={value}")
            };
            if *n > 1 {
                format!("{base} ×{n}")
            } else {
                base
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn inv_title(inv: &Inventory, index: usize) -> String {
    let label = if inv.label.is_empty() {
        format!("inventory #{index}")
    } else {
        inv.label.clone()
    };
    format!("{label} ({} items)", inv.items.len())
}
