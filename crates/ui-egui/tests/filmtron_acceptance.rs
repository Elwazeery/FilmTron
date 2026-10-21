//! FilmTron 13-Step End-to-End Acceptance Suite & Native GPU Visual Audit (`FILM-007`, `FILM-008`).
//!
//! Exercises the real `FilmcraftApp` + `Session` + `wgpu` offscreen renderer + GemiMCP outbound
//! adapter + GemiGuard offline lease consumer across all 13 required acceptance steps and writes
//! the 7 audited 1600x980 PNG screenshots to `docs/images/filmtron/`.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Sender, channel};
use std::time::Duration;

use egui_kittest::Harness;
use filmcraft_automation::gemiguard::{
    GemiAccessClaims, OFFLINE_LEASE_SCHEMA, OfflineLease, PkceChallenge, REVOCATION_LIST_SCHEMA,
    RevocationList, ed25519_public_key, evaluate_entitlement,
};
use filmcraft_automation::gemimcp::{
    FederatedToolCall, OutboundBridgeAdapter, PRODUCT_ID, SCOPE_EDIT, SCOPE_READ, SCOPE_UI,
    build_product_manifest,
};
use filmcraft_automation::security::{
    AuthorizedWorkspace, authentication_reply, generate_token, hex_encode, sha256,
};
use filmcraft_engine::Session;
use filmcraft_project::ClipId;
use filmcraft_time::Tick;
use filmcraft_ui_egui::FilmcraftApp;
use filmcraft_ui_egui::control::ControlRequest;
use serde_json::{Value, json};

