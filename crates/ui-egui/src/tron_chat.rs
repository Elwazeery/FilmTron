//! FilmTron native floating TRON panel (shared GemiMCP workstation model).
//!
//! The launcher is passive and uses zero inference/VRAM. No dummy AI replies:
//! editing chat is visibly disabled until FilmTron's specialist native plan
//! pipeline is connected to the central runtime. PhotoTron's fixed poster
//! prompts/commands must NEVER be used for video timeline edits.
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{Align2, Color32, Context, Id, Order, RichText, ScrollArea, vec2};

#[derive(Clone, Debug)]
struct ViewState {
    open: bool,
    working: bool,
    power_on: bool,
    shared_state: String,
    model: String,
    transport_connected: bool,
    thinking_verified: bool,
    warmup_verified: bool,
    active_leases: u64,
    error: Option<String>,
    input: String,
    last_status_poll: Instant,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            open: false,
            working: false,
            power_on: false,
            shared_state: "NOT_LOADED".to_owned(),
            model: "qwen3.5:4b-q4_K_M".to_owned(),
            transport_connected: false,
            thinking_verified: false,
            warmup_verified: false,
            active_leases: 0,
            error: None,
            input: String::new(),
            last_status_poll: Instant::now(),
        }
    }
}

fn view(ctx: &Context) -> Arc<Mutex<ViewState>> {
    let id = Id::new("filmtron.tron.shared.view");
    ctx.data_mut(|data| {
        if let Some(st) = data.get_temp::<Arc<Mutex<ViewState>>>(id) {
            st
        } else {
            let st = Arc::new(Mutex::new(ViewState::default()));
            data.insert_temp(id, st.clone());
            st
        }
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_status(g: &mut ViewState, response: &serde_json::Value) {
    let status = response.get("status").unwrap_or(response);
    g.shared_state = status.get("state").and_then(serde_json::Value::as_str)
        .unwrap_or("UNKNOWN").to_owned();
    g.model = status.get("model").and_then(serde_json::Value::as_str)
        .unwrap_or("qwen3.5:4b-q4_K_M").to_owned();
    g.thinking_verified = status.get("thinkingVerified").and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    g.warmup_verified = status.get("warmupVerified").and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    g.active_leases = status.get("activeLeaseCount").and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    g.transport_connected = true;
}

/// An actual broker call runs off the UI render thread. Clicking ON is explicit
/// local consent; passive polling and the launcher NEVER call /tron/acquire.
#[cfg(not(target_arch = "wasm32"))]
fn broker_action(st: Arc<Mutex<ViewState>>, ctx: Context, action: &'static str) {
    if let Ok(mut g) = st.lock() {
        if g.working { return; }
        g.working = true;
        g.error = None;
    } else { return; }
    std::thread::spawn(move || {
        let answer = match action {
            "on" => filmcraft_automation::tron_local::activate(),
            "off" => filmcraft_automation::tron_local::release(),
            "minimize" => filmcraft_automation::tron_local::minimize(),
            _ => filmcraft_automation::tron_local::status(),
        };
        if let Ok(mut g) = st.lock() {
            g.working = false;
            g.last_status_poll = Instant::now();
            match answer {
                Ok(result) => {
                    parse_status(&mut g, &result);
                    if action == "on" {
                        g.power_on = g.shared_state == "READY"
                            && g.thinking_verified && g.warmup_verified;
                        if !g.power_on {
                            g.error = Some("Actual model reasoning/warm-up not verified".into());
                        }
                    }
                    if action == "off" || action == "minimize" { g.power_on = false; }
                }
                Err(e) => {
                    g.error = Some(e);
                    if action == "on" { g.power_on = false; }
                    if action == "status" { g.transport_connected = false; }
                }
            }
        }
        ctx.request_repaint();
    });
}

/// Passive launcher + native overlay. Does not modify FilmcraftApp playback,
/// audio, media or timeline UI state.
pub fn show(ctx: &Context) {
    let st = view(ctx);
    let is_open = st.lock().map(|g| g.open).unwrap_or(false);
    if !is_open {
        egui::Area::new(Id::new("filmtron.tron.launcher"))
            .order(Order::Foreground)
            .anchor(Align2::RIGHT_BOTTOM, vec2(-22.0, -28.0))
            .show(ctx, |ui| {
                if ui.add(egui::Button::new(RichText::new("TRON · Creative AI").strong())
                    .min_size(vec2(146.0, 42.0))).clicked()
                {
                    if let Ok(mut g) = st.lock() { g.open = true; }
                }
            });
        return;
    }
    let needs_poll = st.lock().map(|g| {
        !g.working && g.last_status_poll.elapsed() > Duration::from_secs(12)
    }).unwrap_or(false);
    #[cfg(not(target_arch = "wasm32"))]
    if needs_poll {
        broker_action(st.clone(), ctx.clone(), "status");
    }
    #[cfg(target_arch = "wasm32")]
    let _ = needs_poll;

    let area = ctx.content_rect();
    let x = (area.right() - 470.0).max(area.left() + 8.0);
    let mut requested_action: Option<&'static str> = None;
    egui::Window::new("TRON · Film Creative Director")
        .id(Id::new("filmtron.tron.window"))
        .order(Order::Foreground)
        .default_pos(egui::pos2(x, area.top() + 65.0))
        .default_size(vec2(435.0, 490.0))
        .min_size(vec2(320.0, 280.0))
        .resizable(true)
        .collapsible(false)
        .show(ctx, |ui| {
            if let Ok(mut g) = st.lock() {
                ui.horizontal(|ui| {
                    ui.heading("TRON");
                    ui.weak("FilmTron · Editorial Director");
                    if ui.small_button("—").on_hover_text("تصغير").clicked() {
                        g.open = false;
                    }
                });
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    let connected = g.transport_connected;
                    ui.colored_label(
                        if connected { Color32::from_rgb(77, 201, 139) }
                        else { Color32::from_rgb(170, 174, 182) },
                        if connected { "● Runtime connected" } else { "○ Runtime disconnected" },
                    );
                    ui.label(format!("Model: {}", g.shared_state));
                });
                ui.horizontal(|ui| {
                    let ready = g.power_on && g.shared_state == "READY"
                        && g.thinking_verified && g.warmup_verified;
                    ui.colored_label(
                        if ready { Color32::from_rgb(77, 201, 139) }
                        else { Color32::from_rgb(227, 181, 98) },
                        if ready { "Thinking · Verified" } else { "Thinking · Not ready" }
                    );
                    ui.weak(format!("Shared leases: {}", g.active_leases));
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&g.model).strong());
                    if g.working { ui.spinner(); }
                    let enable_on = !g.power_on && !g.working;
                    let enable_off = g.power_on && !g.working;
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        if ui.add_enabled(enable_on, egui::Button::new("ON · تحميل")).clicked() {
                            requested_action = Some("on");
                        }
                        if ui.add_enabled(enable_off, egui::Button::new("OFF · إيقاف")).clicked() {
                            requested_action = Some("off");
                        }
                    }
                    #[cfg(target_arch = "wasm32")]
                    {
                        let _ = (enable_on, enable_off);
                        ui.add_enabled(false, egui::Button::new("Local AI on desktop only"));
                    }
                });
                if let Some(err) = &g.error {
                    ui.colored_label(Color32::from_rgb(235, 112, 101), err);
                }
                ui.add_space(6.0);
                ScrollArea::vertical().max_height(290.0).show(ui, |ui| {
                    ui.strong("جاهز نبني مونتاج له فكرة وإيقاع واضح.");
                    ui.label("TRON بيستخدم نفس الموديل المشترك مع PhotoTron؛ لازم أحافظ على توقيت الفريمات، حقوق الأصول، والصوت، وأرجع معاينة من التايملاين الحقيقي.");
                    ui.add_space(8.0);
                    ui.weak("تنفيذ خطط المونتاج من الشات قيد التوصيل؛ زر التحميل شغال على الـShared Runtime، لكن إرسال أوامر AI مش متاح قبل ربط Native Planner.");
                });
                ui.add_enabled(false,
                    egui::TextEdit::multiline(&mut g.input)
                        .hint_text("وصف فكرة الفيديو أو المطلوب في الـTimeline…")
                        .desired_rows(2)
                );
                ui.add_enabled(false, egui::Button::new("تحليل وتنفيذ المونتاج"));
            }
        });
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(action) = requested_action {
        broker_action(st, ctx.clone(), action);
    }
    #[cfg(target_arch = "wasm32")]
    let _ = requested_action;
    ctx.request_repaint_after(Duration::from_secs(2));
}

/// Explicitly release only FilmTron's OWN broker lease on normal exit.
/// Other creative editors' leases are unaffected.
#[cfg(not(target_arch = "wasm32"))]
pub fn release_on_exit() {
    let _ = filmcraft_automation::tron_local::release();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inert_launcher_does_not_open_model() {
        let st = ViewState::default();
        assert!(!st.open);
        assert!(!st.power_on);
        assert!(!st.transport_connected);
        assert_eq!(st.shared_state, "NOT_LOADED");
    }
}
