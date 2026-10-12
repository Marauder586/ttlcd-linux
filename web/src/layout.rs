//! The tabbed layout. The sidebar lists the display-mode categories and the
//! tool tabs; only the cards belonging to the open tab are shown. On a mode
//! tab that is the mode grid plus the settings of whichever mode is selected.

use std::cell::RefCell;

use crate::dom::{self, El};
use crate::modes;
use crate::util::{card_visible, mode_label};

const TAB_KEY: &str = "ttlcd-tab";
/// The tab that shows the mode grid; every category button opens it.
const MODES_TAB: &str = "modes";
/// Where a card goes if `CARDS` does not list it.
const FALLBACK_TAB: &str = "status";

/// Where a card appears.
enum Place {
    /// On one of the tool tabs, by its `data-tab` name.
    Tab(&'static str),
    /// On the modes tab, as the settings of these display modes.
    Settings(&'static [&'static str]),
}

use Place::{Settings, Tab};

/// Every card, identified by any element inside it, and where it appears.
const CARDS: [(&str, Place); 47] = [
    // Settings of individual display modes.
    ("#cd-hours", Settings(&["countdown"])),
    ("#sw-display", Settings(&["stopwatch"])),
    ("#progress-section", Settings(&["progress"])),
    ("#met-title", Settings(&["metrics"])),
    ("#metrics-source-section", Settings(&["metrics"])),
    ("#weather-city", Settings(&["weather"])),
    ("#notes-title-input", Settings(&["notes"])),
    ("#qr-section", Settings(&["qr_code"])),
    ("#pomodoro-section", Settings(&["pomodoro"])),
    ("#world-clock-section", Settings(&["world_clock"])),
    ("#ticker-section", Settings(&["ticker"])),
    ("#http-status-section", Settings(&["http_status"])),
    ("#shell-cmd-section", Settings(&["shell_cmd"])),
    ("#ping-section", Settings(&["ping_monitor", "ping_graph"])),
    ("#log-tail-section", Settings(&["log_tail"])),
    ("#rss-section", Settings(&["rss_feed"])),
    ("#service-section", Settings(&["service_status"])),
    ("#docker-section", Settings(&["docker_status"])),
    ("#sessions-section", Settings(&["logged_in_users"])),
    ("#pet-section", Settings(&["pet_cat"])),
    // Content pushed to the panel on demand.
    ("#quick-status", Tab("push")),
    ("#txt-title", Tab("push")),
    ("#drop-zone", Tab("push")),
    ("#img-url-input", Tab("push")),
    ("#ss-dir", Tab("push")),
    ("#ban-line1", Tab("push")),
    ("#flash-section", Tab("push")),
    ("#sched-btn", Tab("push")),
    // Things that change the display on a timer.
    ("#rot-modes", Tab("automation")),
    ("#mode-schedule-section", Tab("automation")),
    ("#dim-start", Tab("automation")),
    ("#night-mode-section", Tab("automation")),
    ("#screensaver-section", Tab("automation")),
    // The panel itself and its alerts.
    ("#set-quality", Tab("panel")),
    ("#lights-section", Tab("panel")),
    ("#thresholds-section", Tab("panel")),
    ("#prof-name", Tab("panel")),
    ("#settings-export-btn", Tab("panel")),
    // Connections to other services.
    ("#wh-section", Tab("integrations")),
    ("#ntfy-section", Tab("integrations")),
    ("#auth-section", Tab("integrations")),
    // Read-only information.
    ("#stat-cpu", Tab("status")),
    ("#info-version", Tab("status")),
    ("#history-list", Tab("status")),
    (".shortcuts-card", Tab("status")),
    (".log-card", Tab("status")),
    // The mode grid is handled separately; listed so it is not a fallback.
    ("#modes-section", Tab(MODES_TAB)),
];

thread_local! {
    static TAB: RefCell<String> = const { RefCell::new(String::new()) };
    /// The mode whose settings are shown on the modes tab.
    static SELECTED: RefCell<String> = const { RefCell::new(String::new()) };
}

/// Sort the cards into tabs. Call once every card has been created.
pub fn init() {
    for (selector, place) in &CARDS {
        let card = El::q(selector).closest(".card");
        match place {
            Tab(tab) => card.set_data("tab", tab),
            Settings(modes) => card.set_data("modes", &modes.join(" ")),
        };
    }
    for card in El::all("main > .card") {
        if card.data("tab").is_empty() && card.data("modes").is_empty() {
            card.set_data("tab", FALLBACK_TAB);
        }
    }

    let note = El::new("p");
    note.set_id("mode-settings-note")
        .set_class("mode-settings-note");
    El::id("modes-section").insert_after(&note);

    for btn in El::all(".tab-btn[data-tab]") {
        let tab = btn.data("tab");
        btn.on("click", move |_| open(&tab));
    }

    let saved = dom::storage_get(TAB_KEY).unwrap_or_default();
    let known = El::all(".tab-btn[data-tab]")
        .iter()
        .any(|b| b.data("tab") == saved);
    open(if known { &saved } else { MODES_TAB });
}

/// Open the mode grid; the category buttons call this.
pub fn show_modes() {
    open(MODES_TAB);
}

/// Show the settings of `mode` on the modes tab.
pub fn select_mode(mode: &str) {
    SELECTED.with(|s| *s.borrow_mut() = mode.to_string());
    apply();
}

/// Called whenever the panel's mode is reported. It only picks the initial
/// selection: later changes may come from auto-rotation, and should not
/// swap the settings out from under the user.
pub fn on_active_mode(mode: &str) {
    if SELECTED.with(|s| s.borrow().is_empty()) {
        select_mode(mode);
    }
}

fn open(tab: &str) {
    TAB.with(|t| *t.borrow_mut() = tab.to_string());
    dom::storage_set(TAB_KEY, tab);
    apply();
}

fn apply() {
    let tab = TAB.with(|t| t.borrow().clone());
    if tab.is_empty() {
        // A mode was reported before `init` ran.
        return;
    }
    let selected = SELECTED.with(|s| s.borrow().clone());
    let on_modes = tab == MODES_TAB;

    let mut settings_shown = 0;
    for card in El::all("main > .card") {
        if card.attr("id") == "modes-section" {
            card.class("tab-hidden", !on_modes);
            continue;
        }
        let visible = card_visible(
            &tab,
            MODES_TAB,
            &selected,
            &card.data("tab"),
            &card.data("modes"),
        );
        if visible && on_modes {
            settings_shown += 1;
            // Some settings cards are collapsed <details>. Open one as it
            // appears, since showing the settings is the point of selecting
            // the mode; after that the user may collapse it again.
            if card.has_class("tab-hidden") {
                card.set("open", true);
            }
        }
        card.class("tab-hidden", !visible);
    }

    let note = if !on_modes || selected.is_empty() {
        String::new()
    } else if settings_shown == 0 {
        format!("{} has no settings.", mode_label(&selected))
    } else {
        format!("Settings for {}", mode_label(&selected))
    };
    El::id("mode-settings-note")
        .set_text(&note)
        .display(if note.is_empty() { "none" } else { "" });

    for btn in El::all(".tab-btn[data-tab]") {
        btn.class("active", btn.data("tab") == tab);
    }
    if on_modes {
        // Re-marks the active category button.
        modes::apply_filters();
    } else {
        for btn in El::all(".cat-btn") {
            btn.class("active", false);
        }
    }
}