struct AcceptanceDriver {
    harness: Harness<'static, FilmcraftApp>,
    tx: Sender<ControlRequest>,
    work_dir: PathBuf,
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

impl AcceptanceDriver {
    fn launch() -> Self {
        let work_dir = std::env::temp_dir().join(format!("filmtron-acceptance-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&work_dir);
        std::fs::create_dir_all(&work_dir).expect("create acceptance work_dir");

        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("workspace root");
        let screenshot_dir = repo_root.join("docs/images/filmtron");
        std::fs::create_dir_all(&screenshot_dir).expect("create docs/images/filmtron");

        eprintln!(">>> [0.1] Creating Session::default()...");
        let mut session = Session::default();
        session.prefs_path = Some(work_dir.join("preferences.json"));
        session.export_presets.set_dir(&work_dir.join("presets"));

        eprintln!(">>> [0.2] Creating FilmcraftApp (size = {} bytes)...", std::mem::size_of::<FilmcraftApp>());
        let (tx, rx) = channel();
        let app = FilmcraftApp::new(session).with_control(rx);
        eprintln!(">>> [0.3] Building egui_kittest wgpu Harness (DX12)...");
        let harness = Harness::builder()
            .with_size(egui::vec2(1600.0, 980.0))
            .with_max_steps(10_000)
            .wgpu_setup(windows_safe_wgpu_setup())
            .with_pixels_per_point(1.0)
            .build_eframe(move |_cc| {
                eprintln!(">>> [0.3a] Inside build_eframe closure!");
                app
            });

        eprintln!(">>> [0.4] Stepping initial 4 frames...");
        let mut d = Self { harness, tx, work_dir, screenshot_dir };
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

    /// Pump background FrameServer workers and egui textures so monitors, thumbnails, and scopes
    /// are fully rendered before capturing a wgpu screenshot.
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
        assert!(meta.len() > 15_000, "screenshot {} too small ({} bytes)", filename, meta.len());
        eprintln!("SCREENSHOT_SAVED: {} ({} bytes)", out_path.display(), meta.len());
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

impl Drop for AcceptanceDriver {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.work_dir);
    }
}

#[test]
fn filmtron_13_step_acceptance_and_visual_audit() {
    std::thread::Builder::new()
        .name("filmtron-acceptance".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(run_filmtron_13_step_acceptance)
        .expect("spawn 64 MiB acceptance thread")
        .join()
        .expect("acceptance thread succeeded");
}

fn run_filmtron_13_step_acceptance() {
    eprintln!(">>> [0] Launching AcceptanceDriver...");
    let mut d = AcceptanceDriver::launch();
    eprintln!(">>> [1] AcceptanceDriver launched.");

    // =========================================================================
    // STEP 1 & 2: Generate representative media fixtures (H.264+AAC MP4, PNG still,
    // 48 kHz stereo WAV), then create a fresh FilmTron project and import them.
    // =========================================================================
    let fixture_mp4 = d.work_dir.join("fixture_source.mp4");
    let fixture_wav = d.work_dir.join("fixture_audio.wav");
    let fixture_png = d.work_dir.join("fixture_still.png");

    eprintln!(">>> [1.1] Opening demo project...");
    d.exec("file.openDemoProject", json!({}));
    eprintln!(">>> [1.2] Exporting fixture_source.mp4...");
    d.exec(
        "file.exportMedia",
        json!({
            "path": fixture_mp4.to_string_lossy(),
            "format": "h264",
            "audioCodec": "aac",
            "hardwareEncoding": false,
            "gpuRendering": false,
            "width": 320,
            "height": 180,
            "fps": 24.0,
            "range": "custom",
            "startSeconds": 3.5,
            "endSeconds": 4.5,
            "wait": true
        }),
    );
    assert!(fixture_mp4.exists(), "fixture_source.mp4 created");
    eprintln!(">>> [1.3] Exporting fixture_audio.wav...");

    d.exec(
        "file.exportMedia",
        json!({
            "path": fixture_wav.to_string_lossy(),
            "format": "wav",
            "range": "custom",
            "startSeconds": 0.0,
            "endSeconds": 1.0,
            "wait": true
        }),
    );
    assert!(fixture_wav.exists(), "fixture_audio.wav created");
    eprintln!(">>> [1.4] Exporting fixture_still.png...");

    d.exec(
        "file.exportFrame",
        json!({
            "path": fixture_png.to_string_lossy(),
            "format": "png"
        }),
    );
    assert!(fixture_png.exists(), "fixture_still.png created");

    // Step 1: Create fresh FilmTron project
    eprintln!(">>> [1.5] Creating fresh FilmTron project...");
    d.exec("file.newProject", json!({"name": "FilmTron Acceptance Project"}));
    assert_eq!(d.app().session.project.name, "FilmTron Acceptance Project");

    // Step 2: Create a bin and import video (.mp4), still image (.png), and audio (.wav)
    let bin_res = d.exec("file.newBin", json!({"name": "Media Assets"}));
    let bin_id = bin_res["bin"].as_u64().expect("bin id");
    let import_res = d.exec(
        "file.import",
        json!({
            "paths": [
                fixture_mp4.to_string_lossy(),
                fixture_png.to_string_lossy(),
                fixture_wav.to_string_lossy()
            ],
            "bin": bin_id
        }),
    );
    let imported_items: Vec<u64> = import_res["items"]
        .as_array()
        .expect("imported items array")
        .iter()
        .filter_map(Value::as_u64)
        .collect();
    assert_eq!(imported_items.len(), 3, "imported video, still image, and audio: {import_res}");
    let video_item = imported_items[0];
    let still_item = imported_items[1];
    let audio_item = imported_items[2];

    // =========================================================================
    // STEP 3: Create multitrack sequence and place video, image, and audio clips
    // =========================================================================
    let seq_res = d.exec(
        "file.newSequence",
        json!({
            "name": "FilmTron Master Edit",
            "width": 640,
            "height": 360,
            "fps": 24.0,
            "sampleRate": 48000,
            "video": 3,
            "audio": 3
        }),
    );
    let main_seq_id = seq_res["sequence"].as_u64().expect("main sequence id");
    let rate = d.app().session.sequence_rate();
    assert_eq!(rate.num, 24);
    assert_eq!(rate.den, 1);

    // Place video + linked audio on V1/A1 at frame 0 (24 frames) and frame 24 (24 frames)
    d.exec(
        "timeline.place",
        json!({
            "item": video_item,
            "track": "V1",
            "audioTrack": "A1",
            "frame": 0,
            "duration": rate.tick_of(24).0
        }),
    );
    d.exec(
        "timeline.place",
        json!({
            "item": video_item,
            "track": "V1",
            "audioTrack": "A1",
            "frame": 24,
            "duration": rate.tick_of(24).0
        }),
    );
    // Place still image overlay on V2 from frame 4 to frame 20
    d.exec(
        "timeline.place",
        json!({
            "item": still_item,
            "track": "V2",
            "frame": 4,
            "duration": rate.tick_of(16).0
        }),
    );
    // Place music/tone bed on A2 from frame 0 to frame 36
    d.exec(
        "timeline.place",
        json!({
            "item": audio_item,
            "track": "A2",
            "frame": 0,
            "duration": rate.tick_of(36).0
        }),
    );

    // Open the video asset in Source Monitor so Source Monitor displays a live frame
    d.exec("source.open", json!({"item": video_item}));
    d.exec("source.setPlayhead", json!({"frame": 6}));

    // =========================================================================
    // STEP 4: Make frame-accurate cuts and trims
    // =========================================================================
    let v1_before = d.exec("sequence.inspect", json!({}))["video"][0]["items"]
        .as_array()
        .expect("v1 items")
        .len();
    assert_eq!(v1_before, 2);

    // Razor split at exact frame 12 on V1
    d.exec("playhead.set", json!({"frame": 12}));
    let razor_res = d.exec("timeline.razor", json!({"track": "V1"}));
    assert_eq!(razor_res["cuts"], json!(1));

    let seq_after_cut = d.exec("sequence.inspect", json!({}));
    let v1_items = seq_after_cut["video"][0]["items"].as_array().expect("v1 items after cut");
    assert_eq!(v1_items.len(), 3, "V1 split into 3 clips");
    assert_eq!(v1_items[1]["start"].as_i64(), Some(rate.tick_of(12).0));

    // Trim the out-point of the third V1 clip by -4 frames
    let third_clip_id = v1_items[2]["clip"].as_u64().expect("third clip id");
    let third_dur_before = v1_items[2]["duration"].as_i64().expect("third clip duration");
    d.exec(
        "timeline.trim",
        json!({
            "clip": third_clip_id,
            "edge": "out",
            "mode": "regular",
            "deltaFrames": -4
        }),
    );
    let seq_after_trim = d.exec("sequence.inspect", json!({}));
    let third_dur_after = seq_after_trim["video"][0]["items"][2]["duration"]
        .as_i64()
        .expect("trimmed duration");
    assert_eq!(third_dur_after, third_dur_before - rate.tick_of(4).0);

    // =========================================================================
    // STEP 5: Add titles (English + Arabic RTL), transitions, color changes, and audio adjustments
    // =========================================================================
    d.exec("playhead.set", json!({"frame": 6}));
    let title_en = d.exec(
        "graphics.newText",
        json!({
            "text": "FILMTRON PRO STUDIO",
            "position": [110.0, 80.0],
            "size": 42.0,
            "seconds": 1.5
        }),
    );
    let title_clip_id = title_en["clip"].as_u64().expect("title clip id");
    d.exec(
        "graphics.newText",
        json!({
            "clip": title_clip_id,
            "text": "فيلم ترون — مونتاج احترافي",
            "position": [110.0, 310.0],
            "size": 36.0
        }),
    );

    // Apply Cross Dissolve transition at the cut between V1 clip 0 and clip 1
    let v1_clip0 = seq_after_trim["video"][0]["items"][0]["clip"].as_u64().expect("v1 clip 0");
    let v1_clip1 = seq_after_trim["video"][0]["items"][1]["clip"].as_u64().expect("v1 clip 1");
    d.exec("playhead.set", json!({"frame": 12}));
    d.exec("timeline.select", json!({"clips": [v1_clip1]}));
    d.exec("sequence.applyVideoTransition", json!({"effect": "Cross Dissolve"}));

    // Apply Lumetri Color grading on V1 clip 0
    d.exec("timeline.select", json!({"clips": [v1_clip0]}));
    d.exec("effects.apply", json!({"clip": v1_clip0, "effect": "Lumetri Color"}));
    d.exec(
        "effects.setParam",
        json!({
            "clip": v1_clip0,
            "effect": "lumetri",
            "param": "exposure",
            "value": 0.4
        }),
    );
    d.exec(
        "effects.setParam",
        json!({
            "clip": v1_clip0,
            "effect": "lumetri",
            "param": "contrast",
            "value": 15.0
        }),
    );
    d.exec(
        "effects.setParam",
        json!({
            "clip": v1_clip0,
            "effect": "lumetri",
            "param": "saturation",
            "value": 120.0
        }),
    );

    // Adjust clip audio gain on A1 and track mixer fader on A2
    let a1_clip0 = seq_after_trim["audio"][0]["items"][0]["clip"].as_u64().expect("a1 clip 0");
    d.exec("timeline.select", json!({"clips": [a1_clip0]}));
    d.exec("clip.audioGain", json!({"clips": [a1_clip0], "mode": "set", "db": -2.5}));
    d.exec("mixer.setStrip", json!({"strip": "A2", "volumeDb": -3.0, "pan": 15.0}));
    d.exec("mixer.setValue", json!({"strip": "A2", "lane": "volume", "value": -3.0}));

    // =========================================================================
    // STEP 6: Preview real frames and verify sound
    // =========================================================================
    d.exec("playhead.set", json!({"frame": 8}));
    d.exec("timeline.select", json!({"clips": [v1_clip0]}));

    let preview_png = d.work_dir.join("preview_frame_8.png");
    let frame_export = d.exec(
        "file.exportFrame",
        json!({
            "path": preview_png.to_string_lossy(),
            "format": "png"
        }),
    );
    assert_eq!(frame_export["width"], json!(640));
    assert_eq!(frame_export["height"], json!(360));
    let preview_img = image::open(&preview_png).expect("decode preview PNG").into_rgba8();
    let non_black_pixels = preview_img
        .pixels()
        .filter(|px| px[0] > 8 || px[1] > 8 || px[2] > 8)
        .count();
    assert!(
        non_black_pixels > 10_000,
        "expected non-black composite preview frame, got {non_black_pixels} non-black pixels"
    );

    let scopes_json = d.exec("scopes.read", json!({}));
    assert!(
        scopes_json.is_object(),
        "Lumetri scopes computed from real preview frame: {scopes_json}"
    );

    let peak_json = d.exec("clip.audioPeak", json!({"clips": [a1_clip0]}));
    let max_db = peak_json["peakDb"].as_f64().unwrap_or(-120.0);
    assert!(
        max_db > -60.0,
        "expected audible audio peak on A1 clip, got {max_db} dB ({peak_json})"
    );

    // =========================================================================
    // STEP 7: Undo/redo edits
    // =========================================================================
    let rev_before_undo = d.app().session.revision;
    d.exec("edit.undo", json!({}));
    assert!(d.app().session.revision > rev_before_undo);
    d.exec("edit.redo", json!({}));
    let seq_after_redo = d.exec("sequence.inspect", json!({}));
    assert_eq!(seq_after_redo["video"][0]["items"].as_array().expect("v1").len(), 3);

    // =========================================================================
    // STEP 8: Save, close, and reopen without losing timeline state
    // =========================================================================
    let fcproj_path = d.work_dir.join("filmtron_acceptance.fcproj");
    d.exec("file.saveAs", json!({"path": fcproj_path.to_string_lossy()}));
    assert!(fcproj_path.exists(), ".fcproj saved on disk");
    let fcproj_bytes = std::fs::read(&fcproj_path).expect("read .fcproj bytes");
    let loaded_on_disk = filmcraft_format::decode(&fcproj_bytes).expect("decode .fcproj");
    assert_eq!(loaded_on_disk.project.name, "filmtron_acceptance");

    // Reset to empty project, then reopen saved .fcproj
    d.exec("file.newProject", json!({"name": "Temporary Blank"}));
    assert_eq!(d.app().session.project.name, "Temporary Blank");
    d.exec("file.open", json!({"path": fcproj_path.to_string_lossy()}));
    assert_eq!(d.app().session.project.name, "filmtron_acceptance");
    let reopened_seq = d.exec("sequence.inspect", json!({}));
    assert_eq!(reopened_seq["video"][0]["items"].as_array().expect("reopened v1").len(), 3);
    assert_eq!(
        reopened_seq["video"][0]["transitions"].as_array().expect("reopened transitions").len(),
        2
    );
    let reopened_graphics = d.exec("graphics.list", json!({"clip": title_clip_id}));
    let layer_texts: Vec<&str> = reopened_graphics["layers"]
        .as_array()
        .expect("reopened graphic layers")
        .iter()
        .filter_map(|l| l["text"].as_str())
        .collect();
    assert!(layer_texts.contains(&"FILMTRON PRO STUDIO"), "{layer_texts:?}");
    assert!(layer_texts.contains(&"فيلم ترون — مونتاج احترافي"), "{layer_texts:?}");

    // Re-open source item in Source Monitor and position playhead on the titled/graded frame
    d.exec("source.open", json!({"item": video_item}));
    d.exec("source.setPlayhead", json!({"frame": 6}));
    d.exec("playhead.set", json!({"frame": 8}));
    d.exec("timeline.select", json!({"clips": [v1_clip0]}));

    // =========================================================================
    // STEP 9 & 10: Export a real H.264/AAC MP4 video, then decode it back and
    // verify timing, visuals, and audio
    // =========================================================================
    let exported_mp4 = d.work_dir.join("filmtron_final_export.mp4");
    let export_out = d.exec(
        "file.exportMedia",
        json!({
            "path": exported_mp4.to_string_lossy(),
            "format": "h264",
            "audioCodec": "aac",
            "hardwareEncoding": false,
            "gpuRendering": false,
            "width": 640,
            "height": 360,
            "fps": 24.0,
            "wait": true
        }),
    );
    assert!(exported_mp4.exists(), "exported MP4 exists: {export_out}");
    let mp4_bytes = std::fs::read(&exported_mp4).expect("read exported MP4");
    let mp4_sha256 = hex_encode(&sha256(&mp4_bytes));
    assert!(mp4_bytes.len() > 8_000, "exported MP4 size = {} bytes", mp4_bytes.len());
    eprintln!(
        "ACCEPTANCE_EXPORT_RECEIPT: path={} bytes={} sha256={}",
        exported_mp4.display(),
        mp4_bytes.len(),
        mp4_sha256
    );

    // Step 10: Decode exported MP4 back through FilmTron's media + render pipeline
    let verify_import = d.exec(
        "file.import",
        json!({
            "paths": [exported_mp4.to_string_lossy()]
        }),
    );
    let decoded_item_id = verify_import["items"][0].as_u64().expect("decoded MP4 item id");
    d.exec("project.select", json!({"items": [decoded_item_id]}));
    let verify_seq = d.exec("file.newSequenceFromClip", json!({"item": decoded_item_id}));
    let verify_seq_id = verify_seq["sequence"].as_u64().expect("verify sequence id");
    {
        let seq = d
            .app()
            .session
            .project
            .sequence(filmcraft_project::ItemId(verify_seq_id))
            .expect("verify sequence");
        assert_eq!(seq.settings.width, 640);
        assert_eq!(seq.settings.height, 360);
        assert!(seq.duration() >= rate.tick_of(40), "decoded duration = {:?}", seq.duration());
        assert!(seq.duration() > Tick::ZERO);
    }

    d.exec("playhead.set", json!({"frame": 8}));
    let decoded_frame_png = d.work_dir.join("decoded_export_frame_8.png");
    d.exec(
        "file.exportFrame",
        json!({
            "path": decoded_frame_png.to_string_lossy(),
            "format": "png"
        }),
    );
    let decoded_img = image::open(&decoded_frame_png).expect("decode exported frame PNG").into_rgba8();
    let decoded_non_black = decoded_img
        .pixels()
        .filter(|px| px[0] > 8 || px[1] > 8 || px[2] > 8)
        .count();
    assert!(
        decoded_non_black > 10_000,
        "decoded MP4 frame is non-black ({decoded_non_black} pixels)"
    );

    let verify_seq_json = d.exec("sequence.inspect", json!({}));
    let verify_a1_clip = verify_seq_json["audio"][0]["items"][0]["clip"]
        .as_u64()
        .expect("decoded A1 clip");
    let decoded_peak_json = d.exec("clip.audioPeak", json!({"clips": [verify_a1_clip]}));
    let decoded_max_db = decoded_peak_json["peakDb"].as_f64().unwrap_or(-120.0);
    assert!(
        decoded_max_db > -60.0,
        "decoded MP4 audio is non-silent (peakDb = {decoded_max_db})"
    );

    // Switch back to the main sequence for Step 11-13
    d.exec("sequence.open", json!({"item": main_seq_id}));
    d.exec("sequence.close", json!({"item": verify_seq_id}));
    d.exec("source.open", json!({"item": video_item}));
    d.exec("source.setPlayhead", json!({"frame": 6}));
    d.exec("playhead.set", json!({"frame": 8}));
    d.exec("timeline.select", json!({"clips": [ClipId(v1_clip0).0]}));

    // =========================================================================
    // STEP 11 & 12: Exercise GemiMCP parity, native control security, scope denial,
    // workspace confinement, revision fences, idempotency, and GemiGuard offline lease
    // =========================================================================
    let manifest = build_product_manifest();
    assert!(manifest.federation.outbound_only);
    assert!(manifest.federation.no_public_local_listeners);
    assert_eq!(manifest.engine_command_tool_count, filmcraft_engine::command_specs().len());

    // Unauthenticated control request is denied before dispatch
    let token = generate_token().expect("generate CSPRNG token");
    let (unauth, authed) = authentication_reply(r#"{"id": 1, "method": "engine.execute"}"#, &token);
    assert!(!authed);
    assert_eq!(unauth["ok"], false);
    assert_eq!(unauth["error"], "authentication required");

    let workspace = AuthorizedWorkspace::new(vec![d.work_dir.clone()]).expect("workspace");
    let mut adapter = OutboundBridgeAdapter::new(Some(workspace));
    let granted_scopes = vec![SCOPE_READ.to_string(), SCOPE_EDIT.to_string(), SCOPE_UI.to_string()];

    // Execute semantic tool `film.timeline.add_marker` with revision fence + idempotency key
    let rev0 = d.app().session.revision;
    let marker_call = FederatedToolCall {
        call_id: "accept-1".into(),
        tool_name: "film.timeline.add_marker".into(),
        arguments: json!({
            "name": "Cloud MCP Cut",
            "comments": "Verified via OutboundBridgeAdapter",
            "expectedRevision": rev0,
            "idempotencyKey": "idem-accept-marker-1"
        }),
        granted_scopes: granted_scopes.clone(),
    };
    let marker_res = adapter.execute_on_session(&mut d.app().session, &marker_call);
    assert_eq!(marker_res["ok"], true, "{marker_res}");
    let rev1 = d.app().session.revision;
    assert!(rev1 > rev0);

    // Idempotent replay with same key & payload returns cached result without mutating
    let replay_res = adapter.execute_on_session(&mut d.app().session, &marker_call);
    assert_eq!(replay_res["ok"], true);
    assert_eq!(replay_res["idempotentReplay"], true);
    assert_eq!(d.app().session.revision, rev1);

    // Idempotency conflict when reusing key with different payload
    let conflict_idem = FederatedToolCall {
        call_id: "accept-2".into(),
        tool_name: "film.timeline.add_marker".into(),
        arguments: json!({
            "name": "Tampered Payload",
            "idempotencyKey": "idem-accept-marker-1"
        }),
        granted_scopes: granted_scopes.clone(),
    };
    let conflict_idem_res = adapter.execute_on_session(&mut d.app().session, &conflict_idem);
    assert_eq!(conflict_idem_res["ok"], false);
    assert_eq!(conflict_idem_res["code"], "IDEMPOTENCY_CONFLICT");

    // Stale expectedRevision returns REVISION_CONFLICT
    let stale_call = FederatedToolCall {
        call_id: "accept-3".into(),
        tool_name: "film.timeline.add_marker".into(),
        arguments: json!({
            "name": "Stale",
            "expectedRevision": rev0
        }),
        granted_scopes: granted_scopes.clone(),
    };
    let stale_res = adapter.execute_on_session(&mut d.app().session, &stale_call);
    assert_eq!(stale_res["ok"], false);
    assert_eq!(stale_res["code"], "REVISION_CONFLICT");

    // Missing `filmtron:render` scope returns SCOPE_DENIED
    let scope_denied_call = FederatedToolCall {
        call_id: "accept-4".into(),
        tool_name: "film.render.export_media".into(),
        arguments: json!({"path": d.work_dir.join("denied.mp4").to_string_lossy()}),
        granted_scopes: granted_scopes.clone(),
    };
    let scope_denied_res = adapter.execute_on_session(&mut d.app().session, &scope_denied_call);
    assert_eq!(scope_denied_res["ok"], false);
    assert_eq!(scope_denied_res["code"], "SCOPE_DENIED");

    // Out-of-workspace path returns PATH_OUTSIDE_WORKSPACE
    let escape_call = FederatedToolCall {
        call_id: "accept-5".into(),
        tool_name: "film.project.open".into(),
        arguments: json!({"path": "../outside_secret.fcproj"}),
        granted_scopes,
    };
    let escape_res = adapter.execute_on_session(&mut d.app().session, &escape_call);
    assert_eq!(escape_res["ok"], false);
    assert_eq!(escape_res["code"], "PATH_OUTSIDE_WORKSPACE");

    // Gemi Access PKCE & GemiGuard Ed25519 Offline Lease + Revocation graceful fallback
    let pkce = PkceChallenge::generate().expect("PKCE generate");
    assert_eq!(pkce.code_challenge_method, "S256");
    let signing_key = [0x5a_u8; 32];
    let pub_key = ed25519_public_key(&signing_key);
    let now_unix = 1_780_000_000_u64;
    let mut lease = OfflineLease {
        schema_version: OFFLINE_LEASE_SCHEMA.into(),
        lease_id: "lease-accept-01".into(),
        product_id: PRODUCT_ID.into(),
        subject_id: "usr_filmtron_qa".into(),
        device_id: "dev_win_workstation".into(),
        tier: "pro".into(),
        scopes: vec![SCOPE_READ.into(), SCOPE_EDIT.into()],
        issued_at_unix: now_unix - 120,
        expires_at_unix: now_unix + 86_400,
        offline_grace_seconds: 604_800,
        preserve_user_project_data: true,
        signature_hex: String::new(),
    };
    lease.sign(&signing_key);

    let claims = GemiAccessClaims {
        iss: "https://auth.gemitron.net".into(),
        sub: "usr_filmtron_qa".into(),
        aud: vec!["filmtron-desktop".into()],
        exp: now_unix + 3600,
        iat: now_unix - 60,
        device_id: "dev_win_workstation".into(),
        scopes: vec![SCOPE_READ.into(), SCOPE_EDIT.into()],
    };
    assert!(claims.validate(now_unix, Some(SCOPE_EDIT)).is_ok());

    let valid_decision = evaluate_entitlement(&lease, &pub_key, "dev_win_workstation", now_unix, None);
    assert!(valid_decision.cloud_mcp_allowed);
    assert!(valid_decision.local_editing_allowed);

    let mut revocations = RevocationList {
        schema_version: REVOCATION_LIST_SCHEMA.into(),
        sequence_number: 42,
        issued_at_unix: now_unix - 10,
        revoked_lease_ids: vec!["lease-accept-01".into()],
        revoked_device_ids: vec!["dev_win_workstation".into()],
        signature_hex: String::new(),
    };
    revocations.sign(&signing_key);
    let revoked_decision = evaluate_entitlement(
        &lease,
        &pub_key,
        "dev_win_workstation",
        now_unix,
        Some(&revocations),
    );
    assert!(!revoked_decision.cloud_mcp_allowed, "revoked lease disables cloud MCP");
    assert!(revoked_decision.local_editing_allowed, "offline local editing remains enabled");
    assert!(revoked_decision.preserve_user_project_data, "user project is never corrupted");

    // =========================================================================
    // STEP 13: Screen-by-screen Premium UI/UX Visual Audit (7 real wgpu PNGs)
    // =========================================================================
    // 1. Editing workspace — Inferno Pixel Glass (Dark)
    d.ok("ui.set", json!({"mode": "edit", "workspace": "Editing", "theme": "inferno-dark"}));
    d.settle_and_save_screenshot("01-editing-inferno-dark.png");

    // 2. Editing workspace — Inferno Pixel Glass (Light)
    d.ok("ui.set", json!({"theme": "inferno-light"}));
    d.settle_and_save_screenshot("02-editing-inferno-light.png");

    // Switch back to Inferno Pixel Glass (Dark) for the remaining workspaces
    d.ok("ui.set", json!({"theme": "inferno-dark"}));

    // 3. Color workspace with Lumetri Color & neutral-gray Scopes
    d.ok("ui.set", json!({"workspace": "Color"}));
    d.settle_and_save_screenshot("03-color-workspace-scopes.png");

    // 4. Audio workspace with Audio Track Mixer, Meters & Essential Sound
    d.ok("ui.set", json!({"workspace": "Audio"}));
    d.settle_and_save_screenshot("04-audio-workspace-mixer.png");

    // 5. Export workspace showing H.264/AAC settings, summary & queue
    d.ok("ui.set", json!({"mode": "export"}));
    d.settle_and_save_screenshot("05-export-workspace.png");

    // 6. About FilmTron modal dialog
    d.ok("ui.set", json!({"mode": "edit", "workspace": "Editing"}));
    d.app().dialog = Some(filmcraft_ui_egui::Dialog::About);
    d.settle_and_save_screenshot("06-about-filmtron-dialog.png");
    d.app().dialog = None;
    d.frames(2);

    // 7. Settings ▸ Appearance modal dialog showing Inferno Pixel Glass theme selector
    filmcraft_ui_egui::panels::settings::open(d.app(), "appearance")
        .expect("open Settings ▸ Appearance dialog");
    d.settle_and_save_screenshot("07-preferences-appearance-dialog.png");
    d.app().dialog = None;
    d.frames(2);
}
