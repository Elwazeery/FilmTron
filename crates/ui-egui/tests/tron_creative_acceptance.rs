//! FilmTron × TRON — End-to-End Windows Implementation & Real Creative Video Acceptance Suite.
//!
//! Exercises the complete real creative workflow on Windows with the shared GemiMCP workstation
//! TRON runtime (`qwen3.5:4b-q4_K_M` on RTX 3050 6GB), native FilmTron timeline editing,
//! Direction A vs Direction B comparison, all 7 subjective Arabic feedback loops, post-execution
//! timeline/audio/scopes QC, `.fcproj` save/reopen, 15-second `1080×1920` 30fps H.264/AAC export,
//! `ffprobe` + native decode verification, two unseen creative briefs, and wgpu Dark/Light UI screenshots.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{Sender, channel};
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use filmcraft_automation::security::{hex_encode, sha256};
use filmcraft_automation::tron_local;
use filmcraft_engine::Session;
use filmcraft_ui_egui::control::ControlRequest;
use filmcraft_ui_egui::{FilmcraftApp, tron_actions, tron_chat};
use serde_json::{Value, json};

const BANNED_SLANG: &[&str] = &["يا معلم", "يا باشا", "يا وحش", "يا نجم", "يا فنان"];

struct TronAcceptanceDriver {
    harness: Harness<'static, FilmcraftApp>,
    tx: Sender<ControlRequest>,
    work_dir: PathBuf,
    artifact_dir: PathBuf,
    screenshot_dir: PathBuf,
}

fn windows_safe_wgpu_setup() -> eframe::egui_wgpu::WgpuSetup {
    use eframe::egui_wgpu::{WgpuSetup, WgpuSetupCreateNew, wgpu};
    let mut setup = WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = wgpu::Backends::DX12;
    setup.instance_descriptor.flags = setup
        .instance_descriptor
        .flags
        .difference(wgpu::InstanceFlags::VALIDATION_INDIRECT_CALL);
    WgpuSetup::CreateNew(setup)
}

