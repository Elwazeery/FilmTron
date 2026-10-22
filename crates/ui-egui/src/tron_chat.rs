//! FilmTron native floating TRON panel (shared GemiMCP workstation model).
//!
//! The launcher is passive and uses zero inference/VRAM. No dummy AI replies:
//! editing chat is visibly disabled until FilmTron's specialist native plan
//! pipeline is connected to the central runtime. PhotoTron's fixed poster
//! prompts/commands must NEVER be used for video timeline edits.
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{Align2, Color32, Context, FontId, Frame, Id, Margin, Order, RichText, ScrollArea, Stroke, vec2};

#[derive(Clone, Debug)]
struct ViewState {
    open: bool,
    docked_right: bool,
    working: bool,
    power_on: bool,
    shared_state: String,
    model: String,
    transport_connected: bool,
    thinking_verified: bool,
    warmup_verified: bool,
    active_leases: u64,
    vram_free_mib: Option<u64>,
    error: Option<String>,
    input: String,
    selected_direction_idx: usize,
    last_proposal: Option<serde_json::Value>,
    reviewed_plan_id: Option<String>,
    reviewed_revision: Option<u64>,
    native_review: Option<serde_json::Value>,
    native_receipt: Option<serde_json::Value>,
    last_critique: Option<serde_json::Value>,
    last_status_poll: Instant,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            open: false,
            docked_right: false,
            working: false,
            power_on: false,
            shared_state: "NOT_LOADED".to_owned(),
            model: "qwen3.5:4b-q4_K_M".to_owned(),
            transport_connected: false,
            thinking_verified: false,
            warmup_verified: false,
            active_leases: 0,
            vram_free_mib: None,
            error: None,
            input: String::new(),
            selected_direction_idx: 0,
            last_proposal: None,
            reviewed_plan_id: None,
            reviewed_revision: None,
            native_review: None,
            native_receipt: None,
            last_critique: None,
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

fn paint_bidi_paragraph(ui: &mut egui::Ui, text: &str, size: f32, color: Color32) {
    let wrap_w = ui.available_width().max(140.0);
    let font = FontId::proportional(size);
    let lines = crate::rtl::Lines::new(ui.painter(), text, font, color, wrap_w);
    let desired = vec2(wrap_w, lines.size.y.max(size + 4.0));
    let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
    lines.paint(ui.painter(), rect, color);
}

fn paint_bidi_button(
    ui: &mut egui::Ui,
    text: &str,
    enabled: bool,
    selected: bool,
    accent: Color32,
) -> bool {
    let font = FontId::proportional(12.5);
    let fg = if !enabled {
        Color32::from_gray(125)
    } else if selected {
        Color32::WHITE
    } else {
        ui.visuals().text_color()
    };
    let lines = crate::rtl::Lines::new(ui.painter(), text, font.clone(), fg, f32::INFINITY);
    let pad = vec2(16.0, 10.0);
    let desired = vec2((lines.size.x + pad.x).max(64.0), (lines.size.y + pad.y).max(26.0));
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(desired, sense);
    let bg = if selected {
        accent
    } else if resp.hovered() && enabled {
        ui.visuals().widgets.hovered.bg_fill
    } else {
        ui.visuals().widgets.inactive.bg_fill
    };
    let border = if selected {
        Stroke::new(1.5, accent)
    } else {
        ui.visuals().widgets.inactive.bg_stroke
    };
    ui.painter().rect(rect, 5.0, bg, border, egui::StrokeKind::Inside);
    let text_rect = rect.shrink2(vec2(8.0, 4.0));
    crate::rtl::paint_line(ui.painter(), text_rect, text, font, fg);
    enabled && resp.clicked()
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_status(g: &mut ViewState, response: &serde_json::Value) {
    let status = response.get("status").unwrap_or(response);
    g.shared_state = status
        .get("state")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("UNKNOWN")
        .to_owned();
    g.model = status
        .get("model")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("qwen3.5:4b-q4_K_M")
        .to_owned();
    g.thinking_verified = status
        .get("thinkingVerified")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    g.warmup_verified = status
        .get("warmupVerified")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    g.active_leases = status
        .get("activeLeaseCount")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    g.vram_free_mib = status
        .get("gpu")
        .and_then(|gpu| gpu.get("freeMiB"))
        .and_then(serde_json::Value::as_u64);
    g.transport_connected = true;
}

/// An actual broker call runs off the UI render thread. Clicking ON is explicit
/// local consent; passive polling and the launcher NEVER call /tron/acquire.
#[cfg(not(target_arch = "wasm32"))]
fn broker_action(st: Arc<Mutex<ViewState>>, ctx: Context, action: &'static str) {
    if let Ok(mut g) = st.lock() {
        if g.working && action != "cancel" {
            return;
        }
        g.working = true;
        g.error = None;
    } else {
        return;
    }
    std::thread::spawn(move || {
        let answer = match action {
            "on" => filmcraft_automation::tron_local::activate(),
            "off" => filmcraft_automation::tron_local::release(),
            "minimize" => filmcraft_automation::tron_local::minimize(),
            "cancel" => filmcraft_automation::tron_local::cancel_job(),
            "heartbeat" => filmcraft_automation::tron_local::heartbeat(),
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
                            && g.thinking_verified
                            && g.warmup_verified;
                        if !g.power_on {
                            g.error = Some("Actual model reasoning/warm-up not verified".into());
                        }
                    }
                    if action == "off" || action == "minimize" {
                        g.power_on = false;
                    }
                }
                Err(e) => {
                    g.error = Some(e);
                    if action == "on" {
                        g.power_on = false;
                    }
                    if action == "status" {
                        g.transport_connected = false;
                    }
                }
            }
        }
        ctx.request_repaint();
    });
}

