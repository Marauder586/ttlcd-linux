//! The mode grid: switching, search, categories, favourites, recently used
//! modes, hover previews, the command palette and the rotation picker.

use std::cell::RefCell;

use crate::core::{self, log};
use crate::dom::{self, El};
use crate::util::{esc, mode_label, mode_matches, plural, push_recent, split_list};

const FAVORITES_KEY: &str = "ttlcd_favorites";
/// Favourites saved by an older version of the UI; merged in on load.
const LEGACY_FAVORITES_KEY: &str = "ttlcd-fav-modes";
const RECENT_KEY: &str = "ttlcd_recent_modes";
const CATEGORY_KEY: &str = "ttlcd-active-cat";
const ROTATION_KEY: &str = "ttlcd-rot-modes";

const MAX_RECENT: usize = 8;
/// How many recent modes fit in the band above the grid.
const RECENT_BAND_LEN: usize = 6;

const GRID_BUTTONS: &str = "#mode-grid .mode-btn[data-mode]";

thread_local! {
    static FAVORITES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static CATEGORY: RefCell<String> = const { RefCell::new(String::new()) };
    static PALETTE: RefCell<Palette> = const { RefCell::new(Palette { modes: Vec::new(), selected: 0 }) };
    static ROTATION: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

pub fn init() {
    init_switching();
    init_search();
    init_category_badges();
    init_rotation_picker();
    init_favorites();
    init_categories();
    init_recent();
    init_preview_popup();
    init_palette();
}

/// A grid button's emoji and label.
fn parts(btn: &El) -> (String, String) {
    let label = btn.find("span:not(.fav-star)").text();
    (
        btn.first_text(),
        if label.is_empty() {
            btn.data("mode")
        } else {
            label
        },
    )
}

fn grid_button(mode: &str) -> Option<El> {
    El::all(GRID_BUTTONS)
        .into_iter()
        .find(|b| b.data("mode") == mode)
}

/// A copy of a grid button for the bands above the grid.
fn band_button(mode: &str, class: &str) -> Option<El> {
    let orig = grid_button(mode)?;
    let (emoji, label) = parts(&orig);
    let btn = El::build(
        "button",
        &format!("mode-btn {class}"),
        &format!("{}<span>{}</span>", esc(&emoji), esc(&label)),
    );
    let title = orig.title();
    btn.set_data("mode", mode)
        .set_title(if title.is_empty() { mode } else { &title });
    btn.class("active", orig.has_class("active"));
    Some(btn)
}

/// Create a band above the grid, right after `anchor`.
fn band(id: &str, class: &str, anchor: &El) -> El {
    let band = El::new("div");
    band.set_id(id).set_class(class);
    if !anchor.insert_after(&band) {
        El::id("mode-grid").insert_before(&band);
    }
    band
}

// ─── Switching ───────────────────────────────────────────────────────────────

fn init_switching() {
    // One listener covers the grid and the bands, whose buttons come and go.
    dom::doc_on("click", |e| {
        let btn = dom::target(&e).closest(".mode-btn[data-mode]");
        if btn.ok() {
            let mode = btn.data("mode");
            remember(&mode);
            dom::spawn(async move { core::switch_mode(&mode).await });
        }
    });
}

// ─── Search and categories ───────────────────────────────────────────────────

/// Show the grid buttons in the chosen category, or, while a search is
/// typed, the matches from every category.
pub fn apply_filters() {
    let query = El::id("mode-search").trimmed().to_lowercase();
    let category = CATEGORY.with(|c| c.borrow().clone());
    let favorites = FAVORITES.with(|f| f.borrow().clone());

    let buttons = El::all(GRID_BUTTONS);
    let mut visible = 0;
    for btn in &buttons {
        let mode = btn.data("mode");
        let in_search = mode_matches(&query, &mode, &parts(btn).1, &btn.title());
        // A search looks through every category.
        let in_category = !query.is_empty()
            || match category.as_str() {
                "all" => true,
                "favorites" => favorites.contains(&mode),
                cat => btn.data("cat") == cat,
            };
        btn.class("hidden-by-filter", !in_search)
            .class("hidden-by-cat", !in_category);
        if in_search && in_category {
            visible += 1;
        }
    }
    for btn in El::all(".cat-btn[data-cat]") {
        btn.class("active", btn.data("cat") == category);
    }

    let count = if !query.is_empty() {
        format!("{visible} of {} modes shown", buttons.len())
    } else if category == "favorites" {
        format!("{} favorite{}", favorites.len(), plural(favorites.len()))
    } else if category != "all" {
        format!("Showing {visible} mode{} in {category}", plural(visible))
    } else {
        String::new()
    };
    El::id("mode-filter-count").set_text(&count);
}

fn init_search() {
    let input = El::id("mode-search");
    input.on("input", |_| apply_filters());
    input.on("keydown", |e| {
        if dom::key(&e) == "Escape" {
            let input = El::id("mode-search");
            input.set_val("");
            apply_filters();
            input.call("blur");
        }
    });
    dom::doc_on("keydown", |e| {
        if dom::key(&e) == "/" && !dom::typing() {
            e.prevent_default();
            let input = El::id("mode-search");
            input.focus();
            input.call("select");
        }
    });
}

/// Put the number of modes on each category button.
fn init_category_badges() {
    let buttons = El::all(GRID_BUTTONS);
    for cat_btn in El::all(".cat-btn[data-cat]") {
        let cat = cat_btn.data("cat");
        let count = buttons
            .iter()
            .filter(|b| cat == "all" || b.data("cat") == cat)
            .count();
        if count > 0 {
            let badge = El::new("sup");
            badge
                .set_class("cat-count-badge")
                .set_text(&count.to_string());
            cat_btn.append(&badge);
        }
    }
}

fn init_categories() {
    let bar = El::id("mode-categories");
    let favorites = El::new("button");
    favorites
        .set_class("cat-btn")
        .set_data("cat", "favorites")
        .set_text("⭐ Favorites");
    // Second in the bar, right after "All".
    if !bar.find(":scope > :first-child").insert_after(&favorites) {
        bar.append(&favorites);
    }

    let saved = dom::storage_get(CATEGORY_KEY).filter(|c| !c.is_empty());
    CATEGORY.with(|c| *c.borrow_mut() = saved.unwrap_or_else(|| "all".into()));

    for btn in El::all(".cat-btn[data-cat]") {
        let cat = btn.data("cat");
        btn.on("click", move |_| {
            CATEGORY.with(|c| *c.borrow_mut() = cat.clone());
            dom::storage_set(CATEGORY_KEY, &cat);
            crate::layout::show_modes();
        });
    }
    apply_filters();
}

// ─── Favourites ──────────────────────────────────────────────────────────────

fn is_favorite(mode: &str) -> bool {
    FAVORITES.with(|f| f.borrow().iter().any(|m| m == mode))
}

fn set_star(star: &El, on: bool) {
    star.set_text(if on { "★" } else { "☆" }).set_title(if on {
        "Remove from favorites"
    } else {
        "Add to favorites"
    });
}

fn init_favorites() {
    let mut favorites = dom::storage_list(FAVORITES_KEY);
    for mode in dom::storage_list(LEGACY_FAVORITES_KEY) {
        if !favorites.contains(&mode) {
            favorites.push(mode);
        }
    }
    FAVORITES.with(|f| *f.borrow_mut() = favorites);

    for btn in El::all(GRID_BUTTONS) {
        let mode = btn.data("mode");
        let star = El::new("span");
        star.set_class("fav-star")
            .set_attr("role", "button")
            .set_attr("aria-label", &format!("Favourite {mode}"));
        set_star(&star, is_favorite(&mode));
        let handle = star.clone();
        star.on("click", move |e| {
            // Starring a mode must not also switch to it.
            e.stop_propagation();
            let now_on = FAVORITES.with(|f| {
                let mut f = f.borrow_mut();
                let was_on = f.contains(&mode);
                if was_on {
                    f.retain(|m| *m != mode);
                } else {
                    f.push(mode.clone());
                }
                dom::storage_set_list(FAVORITES_KEY, &f);
                !was_on
            });
            set_star(&handle, now_on);
            render_favorites_band();
            apply_filters();
        });
        btn.append(&star);
    }

    let header = El::id("mode-grid")
        .closest(".card")
        .find(".section-header-row");
    band("fav-modes-band", "fav-band", &header);
    render_favorites_band();
}

fn render_favorites_band() {
    let band = El::id("fav-modes-band");
    let favorites = FAVORITES.with(|f| f.borrow().clone());
    band.set_html(r#"<span class="fav-band-label">★ Pinned:</span>"#);
    let mut shown = 0;
    for btn in favorites
        .iter()
        .filter_map(|mode| band_button(mode, "fav-btn"))
    {
        band.append(&btn);
        shown += 1;
    }
    band.display(if shown > 0 { "flex" } else { "none" });
}

// ─── Recently used ───────────────────────────────────────────────────────────

/// Record a mode as most recently used and show its settings.
pub fn remember(mode: &str) {
    let mut recent = dom::storage_list(RECENT_KEY);
    push_recent(&mut recent, mode, MAX_RECENT);
    dom::storage_set_list(RECENT_KEY, &recent);
    render_recent();
    crate::layout::select_mode(mode);
}

fn init_recent() {
    band(
        "recent-modes-band",
        "recent-band",
        &El::id("fav-modes-band"),
    );
    render_recent();
}

fn render_recent() {
    let recent = dom::storage_list(RECENT_KEY);

    // The band of buttons above the grid.
    let band = El::id("recent-modes-band");
    band.set_html(r#"<span class="recent-band-label">⏱ Recent:</span>"#);
    for btn in recent
        .iter()
        .take(RECENT_BAND_LEN)
        .filter_map(|mode| band_button(mode, "recent-btn"))
    {
        band.append(&btn);
    }
    let clear = El::new("button");
    clear
        .set_class("recent-clear-btn")
        .set_title("Clear recently used list")
        .set_text("✕");
    clear.on("click", |e| {
        e.stop_propagation();
        dom::storage_set_list(RECENT_KEY, &[]);
        render_recent();
    });
    band.append(&clear);
    band.display(if recent.is_empty() { "none" } else { "flex" });

    // The row of chips written in index.html.
    let bar = El::id("recent-modes-bar");
    bar.clear();
    if recent.is_empty() {
        bar.set_text("No recent modes yet");
    }
    for mode in recent {
        let chip = El::new("button");
        chip.set_class("recent-chip")
            .set_text(&mode_label(&mode))
            .set_title(&format!("Switch to {mode}"));
        chip.on("click", move |_| {
            let mode = mode.clone();
            remember(&mode);
            dom::spawn(async move { core::go_to_mode(&mode).await });
        });
        bar.append(&chip);
    }
}

// ─── Hover preview ───────────────────────────────────────────────────────────

/// Milliseconds the pointer must rest on a button before the preview shows.
const POPUP_DELAY_MS: i32 = 250;
/// Popup size; the width must match `.mode-preview-popup` in app.css.
const POPUP_W: f64 = 196.0;
const POPUP_H: f64 = 225.0;
const POPUP_MARGIN: f64 = 8.0;

thread_local! {
    static POPUP_TIMER: RefCell<Option<i32>> = const { RefCell::new(None) };
}

fn cancel_popup_timer() {
    if let Some(id) = POPUP_TIMER.with(|t| t.borrow_mut().take()) {
        dom::clear_timeout(id);
    }
}

fn init_preview_popup() {
    let popup = El::build(
        "div",
        "mode-preview-popup",
        r#"<img class="mpop-img" src="" alt="" /><div class="mpop-name"></div><div class="mpop-cat"></div><div class="mpop-caret"></div>"#,
    );
    El::body().append(&popup);
    let img = popup.find(".mpop-img");
    let loaded = img.clone();
    img.on("load", move |_| {
        loaded.class("loaded", true);
    });

    for btn in El::all(GRID_BUTTONS) {
        let (target, popup_show) = (btn.clone(), popup.clone());
        btn.on("mouseenter", move |_| {
            cancel_popup_timer();
            let (target, popup) = (target.clone(), popup_show.clone());
            let id = dom::set_timeout(POPUP_DELAY_MS, move || show_popup(&popup, &target));
            POPUP_TIMER.with(|t| *t.borrow_mut() = Some(id));
        });
        // Hide on click as well, so the popup does not linger over the grid.
        for event in ["mouseleave", "mousedown"] {
            let popup = popup.clone();
            btn.on(event, move |_| {
                cancel_popup_timer();
                popup.class("visible", false);
            });
        }
    }
}

fn show_popup(popup: &El, btn: &El) {
    let mode = btn.data("mode");
    popup.find(".mpop-name").set_text(&mode_label(&mode));
    popup
        .find(".mpop-cat")
        .set_text(&mode_label(&btn.data("cat")));

    let img = popup.find(".mpop-img");
    let url = format!("/api/render/{}", crate::api::enc(&mode));
    if img.attr("src") != url {
        img.class("loaded", false).set_attr("src", &url);
    }

    // Centre above the button; flip below if there is no room.
    let (left, top, width, height) = btn.rect();
    let viewport_w = dom::win()
        .inner_width()
        .ok()
        .and_then(|w| w.as_f64())
        .unwrap_or(1024.0);
    let centre = left + width / 2.0;
    let x = (centre - POPUP_W / 2.0)
        .min(viewport_w - POPUP_W - POPUP_MARGIN)
        .max(POPUP_MARGIN);
    let above = top - POPUP_H - 10.0;
    let flip = above < POPUP_MARGIN;
    let y = if flip { top + height + 10.0 } else { above };
    popup
        .css("left", &format!("{x}px"))
        .css("top", &format!("{y}px"))
        .class("flip-down", flip);
    popup.find(".mpop-caret").css(
        "left",
        &format!("{}px", (centre - x).clamp(14.0, POPUP_W - 14.0)),
    );
    popup.class("visible", true);
}

// ─── Command palette ─────────────────────────────────────────────────────────

struct Palette {
    /// Modes currently listed, in display order.
    modes: Vec<String>,
    selected: usize,
}

fn init_palette() {
    let overlay = El::build(
        "div",
        "",
        r#"
    <div id="cmd-palette-box" style="
      background:var(--surface0);border:1px solid var(--overlay0);
      border-radius:12px;width:min(560px,92vw);max-height:70vh;
      overflow:hidden;display:flex;flex-direction:column;
      box-shadow:0 8px 40px rgba(0,0,0,0.6)">
      <div style="padding:12px 16px;border-bottom:1px solid var(--surface1);display:flex;align-items:center;gap:8px">
        <span style="color:var(--overlay0);font-size:1.1rem">⌕</span>
        <input id="cmd-palette-input" type="text" placeholder="Switch mode…"
          autocomplete="off" autocorrect="off" spellcheck="false"
          style="flex:1;background:transparent;border:none;outline:none;
                 color:var(--text);font-size:1rem;font-family:var(--font)">
        <kbd style="font-size:0.75rem;color:var(--overlay0)">Esc</kbd>
      </div>
      <div id="cmd-palette-list" style="
        overflow-y:auto;max-height:calc(70vh - 56px);padding:6px 0">
      </div>
    </div>"#,
    );
    overlay.set_id("cmd-palette-overlay").css_text(
        "position:fixed;top:0;left:0;width:100%;height:100%;background:rgba(0,0,0,0.7);z-index:10000;display:none;align-items:flex-start;justify-content:center;padding-top:80px",
    );
    El::body().append(&overlay);

    let input = El::id("cmd-palette-input");
    input.on("input", |_| render_palette());
    input.on("keydown", |e| match dom::key(&e).as_str() {
        "ArrowDown" => {
            e.prevent_default();
            move_palette_selection(1);
        }
        "ArrowUp" => {
            e.prevent_default();
            move_palette_selection(-1);
        }
        "Enter" => {
            e.prevent_default();
            let chosen = PALETTE.with(|p| {
                let p = p.borrow();
                p.modes.get(p.selected).cloned()
            });
            if let Some(mode) = chosen {
                activate_palette(mode);
            }
        }
        "Escape" => close_palette(),
        _ => {}
    });

    overlay.on("click", |e| {
        if !El::id("cmd-palette-box").contains(&dom::target(&e)) {
            close_palette();
        }
    });

    dom::doc_on("keydown", |e| {
        if dom::key(&e) == "k" && (dom::flag(&e, "ctrlKey") || dom::flag(&e, "metaKey")) {
            e.prevent_default();
            if El::id("cmd-palette-overlay").is_hidden() {
                open_palette();
            } else {
                close_palette();
            }
        }
    });
}

fn open_palette() {
    El::id("cmd-palette-overlay").display("flex");
    El::id("cmd-palette-input").set_val("");
    render_palette();
    dom::next_frame(|| El::id("cmd-palette-input").focus());
}

fn close_palette() {
    El::id("cmd-palette-overlay").hide();
    El::id("cmd-palette-input").set_val("");
}

fn activate_palette(mode: String) {
    close_palette();
    remember(&mode);
    dom::spawn(async move {
        core::go_to_mode(&mode).await;
        log(&format!("Switched to {mode} via command palette"));
    });
}

fn render_palette() {
    let query = El::id("cmd-palette-input").val().to_lowercase();
    let list = El::id("cmd-palette-list");
    list.clear();

    let mut modes = Vec::new();
    for btn in El::all(GRID_BUTTONS) {
        let mode = btn.data("mode");
        let (emoji, label) = parts(&btn);
        if !mode_matches(&query, &mode, &label, &btn.title()) {
            continue;
        }
        let idx = modes.len();
        let row = El::build(
            "div",
            "cmd-palette-row",
            &format!(
                r#"
        <span style="font-size:1.3rem;width:24px;text-align:center">{}</span>
        <span style="flex:1">
          <span style="color:var(--text);font-size:0.95rem">{}</span>
          <span style="display:block;color:var(--overlay0);font-size:0.73rem;margin-top:1px">{}</span>
        </span>
        <span style="color:var(--overlay0);font-size:0.73rem">Enter</span>"#,
                esc(if emoji.is_empty() { "▶" } else { &emoji }),
                esc(&label),
                esc(&mode_label(&mode)),
            ),
        );
        row.css_text("padding:10px 16px;cursor:pointer;display:flex;align-items:center;gap:12px;transition:background 0.1s");
        row.on("mouseenter", move |_| select_palette_row(idx));
        let chosen = mode.clone();
        row.on("click", move |_| activate_palette(chosen.clone()));
        list.append(&row);
        modes.push(mode);
    }

    if modes.is_empty() {
        list.set_html(r#"<div style="padding:20px;text-align:center;color:var(--overlay0)">No modes match</div>"#);
    }
    PALETTE.with(|p| *p.borrow_mut() = Palette { modes, selected: 0 });
    select_palette_row(0);
}

fn move_palette_selection(delta: isize) {
    let target = PALETTE.with(|p| p.borrow().selected.saturating_add_signed(delta));
    select_palette_row(target);
}

/// Highlight a row (clamped to the list) and keep it in view.
fn select_palette_row(idx: usize) {
    let selected = PALETTE.with(|p| {
        let mut p = p.borrow_mut();
        p.selected = idx.min(p.modes.len().saturating_sub(1));
        p.selected
    });
    for (i, row) in El::all(".cmd-palette-row").iter().enumerate() {
        row.css(
            "background",
            if i == selected { "var(--surface1)" } else { "" },
        );
        if i == selected {
            row.scroll_into_view();
        }
    }
}

// ─── Rotation picker ─────────────────────────────────────────────────────────

/// Toggle buttons for choosing which modes auto-rotation cycles through,
/// kept in step with the comma-separated `rot-modes` input.
fn init_rotation_picker() {
    let picker = El::id("rot-mode-picker");
    let input = El::id("rot-modes");
    if !picker.ok() || !input.ok() {
        return;
    }

    for btn in El::all(GRID_BUTTONS) {
        let mode = btn.data("mode");
        if mode == "idle" {
            continue;
        }
        let (emoji, label) = parts(&btn);
        let toggle = El::build(
            "button",
            "rot-pick-btn",
            &format!(
                r#"<span class="rpb-emoji">{}</span><span class="rpb-label">{}</span>"#,
                esc(&emoji),
                esc(&label)
            ),
        );
        toggle
            .set_data("mode", &mode)
            .set_title(&format!("Toggle {mode} in rotation"));
        toggle.on("click", move |_| {
            ROTATION.with(|r| {
                let mut r = r.borrow_mut();
                if r.contains(&mode) {
                    r.retain(|m| *m != mode);
                } else {
                    r.push(mode.clone());
                }
            });
            sync_rotation_picker();
        });
        picker.append(&toggle);
    }

    let read_input = || {
        let mut modes = split_list(&El::id("rot-modes").val(), false);
        let mut seen = Vec::new();
        modes.retain(|m| {
            !seen.contains(m) && {
                seen.push(m.clone());
                true
            }
        });
        ROTATION.with(|r| *r.borrow_mut() = modes);
        sync_rotation_picker();
    };
    input.on("change", move |_| read_input());
    // Pick up anything typed before Start is pressed.
    El::id("rot-start-btn").on("click", move |_| read_input());

    if input.val().is_empty() {
        ROTATION.with(|r| *r.borrow_mut() = dom::storage_list(ROTATION_KEY));
        sync_rotation_picker();
    } else {
        read_input();
    }
}

fn sync_rotation_picker() {
    let selected = ROTATION.with(|r| r.borrow().clone());
    for btn in El::id("rot-mode-picker").find_all(".rot-pick-btn") {
        btn.class("rot-selected", selected.contains(&btn.data("mode")));
    }
    El::id("rot-modes").set_val(&selected.join(", "));
    El::id("rot-selected-list").set_text(&selected.join(" → "));
    El::id("rot-selected-preview").display(if selected.is_empty() { "none" } else { "" });
    dom::storage_set_list(ROTATION_KEY, &selected);
}