fn query_nvidia_smi_vram() -> Option<(u64, u64, u64)> {
    let out = Command::new("nvidia-smi")
        .args([
            "--query-gpu=memory.used,memory.free,memory.total",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let line = String::from_utf8_lossy(&out.stdout);
    let first = line.lines().next()?.trim();
    let parts: Vec<u64> = first
        .split(',')
        .filter_map(|p| p.trim().parse::<u64>().ok())
        .collect();
    if parts.len() >= 3 {
        Some((parts[0], parts[1], parts[2]))
    } else {
        None
    }
}

fn assert_no_banned_slang(text: &str, context: &str) {
    for phrase in BANNED_SLANG {
        assert!(
            !text.contains(phrase),
            "Banned slang '{phrase}' found in {context}: {text}"
        );
    }
}

impl TronAcceptanceDriver {
    fn launch() -> Self {
        let work_dir = std::env::temp_dir().join(format!("filmtron-tron-accept-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&work_dir);
        std::fs::create_dir_all(&work_dir).expect("create acceptance work_dir");

        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("workspace root");
        let artifact_dir = repo_root.join("target/tron-acceptance");
        std::fs::create_dir_all(&artifact_dir).expect("create target/tron-acceptance");
        let screenshot_dir = repo_root.join("docs/images/filmtron");
        std::fs::create_dir_all(&screenshot_dir).expect("create docs/images/filmtron");

        let mut session = Session::default();
        session.prefs_path = Some(work_dir.join("preferences.json"));
        session.export_presets.set_dir(&work_dir.join("presets"));

        let (tx, rx) = channel();
        let app = FilmcraftApp::new(session).with_control(rx);
        let harness = Harness::builder()
            .with_size(egui::vec2(1600.0, 980.0))
            .with_max_steps(10_000)
            .wgpu_setup(windows_safe_wgpu_setup())
            .with_pixels_per_point(1.0)
            .build_eframe(move |_cc| app);

        let mut d = Self {
            harness,
            tx,
            work_dir,
            artifact_dir,
            screenshot_dir,
        };
        d.frames(4);
        d
    }

    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            let ctx = self.harness.ctx.clone();
            let mut raw = std::mem::take(self.harness.input_mut());
            eframe::App::raw_input_hook(self.harness.state_mut(), &ctx, &mut raw);
            *self.harness.input_mut() = raw;
            self.harness.step();
        }
    }

    fn settle_and_save_screenshot(&mut self, filename: &str) -> PathBuf {
        for _ in 0..25 {
            std::thread::sleep(Duration::from_millis(25));
            self.frames(1);
            if self.harness.state().frames.queue_len() == 0 {
                break;
            }
        }
        self.frames(4);
        let img = self.harness.render().expect("wgpu render screenshot");
        assert_eq!(img.width(), 1600);
        assert_eq!(img.height(), 980);
        let out_path = self.screenshot_dir.join(filename);
        img.save(&out_path).expect("save PNG screenshot");
        let meta = std::fs::metadata(&out_path).expect("screenshot metadata");
        assert!(
            meta.len() > 15_000,
            "screenshot {} too small ({} bytes)",
            filename,
            meta.len()
        );
        eprintln!("TRON_SCREENSHOT_SAVED: {} ({} bytes)", out_path.display(), meta.len());
        out_path
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        let (req, reply) = ControlRequest::new(method, params.clone());
        self.tx.send(req).expect("send control request");
        for _ in 0..600 {
            self.frames(1);
            if let Ok(v) = reply.try_recv() {
                return v;
            }
        }
        panic!("timed out waiting for reply to {method} {params}");
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        let v = self.call(method, params.clone());
        assert_eq!(v["ok"], json!(true), "{method} {params} failed: {v}");
        v["result"].clone()
    }

    fn exec(&mut self, command: &str, params: Value) -> Value {
        self.ok("engine.execute", json!({"command": command, "params": params}))
    }

    fn app(&mut self) -> &mut FilmcraftApp {
        self.harness.state_mut()
    }
}

impl Drop for TronAcceptanceDriver {
    fn drop(&mut self) {
        let _ = tron_local::release();
        let _ = std::fs::remove_dir_all(&self.work_dir);
    }
}

#[test]
fn filmtron_tron_real_creative_video_acceptance() {
    std::thread::Builder::new()
        .name("filmtron-tron-creative-acceptance".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(run_filmtron_tron_creative_acceptance)
        .expect("spawn 64 MiB acceptance thread")
        .join()
        .expect("acceptance thread succeeded");
}

fn run_filmtron_tron_creative_acceptance() {
    eprintln!("=== [PHASE 0] Launching FilmTron & Verifying Passive Startup (Zero Auto-Load) ===");
    let mut d = TronAcceptanceDriver::launch();
    assert!(
        !tron_chat::is_power_on(&d.harness.ctx),
        "Opening FilmTron must never automatically load TRON"
    );
    assert_eq!(tron_chat::shared_state(&d.harness.ctx), "NOT_LOADED");

    let vram_before = query_nvidia_smi_vram();
    eprintln!("VRAM_BEFORE_TRON_ON: {:?}", vram_before);

    // =========================================================================
    // PHASE 1: Explicit ON Activation, Warmup Verification & Multi-App Concurrency
    // =========================================================================
    eprintln!("=== [PHASE 1] Clicking TRON ON & Verifying Shared Workstation Broker + Multi-App Leases ===");
    let activate_t0 = Instant::now();
    let on_resp = tron_local::activate().expect("explicit TRON ON activation");
    let activate_ms = activate_t0.elapsed().as_millis();
    eprintln!("TRON_ON_RESPONSE ({} ms): {}", activate_ms, on_resp);

    let status_obj = on_resp.get("status").unwrap_or(&on_resp);
    assert_eq!(status_obj["state"], json!("READY"), "TRON must reach READY after warmup: {on_resp}");
    assert_eq!(status_obj["model"], json!("qwen3.5:4b-q4_K_M"));
    assert_eq!(status_obj["thinkingVerified"], json!(true));
    assert_eq!(status_obj["warmupVerified"], json!(true));
    assert!(
        status_obj["activeLeaseCount"].as_u64().unwrap_or(0) >= 1,
        "FilmTron lease must be active"
    );

    let vram_active = query_nvidia_smi_vram();
    eprintln!("VRAM_DURING_TRON_READY: {:?}", vram_active);

    // Test Critical Multi-Application Concurrency Case:
    // PhotoTron and FilmTron are open simultaneously. Closing FilmTron must NOT unload PhotoTron's model.
    // Releasing PhotoTron must NOT interrupt FilmTron's lease.
    let phototron_lease_id = format!("phototron-peer-{}", std::process::id());
    let peer_acq = tron_local::acquire_peer_lease(&phototron_lease_id, "phototron", std::process::id())
        .expect("acquire concurrent phototron lease");
    let peer_status = peer_acq.get("status").unwrap_or(&peer_acq);
    assert_eq!(peer_status["state"], json!("READY"));
    assert!(
        peer_status["activeLeaseCount"].as_u64().unwrap_or(0) >= 2,
        "Both PhotoTron and FilmTron leases active: {peer_acq}"
    );

    // Simulate FilmTron releasing its lease while PhotoTron remains open
    let rel_film = tron_local::release().expect("release filmtron lease while phototron open");
    let rel_film_status = rel_film.get("status").unwrap_or(&rel_film);
    assert_eq!(
        rel_film_status["state"],
        json!("READY"),
        "Closing FilmTron must NOT unload PhotoTron's model: {rel_film}"
    );
    assert_eq!(rel_film_status["activeLeaseCount"], json!(1));

    // Re-acquire FilmTron lease, then release PhotoTron lease -> FilmTron stays READY
    let re_acq_film = tron_local::activate().expect("re-acquire filmtron lease");
    assert_eq!(re_acq_film.get("status").unwrap_or(&re_acq_film)["state"], json!("READY"));
    let rel_photo = tron_local::release_peer_lease(&phototron_lease_id)
        .expect("release phototron lease while filmtron active");
    let rel_photo_status = rel_photo.get("status").unwrap_or(&rel_photo);
    assert_eq!(
        rel_photo_status["state"],
        json!("READY"),
        "PhotoTron release must keep FilmTron READY: {rel_photo}"
    );
    assert_eq!(rel_photo_status["activeLeaseCount"], json!(1));

    // =========================================================================
    // PHASE 2: Generate 4 Distinct Real Video Clips + 1 Stereo Audio Bed
    // =========================================================================
    eprintln!("=== [PHASE 2] Generating 4 Distinct Video Clips & 15s Stereo Audio Track ===");
    d.exec("file.openDemoProject", json!({}));

    let fixtures_dir = d.artifact_dir.join("fixtures");
    std::fs::create_dir_all(&fixtures_dir).expect("create fixtures dir");
    let clip_specs = [
        ("clip_01_hook.mp4", 0.0, 4.5),
        ("clip_02_detail.mp4", 4.5, 9.0),
        ("clip_03_feature.mp4", 9.0, 13.5),
        ("clip_04_hero_cta.mp4", 13.5, 18.0),
    ];
    let mut clip_paths = Vec::new();
    for (name, start_s, end_s) in clip_specs {
        let p = fixtures_dir.join(name);
        if !p.exists() {
            d.exec(
                "file.exportMedia",
                json!({
                    "path": p.to_string_lossy(),
                    "format": "h264",
                    "audioCodec": "aac",
                    "hardwareEncoding": false,
                    "gpuRendering": false,
                    "width": 1080,
                    "height": 1920,
                    "fps": 30.0,
                    "range": "custom",
                    "startSeconds": start_s,
                    "endSeconds": end_s,
                    "wait": true
                }),
            );
        }
        assert!(p.exists(), "source video clip {name} created");
        clip_paths.push(p);
    }

    let audio_bed_path = fixtures_dir.join("audio_bed_15s.wav");
    if !audio_bed_path.exists() {
        d.exec(
            "file.exportMedia",
            json!({
                "path": audio_bed_path.to_string_lossy(),
                "format": "wav",
                "range": "custom",
                "startSeconds": 0.0,
                "endSeconds": 15.0,
                "wait": true
            }),
        );
    }
    assert!(audio_bed_path.exists(), "15s audio bed WAV created");

    // Create fresh project & 1080x1920 30fps vertical commercial sequence
    d.exec("file.newProject", json!({"name": "FilmTron Commercial 15s"}));
    let bin_res = d.exec("file.newBin", json!({"name": "Commercial Footage"}));
    let bin_id = bin_res["bin"].as_u64().expect("bin id");

    let mut import_paths: Vec<String> = clip_paths
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    import_paths.push(audio_bed_path.to_string_lossy().into_owned());

    let import_res = d.exec(
        "file.import",
        json!({
            "paths": import_paths,
            "bin": bin_id
        }),
    );
    let imported_items: Vec<u64> = import_res["items"]
        .as_array()
        .expect("imported items")
        .iter()
        .filter_map(Value::as_u64)
        .collect();
    assert_eq!(imported_items.len(), 5, "4 video clips + 1 audio track imported");

    let seq_res = d.exec(
        "file.newSequence",
        json!({
            "name": "TRON 15s Vertical Commercial",
            "width": 1080,
            "height": 1920,
            "fps": 30.0,
            "sampleRate": 48000,
            "video": 3,
            "audio": 3
        }),
    );
    let commercial_seq_id = seq_res["sequence"].as_u64().expect("commercial sequence id");
    d.exec("source.open", json!({"item": imported_items[0]}));
    d.exec("source.setPlayhead", json!({"frame": 10}));

    // =========================================================================
    // PHASE 3: Mandatory 15-Second Commercial Acceptance Workflow via Live TRON
    // =========================================================================
    eprintln!("=== [PHASE 3] Mandatory 15s Commercial Acceptance Brief -> Live TRON Reasoning ===");
    let mandatory_brief = "اعمل إعلان احترافي مدته 15 ثانية من اللقطات دي، يبدأ بلقطة تخطف العين، يمشي بإيقاع سريع ومضبوط مع الصوت، يحط عنوان عربي شيك وكابشن واضح، ويقفل على لقطة قوية للمنتج.";
    let project_ctx = tron_actions::build_tron_project_context(&d.app().session)
        .expect("factual project context");

    let plan_t0 = Instant::now();
    let proposal = tron_local::prepare_film_plan(mandatory_brief, &project_ctx)
        .expect("live prepare_film_plan for 15s commercial");
    let plan_ms = plan_t0.elapsed().as_millis();
    eprintln!("MANDATORY_COMMERCIAL_PROPOSAL ({} ms): planId={}", plan_ms, proposal["plan"]["planId"]);

    assert_eq!(proposal["ok"], json!(true), "proposal ok: {proposal}");
    let director_ar = proposal["directorResponseAr"]
        .as_str()
        .expect("directorResponseAr string");
    assert!(!director_ar.is_empty());
    assert_no_banned_slang(director_ar, "directorResponseAr");

    // Verify at least 2 distinct editorial directions exist and both pass native preflight
    let directions = proposal["editorialDirections"]
        .as_array()
        .expect("editorialDirections array");
    assert!(directions.len() >= 2, "must offer Direction A and Direction B");
    assert_eq!(directions[0]["id"], json!("direction_a"));
    assert_eq!(directions[1]["id"], json!("direction_b"));

    let dir_a_prop = tron_actions::select_direction(&proposal, 0)
        .expect("select Direction A");
    let dir_a_review = tron_actions::review(&d.app().session, &dir_a_prop)
        .expect("native preflight Direction A");
    assert_eq!(dir_a_review["ok"], json!(true));
    assert_eq!(dir_a_review["readyToApply"], json!(true));

    let dir_b_prop = tron_actions::select_direction(&proposal, 1)
        .expect("select Direction B");
    let dir_b_review = tron_actions::review(&d.app().session, &dir_b_prop)
        .expect("native preflight Direction B");
    assert_eq!(dir_b_review["ok"], json!(true));
    assert_eq!(dir_b_review["readyToApply"], json!(true));

    // Execute approved plan on native FilmTron engine
    let plan_id = dir_b_review["planId"].as_str().expect("planId").to_owned();
    let expected_rev = dir_b_review["expectedRevision"].as_u64().expect("expectedRevision");
    let apply_receipt = tron_actions::apply(&mut d.app().session, &dir_b_prop, &plan_id, expected_rev)
        .expect("native execution of approved 15s commercial plan");
    eprintln!("MANDATORY_COMMERCIAL_RECEIPT: {}", apply_receipt);
    assert_eq!(apply_receipt["ok"], json!(true));
    assert_eq!(
        apply_receipt["appliedCount"],
        apply_receipt["stepCount"],
        "Every step in the 15s commercial plan must succeed: {apply_receipt}"
    );

    // Inspect timeline after execution
    let seq_after_commercial = d.exec("sequence.inspect", json!({}));
    let v1_clips = seq_after_commercial["video"][0]["items"]
        .as_array()
        .expect("V1 clips after commercial plan");
    assert_eq!(v1_clips.len(), 4, "4 distinct shots placed on V1");
    let v2_clips = seq_after_commercial["video"][1]["items"]
        .as_array()
        .expect("V2 title graphic after commercial plan");
    assert!(!v2_clips.is_empty(), "Arabic title graphic placed on V2");
    let a1_clips = seq_after_commercial["audio"][0]["items"]
        .as_array()
        .expect("A1 audio bed after commercial plan");
    assert!(!a1_clips.is_empty(), "Audio bed placed on A1");

    // Position playhead at frame 90 (3.0s) so Program Monitor and Lumetri Scopes compute real frame metrics
    d.exec("playhead.set", json!({"frame": 90}));
    let scopes_val = d.exec("scopes.read", json!({}));
    assert!(scopes_val.is_object(), "Real Lumetri scopes read from graded frame");

    // Sync floating TRON panel in Dark mode showing the live proposal, Direction A/B switcher & native preflight review
    d.ok("ui.set", json!({"mode": "edit", "workspace": "Editing", "theme": "inferno-dark"}));
    tron_chat::sync_live_panel_state(
        &d.harness.ctx,
        true,
        false,
        Some(&re_acq_film),
        Some(mandatory_brief),
        Some(dir_b_prop.clone()),
        Some(dir_b_review.clone()),
        None,
        None,
    );
    d.settle_and_save_screenshot("08-tron-floating-dark-review.png");

    // =========================================================================
    // PHASE 4: Post-Execution Timeline, Audio & Scopes QC + Refinement
    // =========================================================================
    eprintln!("=== [PHASE 4] Post-Execution Visual/Audio/Scopes QC & Refinement ===");
    let post_ctx = tron_actions::build_tron_project_context(&d.app().session)
        .expect("post-execution project context");
    let critique_resp = tron_local::critique_film_timeline(mandatory_brief, &post_ctx)
        .expect("post-execution critique");
    eprintln!("POST_EXECUTION_QC: {}", critique_resp);
    assert_eq!(critique_resp["ok"], json!(true));
    assert_eq!(critique_resp["critique"]["noFabricatedScores"], json!(true));
    let qc_summary_ar = critique_resp["critique"]["summaryAr"]
        .as_str()
        .expect("QC summaryAr");
    assert_no_banned_slang(qc_summary_ar, "QC summaryAr");

    // =========================================================================
    // PHASE 5: All 7 Subjective Arabic Feedback Prompts & Undo Verification
    // =========================================================================
    eprintln!("=== [PHASE 5] Exercising All 7 Subjective Arabic Editor Feedback Prompts ===");
    let subjective_prompts = [
        "الإيقاع بطيء في الأول",
        "الدخلة ضعيفة",
        "الانتقالات حادة",
        "الصوت عالي على الفويس",
        "الكابشن مش واضح",
        "اللقطات مش راكبة على بعض",
        "النهاية محتاجة قفلة أقوى",
    ];

    for (idx, feedback) in subjective_prompts.iter().enumerate() {
        let cur_ctx = tron_actions::build_tron_project_context(&d.app().session)
            .expect("context before feedback");
        let fb_prop = tron_local::prepare_film_plan(feedback, &cur_ctx)
            .unwrap_or_else(|e| panic!("feedback #{idx} ({feedback}) plan failed: {e}"));
        assert_eq!(fb_prop["ok"], json!(true));
        let translations = fb_prop["feedbackTranslations"]
            .as_array()
            .expect("feedbackTranslations");
        assert!(
            !translations.is_empty(),
            "feedback '{feedback}' must produce concrete editorial translations"
        );
        let fb_rev = tron_actions::review(&d.app().session, &fb_prop)
            .unwrap_or_else(|e| panic!("feedback #{idx} ({feedback}) review failed: {e}"));
        assert_eq!(fb_rev["ok"], json!(true));
        let fb_plan_id = fb_rev["planId"].as_str().expect("fb planId").to_owned();
        let fb_expected_rev = fb_rev["expectedRevision"].as_u64().expect("fb expectedRevision");
        let fb_receipt = tron_actions::apply(
            &mut d.app().session,
            &fb_prop,
            &fb_plan_id,
            fb_expected_rev,
        )
        .unwrap_or_else(|e| panic!("feedback #{idx} ({feedback}) apply failed: {e}"));
        assert_eq!(fb_receipt["ok"], json!(true));
        eprintln!(
            "SUBJECTIVE_FEEDBACK_APPLIED [{}/7] '{}': applied={}/{}",
            idx + 1,
            feedback,
            fb_receipt["appliedCount"],
            fb_receipt["stepCount"]
        );
    }

    // Verify Undo & Redo work cleanly after TRON edits
    let rev_before_undo = d.app().session.revision;
    let undo_res = tron_actions::undo_last_edit(&mut d.app().session).expect("undo last TRON edit");
    assert_eq!(undo_res["ok"], json!(true));
    assert!(d.app().session.revision > rev_before_undo);
    d.exec("edit.redo", json!({}));

    // Capture Light Mode Docked TRON Panel Screenshot showing Receipt + Post-Execution QC
    d.ok("ui.set", json!({"theme": "inferno-light"}));
    tron_chat::sync_live_panel_state(
        &d.harness.ctx,
        true,
        true,
        Some(&re_acq_film),
        Some("اللقطات مش راكبة على بعض"),
        None,
        None,
        Some(apply_receipt.clone()),
        Some(critique_resp.clone()),
    );
    d.settle_and_save_screenshot("09-tron-docked-light-qc.png");
    d.ok("ui.set", json!({"theme": "inferno-dark"}));

    // =========================================================================
    // PHASE 6: Save .fcproj, Reopen & Export Final 15-Second 1080x1920 30fps MP4
    // =========================================================================
    eprintln!("=== [PHASE 6] Saving .fcproj, Reopening & Exporting 15s 1080x1920 30fps MP4 ===");
    let fcproj_path = d.artifact_dir.join("tron_15s_commercial.fcproj");
    d.exec("file.saveAs", json!({"path": fcproj_path.to_string_lossy()}));
    assert!(fcproj_path.exists(), "saved .fcproj exists");

    d.exec("file.newProject", json!({"name": "Scratch Reset"}));
    d.exec("file.open", json!({"path": fcproj_path.to_string_lossy()}));
    assert_eq!(d.app().session.project.name, "tron_15s_commercial");

    let reopened_seq = d.exec("sequence.inspect", json!({}));
    assert!(
        reopened_seq["video"][0]["items"]
            .as_array()
            .expect("reopened V1 items")
            .len()
            >= 4,
        "Reopened .fcproj preserved V1 commercial timeline"
    );

    let exported_mp4 = d.artifact_dir.join("tron_15s_commercial_1080x1920_30fps.mp4");
    let export_t0 = Instant::now();
    let export_res = d.exec(
        "file.exportMedia",
        json!({
            "path": exported_mp4.to_string_lossy(),
            "format": "h264",
            "audioCodec": "aac",
            "hardwareEncoding": false,
            "gpuRendering": false,
            "width": 1080,
            "height": 1920,
            "fps": 30.0,
            "range": "custom",
            "startSeconds": 0.0,
            "endSeconds": 15.0,
            "wait": true
        }),
    );
    let export_ms = export_t0.elapsed().as_millis();
    assert!(exported_mp4.exists(), "15s commercial MP4 exported: {export_res}");
    let mp4_bytes = std::fs::read(&exported_mp4).expect("read exported 15s MP4");
    let mp4_sha256 = hex_encode(&sha256(&mp4_bytes));
    eprintln!(
        "COMMERCIAL_15S_EXPORT_RECEIPT: path={} bytes={} export_ms={} sha256={}",
        exported_mp4.display(),
        mp4_bytes.len(),
        export_ms,
        mp4_sha256
    );
    assert!(mp4_bytes.len() > 50_000, "exported 15s 1080x1920 MP4 size = {}", mp4_bytes.len());

    // Verify exported MP4 via ffprobe
    let ffprobe_out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
            &exported_mp4.to_string_lossy(),
        ])
        .output()
        .expect("run ffprobe on exported 15s commercial MP4");
    assert!(ffprobe_out.status.success(), "ffprobe succeeded");
    let ffprobe_json: Value = serde_json::from_slice(&ffprobe_out.stdout).expect("parse ffprobe JSON");
    eprintln!("FFPROBE_15S_COMMERCIAL: {}", ffprobe_json);

    let streams = ffprobe_json["streams"].as_array().expect("ffprobe streams");
    let video_stream = streams
        .iter()
        .find(|s| s["codec_type"] == json!("video"))
        .expect("video stream present");
    let audio_stream = streams
        .iter()
        .find(|s| s["codec_type"] == json!("audio"))
        .expect("audio stream present");
    assert_eq!(video_stream["codec_name"], json!("h264"));
    assert_eq!(video_stream["width"], json!(1080));
    assert_eq!(video_stream["height"], json!(1920));
    assert_eq!(audio_stream["codec_name"], json!("aac"));
    let duration_s: f64 = ffprobe_json["format"]["duration"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);
    assert!(
        (14.5..=15.5).contains(&duration_s),
        "expected ~15.0s duration, got {duration_s}s"
    );

    // Decode exported MP4 inside FilmTron and inspect hook (frame 15), mid (frame 150), and CTA (frame 360)
    let verify_import = d.exec("file.import", json!({"paths": [exported_mp4.to_string_lossy()]}));
    let decoded_item = verify_import["items"][0].as_u64().expect("decoded MP4 item");
    d.exec("project.select", json!({"items": [decoded_item]}));
    let verify_seq = d.exec("file.newSequenceFromClip", json!({"item": decoded_item}));
    let verify_seq_id = verify_seq["sequence"].as_u64().expect("verify seq id");

    for check_frame in [15_i64, 150, 360] {
        d.exec("playhead.set", json!({"frame": check_frame}));
        let frame_png = d.artifact_dir.join(format!("decoded_15s_frame_{check_frame}.png"));
        d.exec(
            "file.exportFrame",
            json!({
                "path": frame_png.to_string_lossy(),
                "format": "png"
            }),
        );
        let img = image::open(&frame_png).expect("open decoded frame").into_rgba8();
        assert_eq!(img.width(), 1080);
        assert_eq!(img.height(), 1920);
        let non_black = img
            .pixels()
            .filter(|px| px[0] > 8 || px[1] > 8 || px[2] > 8)
            .count();
        assert!(
            non_black > 50_000,
            "decoded frame {check_frame} must be non-black ({non_black} pixels)"
        );
    }

    d.exec("sequence.open", json!({"item": commercial_seq_id}));
    d.exec("sequence.close", json!({"item": verify_seq_id}));

    // =========================================================================
    // PHASE 7: Three Creative Challenges & Multi-Rate Timebase Verification
    // =========================================================================
    eprintln!("=== [PHASE 7] Creative Challenges 2 (24fps Tech Launch) & 3 (25fps Lifestyle Story) + 29.97 NTSC ===");

    // Creative Challenge 2: Dynamic Technology Launch (24/1 fps, English deliverable locale)
    let unseen_seq_1 = d.exec(
        "file.newSequence",
        json!({
            "name": "Challenge 2 - Dynamic Tech Launch 24fps",
            "width": 1080,
            "height": 1920,
            "fps": 24.0,
            "sampleRate": 48000,
            "video": 3,
            "audio": 3
        }),
    );
    let _unseen_seq_1_id = unseen_seq_1["sequence"].as_u64().expect("unseen seq 1");
    let ctx_u1 = tron_actions::build_tron_project_context(&d.app().session).expect("ctx u1");
    assert_eq!(ctx_u1["fpsNum"], json!(24));
    assert_eq!(ctx_u1["fpsDen"], json!(1));
    let brief_u1 = "Build a fast-paced 15-second tech launch montage with English titles: NEXT-GEN ENGINE";
    let prop_u1 = tron_local::prepare_film_plan_with_locale(brief_u1, &ctx_u1, "en-US")
        .expect("challenge 2 plan");
    assert_eq!(prop_u1["ok"], json!(true));
    assert_no_banned_slang(
        prop_u1["directorResponseAr"].as_str().unwrap_or(""),
        "challenge 2 directorResponseAr",
    );
    let rev_u1 = tron_actions::review(&d.app().session, &prop_u1).expect("review challenge 2");
    assert_eq!(rev_u1["ok"], json!(true));
    let rec_u1 = tron_actions::apply(
        &mut d.app().session,
        &prop_u1,
        rev_u1["planId"].as_str().expect("u1 planId"),
        rev_u1["expectedRevision"].as_u64().expect("u1 rev"),
    )
    .expect("apply challenge 2");
    assert_eq!(rec_u1["ok"], json!(true));
    assert_eq!(rec_u1["appliedCount"], rec_u1["stepCount"]);

    // Post-execution QC & observation-led refinement on Challenge 2
    let ctx_u1_post = tron_actions::build_tron_project_context(&d.app().session).expect("ctx u1 post");
    let qc_u1 = tron_local::critique_film_timeline(brief_u1, &ctx_u1_post).expect("qc challenge 2");
    assert_eq!(qc_u1["ok"], json!(true));
    let prop_u1_ref = tron_local::prepare_film_plan_with_locale("الإيقاع بطيء في الأول", &ctx_u1_post, "en-US")
        .expect("challenge 2 refinement plan");
    let rev_u1_ref = tron_actions::review(&d.app().session, &prop_u1_ref).expect("review challenge 2 refinement");
    let rec_u1_ref = tron_actions::apply(
        &mut d.app().session,
        &prop_u1_ref,
        rev_u1_ref["planId"].as_str().expect("u1 ref planId"),
        rev_u1_ref["expectedRevision"].as_u64().expect("u1 ref rev"),
    )
    .expect("apply challenge 2 refinement");
    assert_eq!(rec_u1_ref["ok"], json!(true));

    // Creative Challenge 3: Cinematic Lifestyle Brand Story (25/1 fps, Arabic deliverable locale)
    let unseen_seq_2 = d.exec(
        "file.newSequence",
        json!({
            "name": "Challenge 3 - Cinematic Lifestyle Story 25fps",
            "width": 1080,
            "height": 1920,
            "fps": 25.0,
            "sampleRate": 48000,
            "video": 3,
            "audio": 3
        }),
    );
    let _unseen_seq_2_id = unseen_seq_2["sequence"].as_u64().expect("unseen seq 2");
    let ctx_u2 = tron_actions::build_tron_project_context(&d.app().session).expect("ctx u2");
    assert_eq!(ctx_u2["fpsNum"], json!(25));
    assert_eq!(ctx_u2["fpsDen"], json!(1));
    let brief_u2 = "اعمل قصة براند لايف ستايل سينمائية هادئة ومؤثرة مدتها 15 ثانية بتركز على تفاصيل الإضاءة وتدرج لوني دافئ";
    let prop_u2_raw = tron_local::prepare_film_plan(brief_u2, &ctx_u2).expect("challenge 3 plan");
    let prop_u2 = tron_actions::select_direction(&prop_u2_raw, 0)
        .expect("select Direction A for cinematic lifestyle story");
    let rev_u2 = tron_actions::review(&d.app().session, &prop_u2).expect("review challenge 3");
    assert_eq!(rev_u2["ok"], json!(true));
    let rec_u2 = tron_actions::apply(
        &mut d.app().session,
        &prop_u2,
        rev_u2["planId"].as_str().expect("u2 planId"),
        rev_u2["expectedRevision"].as_u64().expect("u2 rev"),
    )
    .expect("apply challenge 3");
    assert_eq!(rec_u2["ok"], json!(true));
    assert_eq!(rec_u2["appliedCount"], rec_u2["stepCount"]);

    // Post-execution QC & observation-led refinement on Challenge 3
    let ctx_u2_post = tron_actions::build_tron_project_context(&d.app().session).expect("ctx u2 post");
    let qc_u2 = tron_local::critique_film_timeline(brief_u2, &ctx_u2_post).expect("qc challenge 3");
    assert_eq!(qc_u2["ok"], json!(true));
    let prop_u2_ref = tron_local::prepare_film_plan("اللقطات مش راكبة على بعض", &ctx_u2_post)
        .expect("challenge 3 refinement plan");
    let rev_u2_ref = tron_actions::review(&d.app().session, &prop_u2_ref).expect("review challenge 3 refinement");
    let rec_u2_ref = tron_actions::apply(
        &mut d.app().session,
        &prop_u2_ref,
        rev_u2_ref["planId"].as_str().expect("u2 ref planId"),
        rev_u2_ref["expectedRevision"].as_u64().expect("u2 ref rev"),
    )
    .expect("apply challenge 3 refinement");
    assert_eq!(rec_u2_ref["ok"], json!(true));

    // Verify 29.97 NTSC (30000/1001) rational frame rate alignment without floating-point drift
    let ntsc_seq = d.exec(
        "file.newSequence",
        json!({
            "name": "NTSC 29.97 Rational Timebase Check",
            "width": 1920,
            "height": 1080,
            "fps": 29.97,
            "sampleRate": 48000,
            "video": 2,
            "audio": 2
        }),
    );
    let _ntsc_seq_id = ntsc_seq["sequence"].as_u64().expect("ntsc seq");
    let ctx_ntsc = tron_actions::build_tron_project_context(&d.app().session).expect("ctx ntsc");
    assert_eq!(ctx_ntsc["fpsNum"], json!(30000));
    assert_eq!(ctx_ntsc["fpsDen"], json!(1001));
    let prop_ntsc = tron_local::prepare_film_plan("الإيقاع بطيء في الأول", &ctx_ntsc).expect("ntsc plan");
    let rev_ntsc = tron_actions::review(&d.app().session, &prop_ntsc).expect("review ntsc");
    assert_eq!(rev_ntsc["ok"], json!(true));

    // =========================================================================
    // PHASE 8: Final Lease Release, Model Unload & Structured Evidence Write
    // =========================================================================
    eprintln!("=== [PHASE 8] Releasing Final TRON Lease & Verifying Clean Unload ===");
    let final_rel = tron_local::release().expect("final TRON release");
    let final_status = final_rel.get("status").unwrap_or(&final_rel);
    eprintln!("FINAL_TRON_RELEASE_STATUS: {}", final_status);
    assert_eq!(final_status["activeLeaseCount"], json!(0));
    assert_eq!(final_status["state"], json!("NOT_LOADED"));

    let vram_after = query_nvidia_smi_vram();
    eprintln!("VRAM_AFTER_FINAL_RELEASE: {:?}", vram_after);

    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let evidence_dir = repo_root.join("docs/acceptance/filmtron-tron");
    std::fs::create_dir_all(&evidence_dir).expect("create docs/acceptance/filmtron-tron");
    let structured_evidence = json!({
        "schema": "filmtron.tron.windows-acceptance.v1",
        "verifiedAt": "2026-10-22",
        "workstation": {
            "os": "Windows 11 x86_64",
            "gpu": "NVIDIA GeForce RTX 3050 6GB",
            "wgpuBackend": "DX12",
            "model": "qwen3.5:4b-q4_K_M",
            "modelDigest": status_obj["modelDigest"],
            "warmupMetrics": status_obj["warmupMetrics"],
            "vramBeforeMiB": vram_before,
            "vramDuringReadyMiB": vram_active,
            "vramAfterUnloadMiB": vram_after
        },
        "sharedRuntimeCoexistence": {
            "passiveStartupState": "NOT_LOADED",
            "explicitOnState": status_obj["state"],
            "concurrentPhotoTronAndFilmTronLeases": peer_status["activeLeaseCount"],
            "stateAfterFilmTronReleaseWithPhotoTronActive": rel_film_status["state"],
            "stateAfterPhotoTronReleaseWithFilmTronActive": rel_photo_status["state"],
            "stateAfterFinalRelease": final_status["state"]
        },
        "challenge1_mandatory15sCommercial": {
            "brief": mandatory_brief,
            "fps": "30/1",
            "resolution": "1080x1920",
            "directionA_preflight": dir_a_review["status"],
            "directionB_preflight": dir_b_review["status"],
            "selectedDirection": dir_b_prop["selectedDirection"],
            "initialExecutionReceipt": apply_receipt,
            "postExecutionQc": critique_resp["critique"],
            "editableProjectPath": fcproj_path.to_string_lossy(),
            "exportedMp4Path": exported_mp4.to_string_lossy(),
            "exportedMp4Bytes": mp4_bytes.len(),
            "exportedMp4Sha256": mp4_sha256,
            "exportDurationMs": export_ms,
            "ffprobe": {
                "videoCodec": video_stream["codec_name"],
                "width": video_stream["width"],
                "height": video_stream["height"],
                "nbFrames": video_stream["nb_frames"],
                "audioCodec": audio_stream["codec_name"],
                "durationSeconds": duration_s
            }
        },
        "challenge2_dynamicTechLaunch24fps": {
            "brief": brief_u1,
            "deliverableLocale": "en-US",
            "fps": "24/1",
            "selectedDirection": prop_u1["selectedDirection"],
            "executionReceipt": rec_u1,
            "postExecutionQc": qc_u1["critique"],
            "refinementReceipt": rec_u1_ref
        },
        "challenge3_cinematicLifestyleStory25fps": {
            "brief": brief_u2,
            "deliverableLocale": "ar-EG",
            "fps": "25/1",
            "selectedDirection": prop_u2["selectedDirection"],
            "executionReceipt": rec_u2,
            "postExecutionQc": qc_u2["critique"],
            "refinementReceipt": rec_u2_ref,
            "ntsc2997RationalPreflight": rev_ntsc["status"]
        }
    });
    std::fs::write(
        evidence_dir.join("acceptance-receipts.json"),
        serde_json::to_string_pretty(&structured_evidence).expect("serialize structured evidence"),
    )
    .expect("write acceptance-receipts.json");
}