/// Reasoning on the shared model, review only. We never automatically edit
/// media, shift clips, overwrite files, export or modify the current session.
#[cfg(not(target_arch = "wasm32"))]
fn dispatch_film_plan(
    st: Arc<Mutex<ViewState>>,
    ctx: Context,
    prompt: String,
    project: serde_json::Value,
) {
    if let Ok(mut g) = st.lock() {
        if g.working || !g.power_on {
            return;
        }
        g.working = true;
        g.error = None;
        g.last_proposal = None;
        g.reviewed_plan_id = None;
        g.reviewed_revision = None;
        g.native_review = None;
        g.native_receipt = None;
        g.last_critique = None;
        g.selected_direction_idx = 0;
    } else {
        return;
    }
    std::thread::spawn(move || {
        let result = filmcraft_automation::tron_local::prepare_film_plan(&prompt, &project);
        if let Ok(mut g) = st.lock() {
            g.working = false;
            match result {
                Ok(data) => {
                    let proposal = data.get("result").unwrap_or(&data);
                    if proposal.get("ok").and_then(serde_json::Value::as_bool) == Some(false) {
                        g.error = proposal
                            .get("status")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_owned)
                            .or(Some("FILM_PLAN_REJECTED".into()));
                    }
                    g.last_proposal = Some(data);
                }
                Err(err) => {
                    g.error = Some(err);
                }
            }
            g.last_status_poll = Instant::now();
        }
        ctx.request_repaint();
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn dispatch_film_critique(
    st: Arc<Mutex<ViewState>>,
    ctx: Context,
    brief: String,
    project: serde_json::Value,
) {
    if let Ok(mut g) = st.lock() {
        if g.working || !g.power_on {
            return;
        }
        g.working = true;
        g.error = None;
    } else {
        return;
    }
    std::thread::spawn(move || {
        let result = filmcraft_automation::tron_local::critique_film_timeline(&brief, &project);
        if let Ok(mut g) = st.lock() {
            g.working = false;
            match result {
                Ok(data) => {
                    g.last_critique = Some(data);
                }
                Err(err) => {
                    g.error = Some(err);
                }
            }
            g.last_status_poll = Instant::now();
        }
        ctx.request_repaint();
    });
}

#[derive(Clone, Copy)]
enum NativeUiAction {
    ReviewCurrentPlan,
    ApplyCurrentPlan,
    SwitchDirection(usize),
    PromoteCritiqueRefinement,
    UndoLastEdit,
}

/// Passive launcher + native overlay. Does not modify FilmcraftApp playback,
/// audio, media or timeline UI state.
pub fn show(
    ctx: &Context,
    project_state: Option<serde_json::Value>,
    session: &mut filmcraft_engine::Session,
) {
    let st = view(ctx);
    let is_dark = crate::theme::ThemeKind::from_pref(&session.prefs.appearance.color_theme).is_dark();
    let accent = if is_dark {
        Color32::from_rgb(56, 189, 248)
    } else {
        Color32::from_rgb(2, 132, 199)
    };
    let text_primary = if is_dark {
        Color32::from_rgb(241, 245, 249)
    } else {
        Color32::from_rgb(15, 23, 42)
    };
    let text_secondary = if is_dark {
        Color32::from_rgb(148, 163, 184)
    } else {
        Color32::from_rgb(71, 85, 105)
    };
    let card_fill = if is_dark {
        Color32::from_rgb(22, 27, 36)
    } else {
        Color32::from_rgb(241, 245, 249)
    };
    let card_stroke = if is_dark {
        Color32::from_rgb(48, 56, 70)
    } else {
        Color32::from_rgb(203, 213, 225)
    };

    let (is_open, is_docked) = st
        .lock()
        .map(|g| (g.open, g.docked_right))
        .unwrap_or((false, false));

    if !is_open {
        egui::Area::new(Id::new("filmtron.tron.launcher"))
            .order(Order::Foreground)
            .anchor(Align2::RIGHT_BOTTOM, vec2(-22.0, -28.0))
            .show(ctx, |ui| {
                let btn = egui::Button::new(
                    RichText::new("✦ TRON · Creative AI")
                        .color(text_primary)
                        .strong(),
                )
                .fill(card_fill)
                .stroke(Stroke::new(1.2, accent))
                .min_size(vec2(162.0, 42.0));
                if ui.add(btn).clicked() {
                    if let Ok(mut g) = st.lock() {
                        g.open = true;
                        g.last_status_poll = Instant::now() - Duration::from_secs(20);
                    }
                }
            });
        return;
    }

    let needs_poll = st
        .lock()
        .map(|g| !g.working && g.last_status_poll.elapsed() > Duration::from_secs(12))
        .unwrap_or(false);
    #[cfg(not(target_arch = "wasm32"))]
    if needs_poll {
        let active = st.lock().map(|g| g.power_on).unwrap_or(false);
        broker_action(
            st.clone(),
            ctx.clone(),
            if active { "heartbeat" } else { "status" },
        );
    }
    #[cfg(target_arch = "wasm32")]
    let _ = needs_poll;

    let area = ctx.content_rect();
    let default_x = (area.right() - 480.0).max(area.left() + 8.0);
    let mut requested_action: Option<&'static str> = None;
    let mut requested_plan: Option<(String, serde_json::Value)> = None;
    let mut requested_critique: Option<(String, serde_json::Value)> = None;
    let mut native_action: Option<NativeUiAction> = None;

    let mut win = egui::Window::new("TRON · Film Creative Director")
        .id(Id::new("filmtron.tron.window"))
        .order(Order::Foreground)
        .default_size(vec2(455.0, 560.0))
        .min_size(vec2(350.0, 320.0))
        .resizable(true)
        .collapsible(false);

    if is_docked {
        win = win.fixed_pos(egui::pos2(
            (area.right() - 465.0).max(area.left()),
            area.top() + 36.0,
        ));
    } else {
        win = win.default_pos(egui::pos2(default_x, area.top() + 56.0));
    }

    win.show(ctx, |ui| {
        if let Ok(mut g) = st.lock() {
            ui.horizontal(|ui| {
                ui.label(RichText::new("✦ TRON").strong().color(accent).size(15.0));
                ui.label(RichText::new("FilmTron · Editorial Director").color(text_secondary));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("—").on_hover_text("تصغير وإخفاء").clicked() {
                        g.open = false;
                        requested_action = Some("minimize");
                    }
                    let dock_label = if g.docked_right { "⧉ Float" } else { "⇥ Dock" };
                    if ui.small_button(dock_label).on_hover_text("تثبيت جانبي أو نافذة حرة").clicked() {
                        g.docked_right = !g.docked_right;
                    }
                });
            });
            ui.separator();

            // Runtime & GPU Telemetry Bar
            Frame::NONE
                .fill(card_fill)
                .stroke(Stroke::new(1.0, card_stroke))
                .inner_margin(Margin::same(8))
                .corner_radius(6.0)
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        let connected = g.transport_connected;
                        ui.colored_label(
                            if connected {
                                Color32::from_rgb(52, 211, 153)
                            } else {
                                text_secondary
                            },
                            if connected {
                                "● Shared Runtime"
                            } else {
                                "○ Offline (Opt-in)"
                            },
                        );
                        ui.label(RichText::new(format!("State: {}", g.shared_state)).color(text_primary));
                        let ready = g.power_on
                            && g.shared_state == "READY"
                            && g.thinking_verified
                            && g.warmup_verified;
                        ui.colored_label(
                            if ready {
                                Color32::from_rgb(52, 211, 153)
                            } else {
                                Color32::from_rgb(245, 158, 11)
                            },
                            if ready {
                                "Thinking · Verified"
                            } else {
                                "Thinking · Standby"
                            },
                        );
                        ui.label(RichText::new(format!("Leases: {}", g.active_leases)).color(text_secondary));
                        if let Some(free_mib) = g.vram_free_mib {
                            ui.label(RichText::new(format!("VRAM Free: {free_mib} MiB")).color(text_secondary));
                        }
                    });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&g.model).strong().color(text_primary));
                        if g.working {
                            ui.spinner();
                        }
                        let enable_on = !g.power_on && !g.working;
                        let enable_off = g.power_on && !g.working;
                        #[cfg(not(target_arch = "wasm32"))]
                        {
                            if paint_bidi_button(ui, "تشغيل الموديل (ON)", enable_on, g.power_on, accent) {
                                requested_action = Some("on");
                            }
                            if paint_bidi_button(ui, "إيقاف وتفريغ (OFF)", enable_off, false, accent) {
                                requested_action = Some("off");
                            }
                            if g.working && paint_bidi_button(ui, "إلغاء العملية (Cancel)", true, false, accent) {
                                requested_action = Some("cancel");
                            }
                        }
                        #[cfg(target_arch = "wasm32")]
                        {
                            let _ = (enable_on, enable_off);
                            ui.add_enabled(false, egui::Button::new("Local AI on desktop only"));
                        }
                    });
                });

            if let Some(err) = &g.error {
                ui.add_space(4.0);
                ui.colored_label(Color32::from_rgb(239, 68, 68), err);
            }

            ui.add_space(6.0);
            let scroll_max_h = if g.docked_right { 640.0 } else { 360.0 };
            ScrollArea::vertical().max_height(scroll_max_h).show(ui, |ui| {
                paint_bidi_paragraph(
                    ui,
                    "جاهز نراجع اللقطات المتاحة ونبني مونتاج إعلاني أو سينمائي بإيقاع محسوب.",
                    13.5,
                    text_primary,
                );
                ui.add_space(4.0);
                paint_bidi_paragraph(
                    ui,
                    "هنقارن اتجاهين للمونتاج (Direction A & Direction B)، نتحقق من الفريمات والصلاحيات، وبعد موافقتك ننفذ على الـ Timeline مع إمكانية المراجعة أو التراجع (Undo).",
                    12.0,
                    text_secondary,
                );

                if !g.input.trim().is_empty() {
                    ui.add_space(6.0);
                    Frame::NONE
                        .fill(card_fill)
                        .stroke(Stroke::new(1.0, card_stroke))
                        .inner_margin(Margin::same(8))
                        .corner_radius(6.0)
                        .show(ui, |ui| {
                            paint_bidi_paragraph(ui, "البريف الإبداعي الحالي:", 12.0, accent);
                            paint_bidi_paragraph(ui, g.input.trim(), 12.5, text_primary);
                        });
                }

                if let Some(last) = g.last_proposal.clone() {
                    ui.add_space(8.0);
                    ui.separator();
                    let v = last.get("result").unwrap_or(&last);
                    paint_bidi_paragraph(ui, "تحليل المخرج الفني والخطة المقترحة", 13.5, accent);
                    let explanation = v
                        .get("userExplanation")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("تم تجهيز خطة المونتاج المبدئية.");
                    paint_bidi_paragraph(ui, explanation, 13.0, text_primary);

                    if let Some(dirs) = v
                        .get("exploredDirections")
                        .and_then(serde_json::Value::as_array)
                        .filter(|a| a.len() >= 2)
                    {
                        ui.add_space(6.0);
                        paint_bidi_paragraph(
                            ui,
                            "مقارنة الاتجاهات الإبداعية (اختر الاتجاه الأنسب):",
                            13.0,
                            text_primary,
                        );
                        for (idx, dir) in dirs.iter().take(2).enumerate() {
                            let is_sel = g.selected_direction_idx == idx;
                            let title = dir
                                .get("title")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("اتجاه مونتاج");
                            let concept = dir
                                .get("concept")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("");
                            Frame::NONE
                                .fill(card_fill)
                                .stroke(Stroke::new(
                                    if is_sel { 1.5 } else { 1.0 },
                                    if is_sel { accent } else { card_stroke },
                                ))
                                .inner_margin(Margin::same(8))
                                .corner_radius(6.0)
                                .show(ui, |ui| {
                                    if paint_bidi_button(ui, title, true, is_sel, accent) {
                                        native_action = Some(NativeUiAction::SwitchDirection(idx));
                                    }
                                    if !concept.is_empty() {
                                        ui.add_space(2.0);
                                        paint_bidi_paragraph(ui, concept, 12.0, text_secondary);
                                    }
                                });
                            ui.add_space(4.0);
                        }
                    }

                    if let Some(steps) = v
                        .get("plan")
                        .and_then(|x| x.get("steps"))
                        .and_then(serde_json::Value::as_array)
                    {
                        ui.add_space(4.0);
                        paint_bidi_paragraph(
                            ui,
                            &format!(
                                "العمليات المقترحة ({} خطوات Native — لم تُنفّذ بعد):",
                                steps.len()
                            ),
                            12.5,
                            text_primary,
                        );
                        for step in steps.iter().take(8) {
                            let name = step
                                .get("tool")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("unknown");
                            let id = step
                                .get("id")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("?");
                            ui.small(RichText::new(format!("• {id} → {name}")).color(text_secondary));
                        }
                    }

                    if g.power_on && !g.working && v.get("plan").is_some() {
                        ui.add_space(6.0);
                        ui.horizontal_wrapped(|ui| {
                            if paint_bidi_button(ui, "افحص الخطة والصلاحيات (Preflight)", true, false, accent) {
                                native_action = Some(NativeUiAction::ReviewCurrentPlan);
                            }
                            if let Some(check) = &g.native_review {
                                let rev = check
                                    .get("expectedRevision")
                                    .and_then(serde_json::Value::as_u64)
                                    .unwrap_or(0);
                                let cnt = check
                                    .get("stepCount")
                                    .and_then(serde_json::Value::as_u64)
                                    .unwrap_or(0);
                                ui.colored_label(
                                    Color32::from_rgb(52, 211, 153),
                                    format!("✓ Preflight OK (Rev {rev}, {cnt} steps)"),
                                );
                                if paint_bidi_button(ui, "اعتماد وتطبيق الخطوات على الـ Timeline", true, true, accent) {
                                    native_action = Some(NativeUiAction::ApplyCurrentPlan);
                                }
                            }
                        });
                    }
                }

                if let Some(receipt) = &g.native_receipt {
                    ui.add_space(8.0);
                    ui.separator();
                    paint_bidi_paragraph(ui, "النتيجة الفعلية من محرك FilmTron", 13.5, accent);
                    let execution = &receipt["nativeExecution"];
                    let status_str = execution["status"].as_str().unwrap_or("UNKNOWN");
                    let rev_after = receipt
                        .get("revisionAfter")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0);
                    ui.label(
                        RichText::new(format!("Status: {status_str} · Revision: {rev_after}"))
                            .color(text_primary),
                    );
                    ui.horizontal_wrapped(|ui| {
                        if g.power_on && !g.working {
                            if paint_bidi_button(ui, "فحص المعاينة والصوت والـ Scopes (QC)", true, false, accent) {
                                if let Some(proj) = project_state.clone() {
                                    requested_critique = Some((g.input.clone(), proj));
                                }
                            }
                        }
                        if paint_bidi_button(ui, "تراجع عن آخر تعديل (Undo)", true, false, accent) {
                            native_action = Some(NativeUiAction::UndoLastEdit);
                        }
                    });
                }

                if let Some(crit) = &g.last_critique {
                    ui.add_space(8.0);
                    ui.separator();
                    let body = crit.get("result").unwrap_or(crit);
                    paint_bidi_paragraph(ui, "مراجعة الجودة الفنية والـ Scopes (QC Critique)", 13.5, accent);
                    if let Some(summary) = body.get("userSummary").and_then(serde_json::Value::as_str) {
                        paint_bidi_paragraph(ui, summary, 12.5, text_primary);
                    }
                    if body
                        .get("refinementPlan")
                        .and_then(|p| p.get("steps"))
                        .and_then(serde_json::Value::as_array)
                        .is_some_and(|s| !s.is_empty())
                    {
                        if paint_bidi_button(ui, "تجهيز تحسينات المراجعة للاعتماد", true, false, accent) {
                            native_action = Some(NativeUiAction::PromoteCritiqueRefinement);
                        }
                    }
                }
            });

            ui.separator();
            let can_plan = g.power_on && !g.working && project_state.is_some();
            ui.add_enabled(
                can_plan,
                egui::TextEdit::multiline(&mut g.input)
                    .hint_text("Enter creative brief or editorial note…")
                    .desired_rows(2),
            );
            ui.horizontal(|ui| {
                if paint_bidi_button(
                    ui,
                    "فكّر واقترح خطة مونتاج",
                    can_plan && !g.input.trim().is_empty(),
                    true,
                    accent,
                ) {
                    if let Some(project) = project_state.clone() {
                        requested_plan = Some((g.input.clone(), project));
                    }
                }
                paint_bidi_paragraph(
                    ui,
                    "لا يتم أي تعديل أو تصدير تلقائي بدون موافقتك.",
                    11.5,
                    text_secondary,
                );
            });
        }
    });

    #[cfg(not(target_arch = "wasm32"))]
    if let Some(action) = requested_action {
        broker_action(st.clone(), ctx.clone(), action);
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Some((prompt, project)) = requested_plan {
        dispatch_film_plan(view(ctx), ctx.clone(), prompt, project);
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Some((brief, project)) = requested_critique {
        dispatch_film_critique(view(ctx), ctx.clone(), brief, project);
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(act) = native_action {
        if let Ok(mut g) = st.lock() {
            match act {
                NativeUiAction::SwitchDirection(idx) => {
                    if let Some(cur) = g.last_proposal.clone() {
                        if let Some(updated) = crate::tron_actions::select_direction(&cur, idx) {
                            g.selected_direction_idx = idx;
                            g.last_proposal = Some(updated);
                            g.native_review = None;
                            g.reviewed_plan_id = None;
                            g.reviewed_revision = None;
                            g.error = None;
                        }
                    }
                }
                NativeUiAction::PromoteCritiqueRefinement => {
                    if let Some(crit) = g.last_critique.clone() {
                        let body = crit.get("result").unwrap_or(&crit);
                        if let Some(ref_plan) = body.get("refinementPlan").cloned() {
                            let summary = body
                                .get("userSummary")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("خطة تحسين مبنية على فحص المعاينة والـ Scopes.");
                            g.last_proposal = Some(serde_json::json!({
                                "ok": true,
                                "result": {
                                    "ok": true,
                                    "status": "PREPARED_NOT_EXECUTED",
                                    "userExplanation": summary,
                                    "plan": ref_plan
                                }
                            }));
                            g.native_review = None;
                            g.reviewed_plan_id = None;
                            g.reviewed_revision = None;
                            g.error = None;
                        }
                    }
                }
                NativeUiAction::UndoLastEdit => {
                    match crate::tron_actions::undo_last_edit(session) {
                        Ok(undo_receipt) => {
                            g.native_receipt = Some(serde_json::json!({
                                "nativeExecution": {"status": "UNDONE"},
                                "revisionAfter": undo_receipt.get("revisionAfter").cloned().unwrap_or(serde_json::json!(session.revision))
                            }));
                            g.error = None;
                        }
                        Err(e) => {
                            g.error = Some(e);
                        }
                    }
                }
                NativeUiAction::ReviewCurrentPlan => {
                    if let Some(proposal) = g.last_proposal.clone() {
                        match crate::tron_actions::review(session, &proposal) {
                            Ok(check) => {
                                g.reviewed_plan_id = check
                                    .get("planId")
                                    .and_then(serde_json::Value::as_str)
                                    .map(str::to_owned);
                                g.reviewed_revision = check
                                    .get("expectedRevision")
                                    .and_then(serde_json::Value::as_u64);
                                g.native_review = Some(check);
                                g.error = None;
                            }
                            Err(err) => {
                                g.error = Some(err);
                                g.native_review = None;
                                g.reviewed_plan_id = None;
                                g.reviewed_revision = None;
                            }
                        }
                    }
                }
                NativeUiAction::ApplyCurrentPlan => {
                    if let Some(proposal) = g.last_proposal.clone() {
                        let res = match (g.reviewed_plan_id.clone(), g.reviewed_revision) {
                            (Some(plan_id), Some(revision)) => {
                                crate::tron_actions::apply(session, &proposal, &plan_id, revision)
                            }
                            _ => Err("FILM_PLAN_NOT_REVIEWED".into()),
                        };
                        match res {
                            Ok(receipt) => {
                                g.native_receipt = Some(receipt);
                                g.native_review = None;
                                g.reviewed_plan_id = None;
                                g.reviewed_revision = None;
                                g.error = None;
                            }
                            Err(err) => {
                                g.error = Some(err);
                                g.native_review = None;
                                g.reviewed_plan_id = None;
                                g.reviewed_revision = None;
                            }
                        }
                    }
                }
            }
        }
    }
    #[cfg(target_arch = "wasm32")]
    let _ = (requested_action, requested_plan, requested_critique, native_action, session);
    ctx.request_repaint_after(Duration::from_secs(2));
}

/// Explicitly release only FilmTron's OWN broker lease on normal exit.
/// Other creative editors' leases are unaffected.
#[cfg(not(target_arch = "wasm32"))]
pub fn release_on_exit() {
    let _ = filmcraft_automation::tron_local::release();
}

/// Returns whether TRON is currently powered ON in the UI state.
pub fn is_power_on(ctx: &Context) -> bool {
    let st = view(ctx);
    let g = st.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    g.power_on
}

/// Returns the current shared broker state string (`"NOT_LOADED"`, `"READY"`, etc.).
pub fn shared_state(ctx: &Context) -> String {
    let st = view(ctx);
    let g = st.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    g.shared_state.clone()
}

/// Synchronize the floating/docked TRON panel state with live broker status, proposals,
/// native preflight reviews, execution receipts, and post-execution QC critiques.
#[cfg(not(target_arch = "wasm32"))]
#[allow(clippy::too_many_arguments)]
pub fn sync_live_panel_state(
    ctx: &Context,
    open: bool,
    docked_right: bool,
    status_response: Option<&serde_json::Value>,
    input: Option<&str>,
    proposal: Option<serde_json::Value>,
    review: Option<serde_json::Value>,
    receipt: Option<serde_json::Value>,
    critique: Option<serde_json::Value>,
) {
    let st = view(ctx);
    let mut g = st.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    g.open = open;
    g.docked_right = docked_right;
    if let Some(resp) = status_response {
        parse_status(&mut g, resp);
        g.power_on = matches!(g.shared_state.as_str(), "READY" | "BUSY");
    }
    if let Some(text) = input {
        g.input = text.to_owned();
    }
    g.last_proposal = proposal;
    if let Some(rev) = review {
        g.reviewed_plan_id = rev
            .get("planId")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned);
        g.reviewed_revision = rev
            .get("expectedRevision")
            .and_then(serde_json::Value::as_u64);
        g.native_review = Some(rev);
    } else {
        g.native_review = None;
    }
    g.native_receipt = receipt;
    g.last_critique = critique;
    g.last_status_poll = Instant::now();
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
        assert!(st.last_proposal.is_none());
    }
}
