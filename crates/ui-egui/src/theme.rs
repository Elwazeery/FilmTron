//! Design tokens. Every widget reads colours/sizes from [`Tokens`] so themes apply everywhere.
//!
//! The default theme follows the look of Premiere Pro's current dark UI (values measured from
//! black-box screenshots, see `plan/premiere/02-ui-ux.md`); fonts are Inter + JetBrains Mono (OFL).

use std::sync::Arc;

use egui::{Color32, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle, Visuals};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeKind {
    /// Premiere-style darkest (default).
    #[default]
    Dark,
    /// Slightly lighter grey panels (Premiere's brightness slider mid position).
    Medium,
    Light,
    /// GemiTokens Inferno Pixel Glass (Dark) — Obsidian Glass surfaces + Inferno Crimson/Gold accents.
    InfernoDark,
    /// GemiTokens Inferno Pixel Glass (Light) — Warm Ivory surfaces + Crimson accents.
    InfernoLight,
}

impl ThemeKind {
    pub const ALL: [ThemeKind; 5] = [ThemeKind::Dark, ThemeKind::Medium, ThemeKind::Light, ThemeKind::InfernoDark, ThemeKind::InfernoLight];

    pub fn is_dark(self) -> bool {
        !matches!(self, ThemeKind::Light | ThemeKind::InfernoLight)
    }

    pub fn label(self) -> &'static str {
        match self {
            ThemeKind::Dark => "Darkest",
            ThemeKind::Medium => "Dark",
            ThemeKind::Light => "Light",
            ThemeKind::InfernoDark => "Inferno Pixel Glass (Dark)",
            ThemeKind::InfernoLight => "Inferno Pixel Glass (Light)",
        }
    }

    /// Settings ▸ Appearance ▸ Color Theme value (`darkest`, `dark`, `light`, `infernoDark`, `infernoLight`).
    pub fn pref_name(self) -> &'static str {
        match self {
            ThemeKind::Dark => "darkest",
            ThemeKind::Medium => "dark",
            ThemeKind::Light => "light",
            ThemeKind::InfernoDark => "infernoDark",
            ThemeKind::InfernoLight => "infernoLight",
        }
    }
    pub fn from_pref(s: &str) -> ThemeKind {
        match s {
            "dark" => ThemeKind::Medium,
            "light" => ThemeKind::Light,
            "infernoDark" | "inferno_dark" | "inferno-dark" => ThemeKind::InfernoDark,
            "infernoLight" | "inferno_light" | "inferno-light" => ThemeKind::InfernoLight,
            _ => ThemeKind::Dark,
        }
    }
    pub fn from_name(s: &str) -> Option<ThemeKind> {
        match s.to_ascii_lowercase().as_str() {
            "dark" | "darkest" => Some(ThemeKind::Dark),
            "medium" | "grey" | "gray" => Some(ThemeKind::Medium),
            "light" => Some(ThemeKind::Light),
            "infernodark" | "inferno_dark" | "inferno-dark" => Some(ThemeKind::InfernoDark),
            "infernolight" | "inferno_light" | "inferno-light" => Some(ThemeKind::InfernoLight),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub kind: ThemeKind,
    /// Space between panels / app background.
    pub app_bg: Color32,
    /// Header bar (top).
    pub header_bg: Color32,
    /// Panel body.
    pub panel_bg: Color32,
    /// Panel tab strip.
    pub tab_bg: Color32,
    pub tab_text: Color32,
    pub tab_text_active: Color32,
    /// Blue focus rectangle around the active panel.
    pub focus: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub text_faint: Color32,
    pub icon: Color32,
    pub icon_active: Color32,
    pub hover: Color32,
    pub pressed: Color32,
    pub field_bg: Color32,
    pub field_border: Color32,
    pub separator: Color32,
    pub row_alt: Color32,
    pub row_selected: Color32,
    /// Scrubby (hot-text) numeric value colour.
    pub hot_text: Color32,
    /// Selected pills and compact toggle backgrounds.
    pub pill_active_bg: Color32,
    /// The selected clip bar in Effect Controls.
    pub clip_bar_bg: Color32,
    /// Raised audio control surfaces, such as mixer channel strips.
    pub control_surface: Color32,
    /// Recessed audio control tracks and panner wells.
    pub control_well: Color32,
    pub control_border: Color32,
    pub control_handle_bg: Color32,
    pub control_handle: Color32,
    pub control_handle_dim: Color32,
    /// Data-plot surfaces (keyframe, EQ and dynamics graphs), distinct from media canvases.
    pub plot_bg: Color32,
    pub plot_grid: Color32,
    pub plot_axis: Color32,
    pub plot_handle: Color32,
    pub plot_handle_dim: Color32,
    /// Meter wells and the separate loudness readout surface.
    pub meter_bg: Color32,
    pub meter_readout_bg: Color32,
    pub meter_clip_off: Color32,
    pub panner_bg: Color32,
    pub panner_border: Color32,
    pub panner_speaker: Color32,
    pub fader_knob: Color32,
    pub eq_track: Color32,
    pub eq_tick: Color32,
    pub eq_knob: Color32,
    pub eq_knob_active: Color32,
    pub eq_node: Color32,
    pub eq_node_off: Color32,
    pub crossover_handle: Color32,
    pub keyframe_plot_bg: Color32,
    pub keyframe_plot_grid: Color32,
    pub keyframe_plot_axis: Color32,
    pub keyframe_handle: Color32,
    pub switch_fill: Color32,
    pub switch_knob: Color32,
    pub switch_border: Color32,
    pub slider_disabled_track: Color32,
    pub slider_disabled_knob: Color32,
    pub button_hover: Color32,
    pub button_border: Color32,
    pub caption_track_bg: Color32,
    pub scroll_thumb: Color32,
    pub scroll_thumb_hover: Color32,
    pub vertical_scroll_thumb: Color32,
    pub disabled_clip_bg: Color32,
    // timeline
    pub tl_bg: Color32,
    pub tl_track_bg: Color32,
    pub tl_track_bg_alt: Color32,
    pub tl_header_bg: Color32,
    pub tl_ruler_bg: Color32,
    pub tl_ruler_tick: Color32,
    pub tl_ruler_text: Color32,
    pub playhead: Color32,
    /// Fill of a track header's source-patch / track-targeting button while it is on.
    pub track_target: Color32,
    pub in_out_shade: Color32,
    pub clip_selected_border: Color32,
    pub render_red: Color32,
    pub render_yellow: Color32,
    pub render_green: Color32,
    pub monitor_bg: Color32,
    pub timecode: Color32,
    pub danger: Color32,
    pub radius: f32,
    pub radius_sm: f32,
    pub gap: f32,
    pub tab_h: f32,
}

impl Tokens {
    pub fn for_kind(kind: ThemeKind) -> Self {
        // Measured from Premiere 26 "Darkest" (plan/premiere/02-ui-ux.md §1).
        let dark = Tokens {
            kind,
            app_bg: Color32::from_rgb(0, 0, 0),
            header_bg: Color32::from_rgb(0x1d, 0x1d, 0x1d),
            panel_bg: Color32::from_rgb(0x1d, 0x1d, 0x1d),
            tab_bg: Color32::from_rgb(0x1d, 0x1d, 0x1d),
            tab_text: Color32::from_rgb(0xb0, 0xb0, 0xb0),
            tab_text_active: Color32::from_rgb(0xd1, 0xd1, 0xd1),
            focus: Color32::from_rgb(0x57, 0x94, 0xec),
            accent: Color32::from_rgb(0x2f, 0x6b, 0xdf),
            accent_hover: Color32::from_rgb(0x3f, 0x7c, 0xe8),
            text: Color32::from_rgb(0xd1, 0xd1, 0xd1),
            text_dim: Color32::from_rgb(0xb0, 0xb0, 0xb0),
            text_faint: Color32::from_rgb(0x6e, 0x6e, 0x6e),
            icon: Color32::from_rgb(0xb0, 0xb0, 0xb0),
            icon_active: Color32::from_rgb(0xd1, 0xd1, 0xd1),
            hover: Color32::from_rgb(0x2c, 0x2c, 0x2c),
            pressed: Color32::from_rgb(0x4b, 0x4b, 0x4b),
            field_bg: Color32::from_rgb(0x0e, 0x0e, 0x0e),
            field_border: Color32::from_rgb(0x30, 0x30, 0x30),
            separator: Color32::from_rgb(0x30, 0x30, 0x30),
            row_alt: Color32::from_rgb(0x21, 0x21, 0x21),
            row_selected: Color32::from_rgb(0x33, 0x33, 0x33),
            hot_text: Color32::from_rgb(0x40, 0x96, 0xf3),
            pill_active_bg: Color32::from_rgb(0x3a, 0x3a, 0x3a),
            clip_bar_bg: Color32::from_rgb(58, 58, 70),
            control_surface: Color32::from_rgb(0x24, 0x24, 0x24),
            control_well: Color32::from_rgb(0x10, 0x10, 0x10),
            control_border: Color32::from_rgb(0x50, 0x50, 0x50),
            control_handle_bg: Color32::from_rgb(0x2a, 0x2a, 0x2a),
            control_handle: Color32::from_rgb(0xe0, 0xe0, 0xe0),
            control_handle_dim: Color32::from_rgb(0xb0, 0xb0, 0xb0),
            plot_bg: Color32::from_rgb(0x18, 0x18, 0x18),
            plot_grid: Color32::from_rgb(0x2c, 0x2c, 0x2c),
            plot_axis: Color32::from_rgb(0x44, 0x44, 0x44),
            plot_handle: Color32::from_rgb(0xe0, 0xe0, 0xe0),
            plot_handle_dim: Color32::from_rgb(0x90, 0x90, 0x90),
            meter_bg: Color32::BLACK,
            meter_readout_bg: Color32::from_rgb(0x14, 0x14, 0x14),
            meter_clip_off: Color32::from_rgb(0x30, 0x30, 0x30),
            panner_bg: Color32::from_gray(22),
            panner_border: Color32::from_gray(96),
            panner_speaker: Color32::from_gray(144),
            fader_knob: Color32::from_gray(90),
            eq_track: Color32::from_gray(48),
            eq_tick: Color32::from_gray(96),
            eq_knob: Color32::from_gray(192),
            eq_knob_active: Color32::from_gray(240),
            eq_node: Color32::from_gray(232),
            eq_node_off: Color32::from_gray(112),
            crossover_handle: Color32::from_gray(255),
            keyframe_plot_bg: Color32::from_gray(25),
            keyframe_plot_grid: Color32::from_gray(42),
            keyframe_plot_axis: Color32::from_gray(51),
            keyframe_handle: Color32::from_gray(208),
            switch_fill: Color32::from_gray(212),
            switch_knob: Color32::from_gray(29),
            switch_border: Color32::from_gray(138),
            slider_disabled_track: Color32::from_gray(74),
            slider_disabled_knob: Color32::from_gray(106),
            button_hover: Color32::from_gray(42),
            button_border: Color32::from_gray(75),
            caption_track_bg: Color32::from_rgb(0x23, 0x20, 0x2a),
            scroll_thumb: Color32::from_gray(0x4b),
            scroll_thumb_hover: Color32::from_gray(0x6a),
            vertical_scroll_thumb: Color32::from_gray(80),
            disabled_clip_bg: Color32::from_gray(0x2a),
            tl_bg: Color32::from_rgb(0x1d, 0x1d, 0x1d),
            tl_track_bg: Color32::from_rgb(0x1d, 0x1d, 0x1d),
            tl_track_bg_alt: Color32::from_rgb(0x1d, 0x1d, 0x1d),
            tl_header_bg: Color32::from_rgb(0x1d, 0x1d, 0x1d),
            tl_ruler_bg: Color32::from_rgb(0x1d, 0x1d, 0x1d),
            tl_ruler_tick: Color32::from_rgb(0x8d, 0x8d, 0x8d),
            tl_ruler_text: Color32::from_rgb(0xb0, 0xb0, 0xb0),
            playhead: Color32::from_rgb(0x58, 0x95, 0xec),
            track_target: Color32::from_rgb(0x26, 0x5b, 0xc1),
            in_out_shade: Color32::from_rgb(0x3f, 0x3f, 0x3f),
            clip_selected_border: Color32::from_rgb(0xeb, 0xeb, 0xeb),
            render_red: Color32::from_rgb(0xe3, 0x48, 0x50),
            render_yellow: Color32::from_rgb(0xf0, 0xf0, 0x4f),
            render_green: Color32::from_rgb(0x2d, 0x9d, 0x78),
            monitor_bg: Color32::from_rgb(0x1d, 0x1d, 0x1d),
            timecode: Color32::from_rgb(0x40, 0x96, 0xf3),
            danger: Color32::from_rgb(0xdc, 0x51, 0x3d),
            radius: 0.0,
            radius_sm: 4.0,
            gap: 4.0,
            tab_h: 32.0,
        };
        match kind {
            ThemeKind::Dark => dark,
            ThemeKind::Medium => Tokens {
                app_bg: Color32::from_rgb(20, 20, 20),
                header_bg: Color32::from_rgb(0x32, 0x32, 0x32),
                panel_bg: Color32::from_rgb(0x32, 0x32, 0x32),
                tab_bg: Color32::from_rgb(0x32, 0x32, 0x32),
                field_bg: Color32::from_rgb(0x22, 0x22, 0x22),
                tl_bg: Color32::from_rgb(0x32, 0x32, 0x32),
                tl_track_bg: Color32::from_rgb(0x32, 0x32, 0x32),
                tl_track_bg_alt: Color32::from_rgb(0x32, 0x32, 0x32),
                tl_header_bg: Color32::from_rgb(0x32, 0x32, 0x32),
                tl_ruler_bg: Color32::from_rgb(0x32, 0x32, 0x32),
                monitor_bg: Color32::from_rgb(0x32, 0x32, 0x32),
                row_alt: Color32::from_rgb(50, 50, 50),
                hover: Color32::from_rgb(64, 64, 64),
                ..dark
            },
            ThemeKind::Light => Tokens {
                app_bg: Color32::from_rgb(180, 180, 180),
                header_bg: Color32::from_rgb(214, 214, 214),
                panel_bg: Color32::from_rgb(232, 232, 232),
                tab_bg: Color32::from_rgb(232, 232, 232),
                tab_text: Color32::from_rgb(90, 90, 90),
                tab_text_active: Color32::from_rgb(20, 20, 20),
                text: Color32::from_rgb(34, 34, 34),
                text_dim: Color32::from_rgb(90, 90, 90),
                text_faint: Color32::from_rgb(140, 140, 140),
                icon: Color32::from_rgb(60, 60, 60),
                hover: Color32::from_rgb(210, 210, 210),
                pressed: Color32::from_rgb(196, 196, 196),
                field_bg: Color32::from_rgb(250, 250, 250),
                field_border: Color32::from_rgb(170, 170, 170),
                separator: Color32::from_rgb(200, 200, 200),
                row_alt: Color32::from_rgb(224, 224, 224),
                row_selected: Color32::from_rgb(170, 200, 240),
                pill_active_bg: Color32::from_rgb(196, 196, 196),
                clip_bar_bg: Color32::from_rgb(190, 203, 224),
                control_surface: Color32::from_rgb(218, 218, 218),
                control_well: Color32::from_rgb(196, 196, 196),
                control_border: Color32::from_rgb(150, 150, 150),
                control_handle_bg: Color32::from_rgb(205, 205, 205),
                control_handle: Color32::from_rgb(50, 50, 50),
                control_handle_dim: Color32::from_rgb(85, 85, 85),
                plot_bg: Color32::from_rgb(245, 245, 245),
                plot_grid: Color32::from_rgb(210, 210, 210),
                plot_axis: Color32::from_rgb(170, 170, 170),
                plot_handle: Color32::from_rgb(50, 50, 50),
                plot_handle_dim: Color32::from_rgb(100, 100, 100),
                meter_bg: Color32::from_rgb(205, 205, 205),
                meter_readout_bg: Color32::from_rgb(218, 218, 218),
                meter_clip_off: Color32::from_rgb(155, 155, 155),
                panner_bg: Color32::from_gray(196),
                panner_border: Color32::from_gray(150),
                panner_speaker: Color32::from_gray(100),
                fader_knob: Color32::from_gray(140),
                eq_track: Color32::from_gray(196),
                eq_tick: Color32::from_gray(150),
                eq_knob: Color32::from_gray(85),
                eq_knob_active: Color32::from_gray(34),
                eq_node: Color32::from_gray(50),
                eq_node_off: Color32::from_gray(140),
                crossover_handle: Color32::from_gray(50),
                keyframe_plot_bg: Color32::from_gray(245),
                keyframe_plot_grid: Color32::from_gray(210),
                keyframe_plot_axis: Color32::from_gray(170),
                keyframe_handle: Color32::from_gray(50),
                switch_fill: Color32::from_gray(60),
                switch_knob: Color32::from_gray(232),
                switch_border: Color32::from_gray(110),
                slider_disabled_track: Color32::from_gray(190),
                slider_disabled_knob: Color32::from_gray(140),
                button_hover: Color32::from_gray(210),
                button_border: Color32::from_gray(170),
                icon_active: Color32::from_rgb(34, 34, 34),
                caption_track_bg: Color32::from_rgb(222, 215, 233),
                scroll_thumb: Color32::from_gray(150),
                scroll_thumb_hover: Color32::from_gray(130),
                vertical_scroll_thumb: Color32::from_gray(150),
                disabled_clip_bg: Color32::from_gray(190),
                in_out_shade: Color32::from_gray(185),
                tl_bg: Color32::from_rgb(210, 210, 210),
                tl_track_bg: Color32::from_rgb(222, 222, 222),
                tl_track_bg_alt: Color32::from_rgb(228, 228, 228),
                tl_header_bg: Color32::from_rgb(214, 214, 214),
                tl_ruler_bg: Color32::from_rgb(226, 226, 226),
                tl_ruler_text: Color32::from_rgb(80, 80, 80),
                ..dark
            },
            ThemeKind::InfernoDark => Tokens {
                kind,
                app_bg: Color32::from_rgb(0x08, 0x07, 0x0c),
                header_bg: Color32::from_rgb(0x0e, 0x0c, 0x14),
                panel_bg: Color32::from_rgb(0x13, 0x11, 0x1c),
                tab_bg: Color32::from_rgb(0x0b, 0x0a, 0x10),
                tab_text: Color32::from_rgb(0x9e, 0x98, 0xaa),
                tab_text_active: Color32::WHITE,
                focus: Color32::from_rgb(0xff, 0xd4, 0x37),
                accent: Color32::from_rgb(0xf8, 0x39, 0x42),
                accent_hover: Color32::from_rgb(0xff, 0x6b, 0x2b),
                text: Color32::from_rgb(0xf5, 0xf2, 0xeb),
                text_dim: Color32::from_rgb(0xb5, 0xb0, 0xbf),
                text_faint: Color32::from_rgb(0x78, 0x72, 0x85),
                icon: Color32::from_rgb(0xcf, 0xca, 0xd8),
                icon_active: Color32::WHITE,
                hover: Color32::from_rgb(0x1e, 0x1a, 0x2b),
                pressed: Color32::from_rgb(0x2a, 0x24, 0x3b),
                field_bg: Color32::from_rgb(0x0b, 0x0a, 0x10),
                field_border: Color32::from_rgb(0x26, 0x22, 0x35),
                separator: Color32::from_rgb(0x1f, 0x1b, 0x2c),
                row_alt: Color32::from_rgb(0x17, 0x14, 0x22),
                row_selected: Color32::from_rgb(0x3d, 0x16, 0x22),
                hot_text: Color32::from_rgb(0xff, 0xd4, 0x37),
                pill_active_bg: Color32::from_rgb(0x3d, 0x16, 0x22),
                clip_bar_bg: Color32::from_rgb(0x2a, 0x18, 0x26),
                // Strictly color-neutral (R == G == B) for video frames & scopes
                plot_bg: Color32::from_rgb(0x10, 0x10, 0x10),
                monitor_bg: Color32::from_rgb(0x0c, 0x0c, 0x0c),
                tl_bg: Color32::from_rgb(0x0d, 0x0b, 0x12),
                tl_track_bg: Color32::from_rgb(0x13, 0x11, 0x1c),
                tl_track_bg_alt: Color32::from_rgb(0x10, 0x0e, 0x18),
                tl_header_bg: Color32::from_rgb(0x17, 0x14, 0x22),
                tl_ruler_bg: Color32::from_rgb(0x0b, 0x0a, 0x10),
                tl_ruler_tick: Color32::from_rgb(0x6a, 0x64, 0x78),
                tl_ruler_text: Color32::from_rgb(0xb5, 0xb0, 0xbf),
                playhead: Color32::from_rgb(0xf8, 0x39, 0x42),
                track_target: Color32::from_rgb(0xc8, 0x1e, 0x32),
                timecode: Color32::from_rgb(0xff, 0xd4, 0x37),
                ..dark
            },
            ThemeKind::InfernoLight => Tokens {
                kind,
                app_bg: Color32::from_rgb(0xf7, 0xf4, 0xee),
                header_bg: Color32::from_rgb(0xef, 0xec, 0xe4),
                panel_bg: Color32::from_rgb(0xfa, 0xf8, 0xf3),
                tab_bg: Color32::from_rgb(0xe8, 0xe4, 0xda),
                tab_text: Color32::from_rgb(0x54, 0x4f, 0x60),
                tab_text_active: Color32::from_rgb(0x10, 0x0e, 0x16),
                focus: Color32::from_rgb(0xc8, 0x1e, 0x32),
                accent: Color32::from_rgb(0xc8, 0x1e, 0x32),
                accent_hover: Color32::from_rgb(0xa8, 0x15, 0x26),
                text: Color32::from_rgb(0x14, 0x12, 0x1a),
                text_dim: Color32::from_rgb(0x4a, 0x46, 0x56),
                text_faint: Color32::from_rgb(0x76, 0x71, 0x84),
                icon: Color32::from_rgb(0x3b, 0x37, 0x46),
                icon_active: Color32::from_rgb(0x10, 0x0e, 0x16),
                hover: Color32::from_rgb(0xea, 0xe5, 0xdb),
                pressed: Color32::from_rgb(0xdf, 0xd9, 0xcd),
                field_bg: Color32::WHITE,
                field_border: Color32::from_rgb(0xcf, 0xca, 0xc0),
                separator: Color32::from_rgb(0xdd, 0xd8, 0xce),
                row_alt: Color32::from_rgb(0xf2, 0xef, 0xe8),
                row_selected: Color32::from_rgb(0xf9, 0xd7, 0xda),
                hot_text: Color32::from_rgb(0xb5, 0x19, 0x2b),
                pill_active_bg: Color32::from_rgb(0xf9, 0xd7, 0xda),
                clip_bar_bg: Color32::from_rgb(0xf2, 0xdc, 0xde),
                control_surface: Color32::from_rgb(0xef, 0xec, 0xe4),
                control_well: Color32::from_rgb(0xe3, 0xdf, 0xd5),
                control_border: Color32::from_rgb(0xc5, 0xc0, 0xb5),
                control_handle_bg: Color32::from_rgb(0xe6, 0xe2, 0xd8),
                control_handle: Color32::from_rgb(0x28, 0x24, 0x32),
                control_handle_dim: Color32::from_rgb(0x54, 0x4f, 0x60),
                // Strictly color-neutral (R == G == B) for video frames & scopes
                plot_bg: Color32::from_rgb(0x14, 0x14, 0x14),
                plot_grid: Color32::from_rgb(0x2c, 0x2c, 0x2c),
                plot_axis: Color32::from_rgb(0x44, 0x44, 0x44),
                plot_handle: Color32::from_rgb(0xe0, 0xe0, 0xe0),
                plot_handle_dim: Color32::from_rgb(0x90, 0x90, 0x90),
                meter_bg: Color32::from_rgb(0x14, 0x14, 0x14),
                meter_readout_bg: Color32::from_rgb(0xef, 0xec, 0xe4),
                meter_clip_off: Color32::from_rgb(0x98, 0x94, 0x8c),
                panner_bg: Color32::from_gray(225),
                panner_border: Color32::from_gray(165),
                panner_speaker: Color32::from_gray(105),
                fader_knob: Color32::from_gray(135),
                eq_track: Color32::from_gray(200),
                eq_tick: Color32::from_gray(150),
                eq_knob: Color32::from_gray(85),
                eq_knob_active: Color32::from_gray(34),
                eq_node: Color32::from_gray(50),
                eq_node_off: Color32::from_gray(140),
                crossover_handle: Color32::from_gray(50),
                keyframe_plot_bg: Color32::from_gray(245),
                keyframe_plot_grid: Color32::from_gray(210),
                keyframe_plot_axis: Color32::from_gray(170),
                keyframe_handle: Color32::from_gray(50),
                switch_fill: Color32::from_gray(60),
                switch_knob: Color32::from_gray(232),
                switch_border: Color32::from_gray(110),
                slider_disabled_track: Color32::from_gray(190),
                slider_disabled_knob: Color32::from_gray(140),
                button_hover: Color32::from_gray(220),
                button_border: Color32::from_gray(180),
                caption_track_bg: Color32::from_rgb(0xeb, 0xe3, 0xd8),
                scroll_thumb: Color32::from_gray(160),
                scroll_thumb_hover: Color32::from_gray(135),
                vertical_scroll_thumb: Color32::from_gray(160),
                disabled_clip_bg: Color32::from_gray(195),
                in_out_shade: Color32::from_gray(195),
                tl_bg: Color32::from_rgb(0xef, 0xec, 0xe4),
                tl_track_bg: Color32::from_rgb(0xf7, 0xf4, 0xee),
                tl_track_bg_alt: Color32::from_rgb(0xf1, 0xed, 0xe5),
                tl_header_bg: Color32::from_rgb(0xea, 0xe6, 0xdc),
                tl_ruler_bg: Color32::from_rgb(0xe5, 0xe0, 0xd5),
                tl_ruler_tick: Color32::from_rgb(0x7c, 0x77, 0x88),
                tl_ruler_text: Color32::from_rgb(0x4a, 0x46, 0x56),
                playhead: Color32::from_rgb(0xc8, 0x1e, 0x32),
                track_target: Color32::from_rgb(0xa8, 0x15, 0x26),
                monitor_bg: Color32::from_rgb(0x14, 0x14, 0x14),
                timecode: Color32::from_rgb(0xb5, 0x19, 0x2b),
                ..dark
            },
        }
    }

    /// Settings ▸ Appearance on top of a theme: the highlight colour (selections, primary buttons,
    /// the focus outline, the playhead, timecodes, drag values and track targeting) and accessible
    /// colour contrast (brighter secondary text and borders).
    pub fn with_appearance(mut self, highlight: Option<[u8; 3]>, accessible_contrast: bool) -> Self {
        let is_inferno = matches!(self.kind, ThemeKind::InfernoDark | ThemeKind::InfernoLight);
        let is_legacy_default_highlight = highlight == Some([0x2f, 0x6b, 0xdf]);
        if let Some([r, g, b]) = highlight
            && !(is_inferno && is_legacy_default_highlight)
        {
            let c = Color32::from_rgb(r, g, b);
            // A highlight of the user's own also colours the theme's other blues: the same hue, as
            // much more or less saturated and bright as each of them is than the theme's highlight.
            // The theme's own highlight leaves them exactly as designed.
            if c != self.accent {
                let tint = |s: f32, v: f32| {
                    let mut h = egui::ecolor::HsvaGamma::from(c);
                    h.s = (h.s * s).clamp(0.0, 1.0);
                    h.v = (h.v * v).clamp(0.0, 1.0);
                    Color32::from(h)
                };
                self.focus = tint(0.92, 1.06);
                self.playhead = self.focus;
                self.timecode = tint(0.98, 1.09);
                self.hot_text = self.timecode;
                self.track_target = tint(1.0, 0.865);
            }
            let lift = |v: u8| v.saturating_add(16);
            self.accent = c;
            self.accent_hover = Color32::from_rgb(lift(r), lift(g), lift(b));
        }
        if accessible_contrast {
            if !self.kind.is_dark() {
                self.text_dim = Color32::from_rgb(40, 40, 40);
                self.text_faint = Color32::from_rgb(80, 80, 80);
                self.field_border = Color32::from_rgb(110, 110, 110);
            } else {
                self.text_dim = self.text;
                self.text_faint = Color32::from_rgb(0xa8, 0xa8, 0xa8);
                self.tab_text = self.tab_text_active;
                self.field_border = Color32::from_rgb(0x6a, 0x6a, 0x6a);
                self.separator = Color32::from_rgb(0x50, 0x50, 0x50);
            }
        }
        self
    }

    /// Canonical primary sans-serif family for this token palette (`GemiFontSans` for Inferno Pixel
    /// Glass / GemiTokens modes, `Inter` for legacy palettes).
    pub fn font_sans(&self) -> &'static str {
        match self.kind {
            ThemeKind::InfernoDark | ThemeKind::InfernoLight => GEMI_FONT_SANS,
            _ => filmcraft_text::fonts::DEFAULT_FAMILY,
        }
    }

    /// Primary UI body font (`GemiFontSans` on GemiTokens palettes, proportional default elsewhere).
    pub fn font_id(&self, size: f32) -> FontId {
        match self.kind {
            ThemeKind::InfernoDark | ThemeKind::InfernoLight => gemi_sans(size),
            _ => FontId::proportional(size),
        }
    }

    /// Medium-weight UI font (`GemiFontSans-Medium` on GemiTokens palettes, `Inter-Medium` elsewhere).
    pub fn medium_font_id(&self, size: f32) -> FontId {
        match self.kind {
            ThemeKind::InfernoDark | ThemeKind::InfernoLight => gemi_medium(size),
            _ => FontId::new(size, FontFamily::Name("medium".into())),
        }
    }

    /// Bold/heading UI font (`GemiFontSans-Bold` on GemiTokens palettes, `Inter-SemiBold` elsewhere).
    pub fn bold_font_id(&self, size: f32) -> FontId {
        match self.kind {
            ThemeKind::InfernoDark | ThemeKind::InfernoLight => gemi_bold(size),
            _ => Self::semibold(size),
        }
    }

    /// Mono font for timecode.
    pub fn mono(size: f32) -> FontId {
        FontId::new(size, FontFamily::Monospace)
    }
    pub fn ui(size: f32) -> FontId {
        FontId::new(size, FontFamily::Proportional)
    }
    pub fn semibold(size: f32) -> FontId {
        FontId::new(size, FontFamily::Name("semibold".into()))
    }
    /// The big blue timecode over the timeline and under the monitors: regular weight, as in
    /// Premiere (Premiere's reads 10 px tall and 76 px wide for 00:00:16:05 at 1x).
    pub fn timecode() -> FontId {
        FontId::new(16.0, FontFamily::Proportional)
    }
}

/// Canonical GemiTokens / GemiFonts Arabic+Latin UI family name.
pub const GEMI_FONT_SANS: &str = filmcraft_text::fonts::GEMI_FONT_SANS_FAMILY;

pub fn gemi_sans(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(GEMI_FONT_SANS.into()))
}
pub fn gemi_light(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("GemiFontSans-Light".into()))
}
pub fn gemi_medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("GemiFontSans-Medium".into()))
}
pub fn gemi_bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("GemiFontSans-Bold".into()))
}

/// Menu text: Premiere's menus are the system's, larger and brighter than panel text, one item
/// every 24 px.
pub const MENU_TEXT_SIZE: f32 = 13.0;
pub const MENU_TEXT: Color32 = Color32::from_rgb(0xde, 0xde, 0xde);

/// Style of the menu bar's menus and their submenus (on top of egui's own menu style).
pub fn menu_style(s: &mut egui::Style) {
    egui::containers::menu::menu_style(s);
    s.text_styles.insert(TextStyle::Button, FontId::new(MENU_TEXT_SIZE, FontFamily::Proportional));
    s.text_styles.insert(TextStyle::Body, FontId::new(MENU_TEXT_SIZE, FontFamily::Proportional));
    s.spacing.item_spacing.y = 0.0;
    if s.visuals.dark_mode {
        s.visuals.override_text_color = Some(MENU_TEXT);
    }
}

/// Every font family the theme defines (fallback fonts such as the Japanese fonts are added to each
/// of them).
pub fn font_families() -> Vec<FontFamily> {
    vec![FontFamily::Proportional, FontFamily::Monospace, FontFamily::Name("semibold".into()), FontFamily::Name("medium".into())]
}

/// Install the Latin UI fonts, GemiFontSans (GemiTokens Arabic+Latin UI), Chinese fallback
/// (craft-fonts or an installed face), Japanese craft-fonts, Arabic and Hebrew fallbacks
/// (craft-fonts or installed faces, [`crate::rtl`]) and egui visuals. Document names keep their
/// glyphs in every interface language.
pub fn install(ctx: &egui::Context, t: &Tokens) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert("inter".into(), Arc::new(FontData::from_static(filmcraft_text::fonts::INTER_REGULAR)));
    fonts.font_data.insert("inter-medium".into(), Arc::new(FontData::from_static(filmcraft_text::fonts::INTER_MEDIUM)));
    fonts.font_data.insert("inter-semibold".into(), Arc::new(FontData::from_static(filmcraft_text::fonts::INTER_SEMIBOLD)));
    fonts.font_data.insert("jbmono".into(), Arc::new(FontData::from_static(filmcraft_text::fonts::JETBRAINS_MONO_REGULAR)));
    fonts.font_data.insert("GemiFontSans-Light".into(), Arc::new(FontData::from_static(filmcraft_text::fonts::GEMI_FONT_SANS_LIGHT)));
    fonts.font_data.insert("GemiFontSans-Medium".into(), Arc::new(FontData::from_static(filmcraft_text::fonts::GEMI_FONT_SANS_MEDIUM)));
    fonts.font_data.insert("GemiFontSans-Bold".into(), Arc::new(FontData::from_static(filmcraft_text::fonts::GEMI_FONT_SANS_BOLD)));
    {
        let prop = fonts.families.entry(FontFamily::Proportional).or_default();
        prop.insert(0, "GemiFontSans-Medium".into());
        prop.insert(0, "inter".into());
    }
    fonts.families.entry(FontFamily::Monospace).or_default().insert(0, "jbmono".into());
    // Named weights fall back to the default stack for missing glyphs.
    let fallback: Vec<String> = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    for (fam, primary) in [
        ("semibold", "inter-semibold"),
        ("medium", "inter-medium"),
        (GEMI_FONT_SANS, "GemiFontSans-Medium"),
        ("GemiFontSans-Light", "GemiFontSans-Light"),
        ("GemiFontSans-Medium", "GemiFontSans-Medium"),
        ("GemiFontSans-Bold", "GemiFontSans-Bold"),
    ] {
        let mut stack: Vec<String> = vec![primary.into()];
        for f in &fallback {
            if f != primary {
                stack.push(f.clone());
            }
        }
        fonts.families.insert(FontFamily::Name(fam.into()), stack);
    }
    // Japanese before Chinese: the two share code points, and Japanese text must keep Japanese
    // glyph forms; Chinese faces still cover the hanzi Japanese fonts lack
    add_craft_fonts(&mut fonts);
    crate::cjk::install(&mut fonts);
    crate::rtl::install(&mut fonts);
    ctx.set_fonts(fonts);
    apply_visuals(ctx, t);
}

/// Name of the egui font for a craft-fonts entry.
fn craft_font_name(f: &filmcraft_text::fonts::CraftFont) -> String {
    format!("craft:{} {}", f.family, f.style)
}

/// Append the Japanese craft-fonts (empty unless built with `CRAFT_FONTS_DIR`) as the last fallbacks
/// of every font family, after the app's own fonts: BIZ UDPGothic first (bold before regular in
/// the "semibold" and "medium" families), then the other Japanese faces.
fn add_craft_fonts(fonts: &mut FontDefinitions) {
    let jpan: Vec<_> = filmcraft_text::fonts::craft_japanese().collect();
    for f in &jpan {
        fonts.font_data.insert(craft_font_name(f), Arc::new(FontData::from_static(f.bytes)));
    }
    for (family, stack) in fonts.families.iter_mut() {
        let heavy = matches!(family, FontFamily::Name(n) if matches!(n.as_ref(), "semibold" | "medium"));
        let mut order = jpan.clone();
        order.sort_by_key(|f| (!f.family.contains("Gothic"), (f.style == "Bold") != heavy));
        stack.extend(order.iter().map(|f| craft_font_name(f)));
    }
}

pub fn apply_visuals(ctx: &egui::Context, t: &Tokens) {
    let light = !t.kind.is_dark();
    // FilmCraft decides between light and dark itself (Settings ▸ Appearance ▸ Appearance Mode), so
    // egui must not switch to its other built-in style when the system appearance changes.
    ctx.set_theme(if light { egui::Theme::Light } else { egui::Theme::Dark });
    let mut v = if light { Visuals::light() } else { Visuals::dark() };
    v.panel_fill = t.panel_bg;
    v.window_fill = t.panel_bg;
    v.extreme_bg_color = t.field_bg;
    v.faint_bg_color = t.row_alt;
    v.override_text_color = Some(t.text);
    v.selection.bg_fill = t.accent;
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    v.hyperlink_color = t.accent;
    v.window_stroke = Stroke::new(1.0, t.field_border);
    v.window_corner_radius = egui::CornerRadius::same(6);
    v.menu_corner_radius = egui::CornerRadius::same(6);
    v.popup_shadow = egui::epaint::Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(140) };
    v.window_shadow = v.popup_shadow;
    for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = egui::CornerRadius::same(t.radius_sm as u8);
    }
    v.widgets.noninteractive.bg_fill = t.panel_bg;
    v.widgets.noninteractive.weak_bg_fill = t.panel_bg;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, t.separator);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, t.text);
    v.widgets.inactive.bg_fill = t.field_bg;
    v.widgets.inactive.weak_bg_fill = t.field_bg;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, t.field_border);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, t.text);
    v.widgets.hovered.bg_fill = t.hover;
    v.widgets.hovered.weak_bg_fill = t.hover;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, t.field_border);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, t.tab_text_active);
    v.widgets.active.bg_fill = t.pressed;
    v.widgets.active.weak_bg_fill = t.pressed;
    v.widgets.active.fg_stroke = Stroke::new(1.0, t.tab_text_active);
    v.widgets.open.bg_fill = t.hover;
    v.widgets.open.weak_bg_fill = t.hover;
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(6.0, 4.0);
        s.spacing.button_padding = egui::vec2(8.0, 3.0);
        s.spacing.interact_size.y = 24.0;
        s.spacing.menu_margin = egui::Margin::same(4);
        s.text_styles.insert(TextStyle::Body, t.font_id(12.0));
        s.text_styles.insert(TextStyle::Button, t.font_id(12.0));
        s.text_styles.insert(TextStyle::Small, t.font_id(11.0));
        s.text_styles.insert(TextStyle::Heading, t.bold_font_id(15.0));
        s.text_styles.insert(TextStyle::Monospace, FontId::new(12.0, FontFamily::Monospace));
        s.animation_time = ANIMATION_TIME;
    });
}

/// How long egui's fades and slides take (menus, collapsing sections, tooltips); 0 when
/// Settings ▸ Appearance ▸ "Animate zoom, scrolling and menus" is off (#2325).
pub const ANIMATION_TIME: f32 = 0.12;

#[cfg(test)]
mod tests {
    use super::*;

    const JAPANESE: &str = "日本語の文字";

    fn installed() -> egui::Context {
        let ctx = egui::Context::default();
        install(&ctx, &Tokens::for_kind(ThemeKind::default()));
        let mut out = ctx.run_ui(egui::RawInput::default(), |_| {});
        out.textures_delta.clear();
        ctx
    }

    #[test]
    fn light_theme_has_light_chrome_and_plot_surfaces() {
        let dark = Tokens::for_kind(ThemeKind::Dark);
        let light = Tokens::for_kind(ThemeKind::Light);
        let luminance = |c: Color32| u16::from(c.r()) + u16::from(c.g()) + u16::from(c.b());

        for (name, dark_surface, light_surface) in [
            ("active pill", dark.pill_active_bg, light.pill_active_bg),
            ("clip bar", dark.clip_bar_bg, light.clip_bar_bg),
            ("control surface", dark.control_surface, light.control_surface),
            ("control well", dark.control_well, light.control_well),
            ("plot", dark.plot_bg, light.plot_bg),
            ("keyframe plot", dark.keyframe_plot_bg, light.keyframe_plot_bg),
            ("panner", dark.panner_bg, light.panner_bg),
            ("caption lane", dark.caption_track_bg, light.caption_track_bg),
            ("active toggle", dark.pressed, light.pressed),
            ("meter", dark.meter_bg, light.meter_bg),
            ("meter readout", dark.meter_readout_bg, light.meter_readout_bg),
        ] {
            assert!(luminance(light_surface) > luminance(dark_surface), "{name} did not adapt to Light");
        }
        assert!(luminance(light.control_handle) < luminance(light.control_handle_bg));
        assert!(luminance(light.plot_handle) < luminance(light.plot_bg));
        assert!(luminance(light.icon_active) < luminance(light.pressed));
        assert_eq!(Tokens::for_kind(ThemeKind::Medium).plot_bg, dark.plot_bg, "Medium keeps the established dark plot treatment");
    }

    #[test]
    fn dark_and_medium_keep_existing_audio_and_effect_palettes() {
        for kind in [ThemeKind::Dark, ThemeKind::Medium] {
            let t = Tokens::for_kind(kind);
            // Existing painted colors, recorded independently of the Light palette.
            assert_eq!(t.pill_active_bg, Color32::from_gray(0x3a));
            assert_eq!(t.clip_bar_bg, Color32::from_rgb(58, 58, 70));
            assert_eq!(t.control_surface, Color32::from_gray(0x24));
            assert_eq!(t.control_well, Color32::from_gray(0x10));
            assert_eq!(t.control_handle, Color32::from_gray(0xe0));
            assert_eq!(t.meter_bg, Color32::BLACK);
            assert_eq!(t.plot_bg, Color32::from_gray(0x18));
            assert_eq!(t.plot_grid, Color32::from_gray(0x2c));
            assert_eq!(t.keyframe_plot_bg, Color32::from_gray(0x19));
            assert_eq!(t.keyframe_plot_grid, Color32::from_gray(0x2a));
            assert_eq!(t.keyframe_plot_axis, Color32::from_gray(0x33));
            assert_eq!(t.keyframe_handle, Color32::from_gray(0xd0));
        }
    }

    /// The colours the highlight drives besides the accent (#707).
    fn highlight_blues(t: &Tokens) -> [Color32; 5] {
        [t.focus, t.playhead, t.timecode, t.hot_text, t.track_target]
    }

    #[test]
    fn the_default_highlight_leaves_the_theme_blues_as_designed() {
        let default = filmcraft_engine::settings::parse_hex(filmcraft_engine::settings::DEFAULT_HIGHLIGHT);
        for k in [ThemeKind::Dark, ThemeKind::Medium, ThemeKind::Light] {
            assert_eq!(highlight_blues(&Tokens::for_kind(k).with_appearance(default, false)), highlight_blues(&Tokens::for_kind(k)), "{k:?}");
        }
    }

    #[test]
    fn a_highlight_of_ones_own_colours_the_playhead_timecodes_focus_and_targeting() {
        let pink = Color32::from_rgb(0xe0, 0x45, 0x7b);
        let t = Tokens::for_kind(ThemeKind::Dark).with_appearance(Some([0xe0, 0x45, 0x7b]), false);
        let hue = |c: Color32| egui::ecolor::HsvaGamma::from(c).h;
        for (c, was) in highlight_blues(&t).into_iter().zip(highlight_blues(&Tokens::for_kind(ThemeKind::Dark))) {
            assert_ne!(c, was);
            assert!((hue(c) - hue(pink)).abs() < 0.01, "{c:?} has the highlight's hue");
        }
        let v = |c: Color32| egui::ecolor::HsvaGamma::from(c).v;
        assert!(v(t.playhead) > v(pink) && v(t.timecode) > v(pink) && v(t.track_target) < v(pink), "lighter and darker as the default blues are");
    }

    /// Chinese media and track names must render even when the interface stays in English.
    /// Compare the rasterized glyphs with the missing-glyph box, including custom weight families.
    #[test]
    fn chinese_names_render_in_every_theme_and_font_family() {
        const CHINESE: &str = "中文旁白音乐声轨测试简体汉语繁體漢語车";
        filmcraft_text::fonts::scan_system();
        let available = filmcraft_text::fonts::all_faces()
            .iter()
            .any(|f| matches!(f.info.origin, "system" | filmcraft_text::fonts::CRAFT_ORIGIN) && CHINESE.chars().all(|c| f.has_char(c)));
        if !available {
            eprintln!("SKIPPED: no craft-fonts or installed Chinese font");
            return;
        }
        let ctx = egui::Context::default();
        for kind in [ThemeKind::Dark, ThemeKind::Medium, ThemeKind::Light] {
            install(&ctx, &Tokens::for_kind(kind));
            ctx.run_ui(egui::RawInput::default(), |_| {}).textures_delta.clear();
            ctx.fonts_mut(|fonts| {
                for family in font_families() {
                    let font = FontId::new(13.0, family);
                    let missing = fonts.layout_no_wrap("\u{fffd}".into(), font.clone(), Color32::WHITE);
                    let missing_uv = missing.rows[0].glyphs[0].uv_rect;
                    let names = fonts.layout_no_wrap(CHINESE.into(), font.clone(), Color32::WHITE);
                    assert_eq!(names.rows.iter().map(|r| r.glyphs.len()).sum::<usize>(), CHINESE.chars().count());
                    for glyph in names.rows.iter().flat_map(|r| &r.glyphs) {
                        assert!(!glyph.uv_rect.is_nothing(), "empty {} in {font:?}", glyph.chr);
                        assert_ne!(glyph.uv_rect, missing_uv, "missing {} in {kind:?} / {font:?}", glyph.chr);
                    }
                }
            });
        }
        // the layouts above put glyphs in the atlas: take that update so it isn't dropped unapplied
        ctx.run_ui(egui::RawInput::default(), |_| {}).textures_delta.clear();
    }

    /// Typed, pasted or IME-composed Hangul draws real glyphs, not the missing-glyph box (artcraft#2093).
    #[test]
    fn korean_text_renders_with_an_installed_korean_font() {
        const KOREAN: &str = "한국어 입력 테스트";
        filmcraft_text::fonts::scan_system();
        let available = filmcraft_text::fonts::all_faces().iter().any(|f| f.info.origin == "system" && KOREAN.chars().all(|c| f.has_char(c)));
        if !available {
            eprintln!("SKIPPED: no installed Korean font");
            return;
        }
        let ctx = egui::Context::default();
        install(&ctx, &Tokens::for_kind(ThemeKind::Dark));
        ctx.run_ui(egui::RawInput::default(), |_| {}).textures_delta.clear();
        ctx.fonts_mut(|fonts| {
            for family in font_families() {
                let font = FontId::new(13.0, family);
                let missing = fonts.layout_no_wrap("\u{fffd}".into(), font.clone(), Color32::WHITE);
                let missing_uv = missing.rows[0].glyphs[0].uv_rect;
                let text = fonts.layout_no_wrap(KOREAN.into(), font.clone(), Color32::WHITE);
                for glyph in text.rows.iter().flat_map(|r| &r.glyphs).filter(|g| g.chr != ' ') {
                    assert_ne!(glyph.uv_rect, missing_uv, "missing {} in {font:?}", glyph.chr);
                }
            }
        });
        ctx.run_ui(egui::RawInput::default(), |_| {}).textures_delta.clear();
    }

    /// Built with craft-fonts: every font family ends with the Japanese faces (BIZ UDPGothic
    /// first), and Japanese text gets real glyphs, not the replacement box.
    #[test]
    fn japanese_renders_with_craft_fonts() {
        if filmcraft_text::fonts::craft_japanese().next().is_none() {
            eprintln!("SKIPPED: built without craft-fonts (set CRAFT_FONTS_DIR to run)");
            return;
        }
        let ctx = installed();
        ctx.fonts_mut(|fonts| {
            let defs = fonts.definitions().clone();
            for (family, stack) in &defs.families {
                // installed right-to-left faces (Hebrew is not in craft-fonts) come after the craft run
                let stack: Vec<&String> = stack.iter().filter(|n| !crate::rtl::is_system_face(n)).collect();
                let first = stack.iter().position(|n| n.starts_with("craft:")).unwrap_or(stack.len());
                assert!(first > 0 && stack[first..].iter().all(|n| n.starts_with("craft:")), "{family:?}: {stack:?}");
                let first_japanese = stack.iter().find(|n| n.starts_with("craft:BIZ") || n.starts_with("craft:Shippori"));
                assert!(first_japanese.is_some_and(|n| n.starts_with("craft:BIZ UDPGothic")), "{family:?}: {stack:?}");
            }
            for family in [FontFamily::Proportional, FontFamily::Monospace] {
                let font = FontId::new(13.0, family);
                for ch in JAPANESE.chars() {
                    assert!(fonts.has_glyph(&font, ch), "missing {ch} in {font:?}");
                }
            }
        });
        // laid out and drawn: the replacement box (U+FFFD) would show as one glyph per character
        // from the fallback face; a real face gives each character its own width
        let galley = ctx.fonts_mut(|f| f.layout_no_wrap(JAPANESE.into(), FontId::new(13.0, FontFamily::Proportional), Color32::WHITE));
        assert_eq!(galley.rows.iter().map(|r| r.glyphs.len()).sum::<usize>(), JAPANESE.chars().count());
        assert!(galley.size().x > 13.0 * 4.0, "{:?}", galley.size());
    }

    /// Latin text keeps its UI faces, with or without craft-fonts and system Chinese fonts.
    #[test]
    fn works_without_craft_fonts() {
        let ctx = installed();
        let n = filmcraft_text::fonts::craft_japanese().count() + crate::cjk::craft_fonts().count() + crate::rtl::craft_fonts().count();
        ctx.fonts_mut(|fonts| {
            let defs = fonts.definitions().clone();
            assert_eq!(defs.font_data.keys().filter(|k| k.starts_with("craft:")).count(), n);
            assert_eq!(defs.families[&FontFamily::Proportional].first().map(String::as_str), Some("inter"));
            assert_eq!(defs.families[&FontFamily::Monospace].first().map(String::as_str), Some("jbmono"));
            assert!(fonts.has_glyph(&FontId::new(13.0, FontFamily::Proportional), 'A'));
            if n == 0 {
                assert!(defs.families.values().flatten().all(|name| !name.starts_with("craft:")));
            }
        });
    }

    #[test]
    fn inferno_pixel_glass_themes_preserve_color_neutral_monitors_and_scopes() {
        let dark = Tokens::for_kind(ThemeKind::InfernoDark);
        let light = Tokens::for_kind(ThemeKind::InfernoLight);
        assert!(ThemeKind::InfernoDark.is_dark());
        assert!(!ThemeKind::InfernoLight.is_dark());
        assert_eq!(ThemeKind::from_pref("infernoDark"), ThemeKind::InfernoDark);
        assert_eq!(ThemeKind::from_pref("infernoLight"), ThemeKind::InfernoLight);
        assert_eq!(ThemeKind::from_name("infernoDark"), Some(ThemeKind::InfernoDark));
        assert_eq!(ThemeKind::from_name("infernoLight"), Some(ThemeKind::InfernoLight));
        assert_eq!(dark.accent, Color32::from_rgb(0xf8, 0x39, 0x42));
        assert_eq!(dark.focus, Color32::from_rgb(0xff, 0xd4, 0x37));
        assert_eq!(light.accent, Color32::from_rgb(0xc8, 0x1e, 0x32));
        assert_eq!(dark.font_sans(), GEMI_FONT_SANS);
        assert_eq!(light.font_sans(), GEMI_FONT_SANS);
        // Monitor and scope plot surfaces must be strictly color-neutral (R == G == B) across all themes
        for k in ThemeKind::ALL {
            let t = Tokens::for_kind(k);
            assert_eq!(t.monitor_bg.r(), t.monitor_bg.g(), "{k:?} monitor_bg R != G");
            assert_eq!(t.monitor_bg.g(), t.monitor_bg.b(), "{k:?} monitor_bg G != B");
            assert_eq!(t.plot_bg.r(), t.plot_bg.g(), "{k:?} plot_bg R != G");
            assert_eq!(t.plot_bg.g(), t.plot_bg.b(), "{k:?} plot_bg G != B");
        }
    }

    #[test]
    fn gemi_font_sans_via_gemi_tokens_is_registered_and_shapes_arabic_and_latin() {
        let ctx = egui::Context::default();
        let t = Tokens::for_kind(ThemeKind::InfernoDark);
        install(&ctx, &t);
        ctx.run_ui(egui::RawInput::default(), |_| {}).textures_delta.clear();
        assert_eq!(t.font_sans(), "GemiFontSans");
        ctx.run_ui(egui::RawInput::default(), |ui| {
            for fam in ["GemiFontSans", "GemiFontSans-Light", "GemiFontSans-Medium", "GemiFontSans-Bold"] {
                assert!(ui.ctx().fonts_mut(|f| f.families().contains(&FontFamily::Name(fam.into()))), "missing family {fam}");
            }
            for font in [t.font_id(13.0), t.medium_font_id(13.0), t.bold_font_id(15.0), gemi_light(12.0)] {
                let sz = ui.painter().layout_no_wrap("FilmTron — محرر الفيديو الاحترافي".into(), font, t.text).size();
                assert!(sz.x > 40.0 && sz.y > 8.0, "GemiFontSans shaped bilingual text: {sz:?}");
            }
        })
        .textures_delta
        .clear();
    }

    #[test]
    fn all_20_gemi_tokens_presets_resolve_with_neutral_monitor_and_scope_plot() {
        assert_eq!(gemi_tokens_v1::PRESET_IDS.len(), 20);
        for id in gemi_tokens_v1::PRESET_IDS {
            for dark in [true, false] {
                let t = Tokens::for_gemi_preset(id, dark).expect("canonical GemiTokens preset");
                assert_eq!(t.monitor_bg, Color32::from_rgb(24, 24, 24), "{id} monitor_bg");
                assert_eq!(t.plot_bg, Color32::from_rgb(18, 18, 18), "{id} plot_bg");
            }
        }
    }
}

/// Canonical 20-preset `GemiTokens` v1 surface constants & runtime preset resolver
/// (`GemiTron88/GemiTokens` `dist/gemi-tokens.rs`).
pub mod gemi_tokens_v1 {
    use egui::Color32;

    /// Color-neutral Program/Source video monitor viewport (`RGB(24, 24, 24)` / `#181818`).
    pub const NEUTRAL_MEDIA_VIEWPORT: Color32 = Color32::from_rgb(24, 24, 24);
    /// Color-neutral Lumetri waveform/vectorscope/histogram plot (`RGB(18, 18, 18)` / `#121212`).
    pub const NEUTRAL_SCOPE_PLOT: Color32 = Color32::from_rgb(18, 18, 18);

    pub const PRESET_IDS: [&str; 20] = [
        "inferno",
        "royalGold",
        "cyberCyan",
        "liquidGlass",
        "modernMaterial",
        "cleanCommerce",
        "terracotta",
        "amethyst",
        "pixelArcade",
        "nordicFrost",
        "frostedWin",
        "macLiquid",
        "sakuraNody",
        "bakkaGold",
        "catVelvet",
        "retroMatrix",
        "oceanAbyss",
        "oledPitch",
        "sunsetDune",
        "emeraldMushaf",
    ];

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct PresetColors {
        pub bg: Color32,
        pub surface: Color32,
        pub raised: Color32,
        pub text: Color32,
        pub muted: Color32,
        pub primary: Color32,
        pub border: Color32,
        pub focus: Color32,
        pub danger: Color32,
        pub radii_sm: u8,
        pub radii_md: u8,
    }

    const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
        Color32::from_rgb(r, g, b)
    }

    pub fn lookup_preset(id: &str, dark: bool) -> Option<PresetColors> {
        match (id, dark) {
            ("inferno", true) => Some(PresetColors { bg: rgb(0x08, 0x08, 0x0A), surface: rgb(0x10, 0x10, 0x13), raised: rgb(0x24, 0x24, 0x2A), text: rgb(0xF5, 0xF1, 0xED), muted: rgb(0xAC, 0xA8, 0xAE), primary: rgb(0xF8, 0x39, 0x42), border: rgb(0x37, 0x32, 0x39), focus: rgb(0xFF, 0xD4, 0x37), danger: rgb(0xFF, 0x6B, 0x81), radii_sm: 6, radii_md: 8 }),
            ("inferno", false) => Some(PresetColors { bg: rgb(0xFB, 0xF9, 0xF7), surface: rgb(0xFF, 0xFF, 0xFF), raised: rgb(0xF2, 0xEE, 0xEB), text: rgb(0x14, 0x12, 0x16), muted: rgb(0x57, 0x50, 0x5A), primary: rgb(0xC8, 0x1E, 0x32), border: rgb(0xD8, 0xD1, 0xCB), focus: rgb(0xC8, 0x1E, 0x32), danger: rgb(0xB4, 0x23, 0x42), radii_sm: 6, radii_md: 8 }),
            ("royalGold", true) => Some(PresetColors { bg: rgb(0x09, 0x09, 0x0C), surface: rgb(0x12, 0x11, 0x16), raised: rgb(0x25, 0x22, 0x2B), text: rgb(0xF7, 0xF4, 0xEC), muted: rgb(0xB5, 0xAE, 0xA2), primary: rgb(0xE5, 0xB8, 0x42), border: rgb(0x3B, 0x35, 0x2C), focus: rgb(0xF7, 0xD3, 0x6A), danger: rgb(0xFF, 0x75, 0x88), radii_sm: 4, radii_md: 6 }),
            ("royalGold", false) => Some(PresetColors { bg: rgb(0xFA, 0xF7, 0xF2), surface: rgb(0xFF, 0xFF, 0xFF), raised: rgb(0xF2, 0xEC, 0xE1), text: rgb(0x17, 0x14, 0x10), muted: rgb(0x59, 0x51, 0x45), primary: rgb(0x7C, 0x58, 0x0B), border: rgb(0xD9, 0xCF, 0xC0), focus: rgb(0x7C, 0x58, 0x0B), danger: rgb(0xB4, 0x23, 0x42), radii_sm: 4, radii_md: 6 }),
            ("cyberCyan", true) => Some(PresetColors { bg: rgb(0x07, 0x0B, 0x10), surface: rgb(0x0E, 0x14, 0x1D), raised: rgb(0x1D, 0x27, 0x36), text: rgb(0xEC, 0xF4, 0xFA), muted: rgb(0x9F, 0xB0, 0xC2), primary: rgb(0x22, 0xD3, 0xEE), border: rgb(0x2B, 0x3A, 0x4E), focus: rgb(0x67, 0xE8, 0xF9), danger: rgb(0xFF, 0x75, 0x8F), radii_sm: 3, radii_md: 5 }),
            ("cyberCyan", false) => Some(PresetColors { bg: rgb(0xF3, 0xF8, 0xFA), surface: rgb(0xFF, 0xFF, 0xFF), raised: rgb(0xE6, 0xF0, 0xF5), text: rgb(0x0D, 0x18, 0x21), muted: rgb(0x43, 0x56, 0x66), primary: rgb(0x08, 0x68, 0x80), border: rgb(0xC5, 0xD7, 0xE2), focus: rgb(0x08, 0x68, 0x80), danger: rgb(0xB4, 0x23, 0x42), radii_sm: 3, radii_md: 5 }),
            ("liquidGlass", true) => Some(PresetColors { bg: rgb(0x09, 0x0C, 0x15), surface: rgb(0x11, 0x16, 0x25), raised: rgb(0x22, 0x2A, 0x42), text: rgb(0xEF, 0xF2, 0xFC), muted: rgb(0xA8, 0xB1, 0xCA), primary: rgb(0x81, 0x8C, 0xF8), border: rgb(0x32, 0x3C, 0x59), focus: rgb(0xA5, 0xB4, 0xFC), danger: rgb(0xFF, 0x7A, 0x90), radii_sm: 8, radii_md: 12 }),
            ("liquidGlass", false) => Some(PresetColors { bg: rgb(0xF5, 0xF6, 0xFC), surface: rgb(0xFF, 0xFF, 0xFF), raised: rgb(0xEA, 0xEC, 0xF7), text: rgb(0x12, 0x15, 0x26), muted: rgb(0x4B, 0x52, 0x6B), primary: rgb(0x37, 0x30, 0xA3), border: rgb(0xCE, 0xD3, 0xE8), focus: rgb(0x37, 0x30, 0xA3), danger: rgb(0xB4, 0x23, 0x42), radii_sm: 8, radii_md: 12 }),
            ("modernMaterial", true) => Some(PresetColors { bg: rgb(0x0A, 0x0E, 0x14), surface: rgb(0x12, 0x18, 0x22), raised: rgb(0x22, 0x2C, 0x3C), text: rgb(0xEE, 0xF3, 0xFA), muted: rgb(0xA3, 0xB1, 0xC4), primary: rgb(0x60, 0xA5, 0xFA), border: rgb(0x2F, 0x3D, 0x52), focus: rgb(0x93, 0xC5, 0xFD), danger: rgb(0xFF, 0x78, 0x8E), radii_sm: 6, radii_md: 10 }),
            ("modernMaterial", false) => Some(PresetColors { bg: rgb(0xF6, 0xF8, 0xFC), surface: rgb(0xFF, 0xFF, 0xFF), raised: rgb(0xEB, 0xF0, 0xF8), text: rgb(0x11, 0x18, 0x27), muted: rgb(0x47, 0x55, 0x69), primary: rgb(0x1E, 0x40, 0xAF), border: rgb(0xCB, 0xD5, 0xE1), focus: rgb(0x1E, 0x40, 0xAF), danger: rgb(0xB4, 0x23, 0x42), radii_sm: 6, radii_md: 10 }),
            ("cleanCommerce", true) => Some(PresetColors { bg: rgb(0x07, 0x10, 0x0D), surface: rgb(0x0E, 0x1B, 0x16), raised: rgb(0x1D, 0x32, 0x2A), text: rgb(0xEC, 0xF7, 0xF2), muted: rgb(0x9E, 0xB8, 0xAD), primary: rgb(0x34, 0xD3, 0x99), border: rgb(0x2B, 0x47, 0x3C), focus: rgb(0x6E, 0xE7, 0xB7), danger: rgb(0xFF, 0x78, 0x8E), radii_sm: 5, radii_md: 8 }),
            ("cleanCommerce", false) => Some(PresetColors { bg: rgb(0xF5, 0xFA, 0xF7), surface: rgb(0xFF, 0xFF, 0xFF), raised: rgb(0xE6, 0xF2, 0xEC), text: rgb(0x0D, 0x1C, 0x16), muted: rgb(0x42, 0x59, 0x4F), primary: rgb(0x06, 0x5F, 0x46), border: rgb(0xC4, 0xDC, 0xD0), focus: rgb(0x06, 0x5F, 0x46), danger: rgb(0xB4, 0x23, 0x42), radii_sm: 5, radii_md: 8 }),
            ("terracotta", true) => Some(PresetColors { bg: rgb(0x10, 0x0B, 0x09), surface: rgb(0x19, 0x12, 0x10), raised: rgb(0x2D, 0x22, 0x1E), text: rgb(0xF8, 0xF1, 0xEC), muted: rgb(0xB8, 0xA9, 0xA1), primary: rgb(0xFB, 0x92, 0x3C), border: rgb(0x42, 0x32, 0x2C), focus: rgb(0xFD, 0xBA, 0x74), danger: rgb(0xFF, 0x7A, 0x8E), radii_sm: 7, radii_md: 10 }),
            ("terracotta", false) => Some(PresetColors { bg: rgb(0xFB, 0xF7, 0xF3), surface: rgb(0xFF, 0xFF, 0xFF), raised: rgb(0xF3, 0xEA, 0xE2), text: rgb(0x1C, 0x13, 0x10), muted: rgb(0x5E, 0x4B, 0x44), primary: rgb(0x8A, 0x2E, 0x0D), border: rgb(0xDE, 0xCB, 0xC0), focus: rgb(0x8A, 0x2E, 0x0D), danger: rgb(0xB4, 0x23, 0x42), radii_sm: 7, radii_md: 10 }),
            ("amethyst", true) => Some(PresetColors { bg: rgb(0x0C, 0x08, 0x12), surface: rgb(0x15, 0x10, 0x1F), raised: rgb(0x28, 0x1F, 0x38), text: rgb(0xF5, 0xEF, 0xFB), muted: rgb(0xB2, 0xA5, 0xC2), primary: rgb(0xC0, 0x84, 0xFC), border: rgb(0x3B, 0x2E, 0x52), focus: rgb(0xD8, 0xB4, 0xFE), danger: rgb(0xFF, 0x78, 0x90), radii_sm: 7, radii_md: 11 }),
            ("amethyst", false) => Some(PresetColors { bg: rgb(0xFA, 0xF7, 0xFC), surface: rgb(0xFF, 0xFF, 0xFF), raised: rgb(0xF0, 0xE9, 0xF7), text: rgb(0x1A, 0x11, 0x24), muted: rgb(0x58, 0x48, 0x69), primary: rgb(0x6B, 0x21, 0xA8), border: rgb(0xD8, 0xCB, 0xE8), focus: rgb(0x6B, 0x21, 0xA8), danger: rgb(0xB4, 0x23, 0x42), radii_sm: 7, radii_md: 11 }),
            ("pixelArcade", true) => Some(PresetColors { bg: rgb(0x08, 0x08, 0x11), surface: rgb(0x11, 0x11, 0x22), raised: rgb(0x23, 0x23, 0x3E), text: rgb(0xF6, 0xF0, 0xFA), muted: rgb(0xB0, 0xA7, 0xC4), primary: rgb(0xFF, 0x5C, 0xA8), border: rgb(0x3A, 0x36, 0x5E), focus: rgb(0x00, 0xF5, 0xD4), danger: rgb(0xFF, 0x78, 0x90), radii_sm: 2, radii_md: 3 }),
            ("pixelArcade", false) => Some(PresetColors { bg: rgb(0xF4, 0xF1, 0xE8), surface: rgb(0xFF, 0xFF, 0xFF), raised: rgb(0xE8, 0xE3, 0xD5), text: rgb(0x16, 0x13, 0x20), muted: rgb(0x55, 0x4D, 0x63), primary: rgb(0x9E, 0x0B, 0x52), border: rgb(0xCE, 0xC7, 0xB6), focus: rgb(0x9E, 0x0B, 0x52), danger: rgb(0xB4, 0x23, 0x42), radii_sm: 2, radii_md: 3 }),
            ("nordicFrost", true) => Some(PresetColors { bg: rgb(0x09, 0x0E, 0x14), surface: rgb(0x11, 0x19, 0x22), raised: rgb(0x20, 0x2E, 0x3D), text: rgb(0xED, 0xF4, 0xF9), muted: rgb(0xA1, 0xB3, 0xC4), primary: rgb(0x38, 0xBD, 0xF8), border: rgb(0x2D, 0x40, 0x54), focus: rgb(0x7D, 0xD3, 0xFC), danger: rgb(0xFF, 0x78, 0x8E), radii_sm: 4, radii_md: 6 }),
            ("nordicFrost", false) => Some(PresetColors { bg: rgb(0xF4, 0xF8, 0xFB), surface: rgb(0xFF, 0xFF, 0xFF), raised: rgb(0xE7, 0xEF, 0xF6), text: rgb(0x0F, 0x19, 0x22), muted: rgb(0x46, 0x58, 0x69), primary: rgb(0x07, 0x59, 0x85), border: rgb(0xC6, 0xD6, 0xE4), focus: rgb(0x07, 0x59, 0x85), danger: rgb(0xB4, 0x23, 0x42), radii_sm: 4, radii_md: 6 }),
            ("frostedWin", true) => Some(PresetColors { bg: rgb(0x0B, 0x0E, 0x14), surface: rgb(0x13, 0x18, 0x22), raised: rgb(0x21, 0x29, 0x3A), text: rgb(0xF0, 0xF4, 0xFA), muted: rgb(0xA5, 0xB2, 0xC6), primary: rgb(0x4C, 0xC2, 0xFF), border: rgb(0x32, 0x3E, 0x54), focus: rgb(0x7A, 0xD4, 0xFF), danger: rgb(0xFF, 0x7A, 0x90), radii_sm: 6, radii_md: 10 }),
            ("frostedWin", false) => Some(PresetColors { bg: rgb(0xF4, 0xF7, 0xFB), surface: rgb(0xFF, 0xFF, 0xFF), raised: rgb(0xE8, 0xEE, 0xF7), text: rgb(0x11, 0x17, 0x22), muted: rgb(0x49, 0x56, 0x6B), primary: rgb(0x00, 0x5F, 0xB8), border: rgb(0xC8, 0xD4, 0xE6), focus: rgb(0x00, 0x5F, 0xB8), danger: rgb(0xB4, 0x23, 0x42), radii_sm: 6, radii_md: 10 }),
            ("macLiquid", true) => Some(PresetColors { bg: rgb(0x07, 0x09, 0x11), surface: rgb(0x10, 0x14, 0x24), raised: rgb(0x1E, 0x25, 0x40), text: rgb(0xF2, 0xF5, 0xFF), muted: rgb(0xA6, 0xB0, 0xCE), primary: rgb(0x7A, 0xA2, 0xF7), border: rgb(0x30, 0x3B, 0x60), focus: rgb(0xA9, 0xC4, 0xFF), danger: rgb(0xFF, 0x7A, 0x93), radii_sm: 8, radii_md: 14 }),
            ("macLiquid", false) => Some(PresetColors { bg: rgb(0xF6, 0xF7, 0xFC), surface: rgb(0xFF, 0xFF, 0xFF), raised: rgb(0xEB, 0xEE, 0xF9), text: rgb(0x10, 0x14, 0x26), muted: rgb(0x49, 0x51, 0x6E), primary: rgb(0x25, 0x46, 0xB8), border: rgb(0xCC, 0xD3, 0xEB), focus: rgb(0x25, 0x46, 0xB8), danger: rgb(0xB4, 0x23, 0x42), radii_sm: 8, radii_md: 14 }),
            ("sakuraNody", true) => Some(PresetColors { bg: rgb(0x12, 0x09, 0x10), surface: rgb(0x1C, 0x10, 0x19), raised: rgb(0x32, 0x1D, 0x2D), text: rgb(0xFC, 0xF0, 0xF7), muted: rgb(0xC8, 0xA6, 0xBA), primary: rgb(0xFF, 0x85, 0xB3), border: rgb(0x4D, 0x2E, 0x45), focus: rgb(0xFF, 0xB3, 0xD1), danger: rgb(0xFF, 0x75, 0x8F), radii_sm: 8, radii_md: 12 }),
            ("sakuraNody", false) => Some(PresetColors { bg: rgb(0xFD, 0xF6, 0xF9), surface: rgb(0xFF, 0xFF, 0xFF), raised: rgb(0xF7, 0xE8, 0xEF), text: rgb(0x22, 0x10, 0x1A), muted: rgb(0x66, 0x46, 0x58), primary: rgb(0x9C, 0x19, 0x53), border: rgb(0xE3, 0xC8, 0xD6), focus: rgb(0x9C, 0x19, 0x53), danger: rgb(0xB4, 0x23, 0x42), radii_sm: 8, radii_md: 12 }),
            ("bakkaGold", true) => Some(PresetColors { bg: rgb(0x07, 0x07, 0x06), surface: rgb(0x11, 0x10, 0x0D), raised: rgb(0x24, 0x20, 0x18), text: rgb(0xF9, 0xF5, 0xEB), muted: rgb(0xBB, 0xAE, 0x96), primary: rgb(0xF0, 0xC4, 0x4D), border: rgb(0x3D, 0x35, 0x24), focus: rgb(0xFF, 0xE0, 0x82), danger: rgb(0xFF, 0x78, 0x8A), radii_sm: 6, radii_md: 10 }),
            ("bakkaGold", false) => Some(PresetColors { bg: rgb(0xFB, 0xF9, 0xF3), surface: rgb(0xFF, 0xFF, 0xFF), raised: rgb(0xF3, 0xED, 0xE0), text: rgb(0x18, 0x15, 0x0E), muted: rgb(0x5C, 0x53, 0x40), primary: rgb(0x75, 0x53, 0x08), border: rgb(0xDD, 0xD2, 0xBC), focus: rgb(0x75, 0x53, 0x08), danger: rgb(0xB4, 0x23, 0x42), radii_sm: 6, radii_md: 10 }),
            ("catVelvet", true) => Some(PresetColors { bg: rgb(0x10, 0x0B, 0x0E), surface: rgb(0x1A, 0x12, 0x17), raised: rgb(0x2E, 0x20, 0x29), text: rgb(0xFA, 0xF0, 0xF5), muted: rgb(0xC2, 0xA7, 0xB5), primary: rgb(0xFF, 0xA0, 0x7A), border: rgb(0x47, 0x32, 0x3F), focus: rgb(0xFF, 0xC1, 0xA6), danger: rgb(0xFF, 0x78, 0x90), radii_sm: 8, radii_md: 12 }),
            ("catVelvet", false) => Some(PresetColors { bg: rgb(0xFC, 0xF7, 0xF9), surface: rgb(0xFF, 0xFF, 0xFF), raised: rgb(0xF5, 0xE9, 0xEE), text: rgb(0x1F, 0x13, 0x19), muted: rgb(0x61, 0x49, 0x55), primary: rgb(0x8C, 0x2D, 0x19), border: rgb(0xDF, 0xCC, 0xD5), focus: rgb(0x8C, 0x2D, 0x19), danger: rgb(0xB4, 0x23, 0x42), radii_sm: 8, radii_md: 12 }),
            ("retroMatrix", true) => Some(PresetColors { bg: rgb(0x05, 0x0B, 0x07), surface: rgb(0x0B, 0x16, 0x0F), raised: rgb(0x16, 0x2B, 0x1E), text: rgb(0xE6, 0xF9, 0xEC), muted: rgb(0x94, 0xBD, 0x9F), primary: rgb(0x4A, 0xDE, 0x80), border: rgb(0x24, 0x47, 0x31), focus: rgb(0x86, 0xEF, 0xAC), danger: rgb(0xFF, 0x78, 0x8E), radii_sm: 2, radii_md: 4 }),
            ("retroMatrix", false) => Some(PresetColors { bg: rgb(0xF2, 0xF8, 0xF4), surface: rgb(0xFF, 0xFF, 0xFF), raised: rgb(0xE3, 0xEF, 0xE7), text: rgb(0x0B, 0x1B, 0x10), muted: rgb(0x3F, 0x59, 0x47), primary: rgb(0x0F, 0x62, 0x2C), border: rgb(0xC0, 0xD8, 0xC7), focus: rgb(0x0F, 0x62, 0x2C), danger: rgb(0xB4, 0x23, 0x42), radii_sm: 2, radii_md: 4 }),
            ("oceanAbyss", true) => Some(PresetColors { bg: rgb(0x06, 0x0D, 0x13), surface: rgb(0x0D, 0x18, 0x22), raised: rgb(0x1A, 0x2E, 0x3F), text: rgb(0xEC, 0xF6, 0xFC), muted: rgb(0x9E, 0xB6, 0xC7), primary: rgb(0x2D, 0xD4, 0xBF), border: rgb(0x27, 0x44, 0x5C), focus: rgb(0x5E, 0xEA, 0xD4), danger: rgb(0xFF, 0x78, 0x8E), radii_sm: 6, radii_md: 10 }),
            ("oceanAbyss", false) => Some(PresetColors { bg: rgb(0xF3, 0xF9, 0xFB), surface: rgb(0xFF, 0xFF, 0xFF), raised: rgb(0xE5, 0xF2, 0xF6), text: rgb(0x0C, 0x1A, 0x24), muted: rgb(0x42, 0x58, 0x66), primary: rgb(0x0B, 0x64, 0x70), border: rgb(0xC2, 0xD9, 0xE0), focus: rgb(0x0B, 0x64, 0x70), danger: rgb(0xB4, 0x23, 0x42), radii_sm: 6, radii_md: 10 }),
            ("oledPitch", true) => Some(PresetColors { bg: rgb(0x00, 0x00, 0x00), surface: rgb(0x09, 0x09, 0x0B), raised: rgb(0x18, 0x18, 0x1C), text: rgb(0xFA, 0xFA, 0xFC), muted: rgb(0xA6, 0xA6, 0xB0), primary: rgb(0xFF, 0xD4, 0x37), border: rgb(0x2E, 0x2E, 0x36), focus: rgb(0xFF, 0xE3, 0x73), danger: rgb(0xFF, 0x75, 0x88), radii_sm: 6, radii_md: 10 }),
            ("oledPitch", false) => Some(PresetColors { bg: rgb(0xF8, 0xF8, 0xFA), surface: rgb(0xFF, 0xFF, 0xFF), raised: rgb(0xEC, 0xEC, 0xF0), text: rgb(0x0E, 0x0E, 0x12), muted: rgb(0x4D, 0x4D, 0x57), primary: rgb(0x1F, 0x1F, 0x29), border: rgb(0xCF, 0xCF, 0xD8), focus: rgb(0x1F, 0x1F, 0x29), danger: rgb(0xB4, 0x23, 0x42), radii_sm: 6, radii_md: 10 }),
            ("sunsetDune", true) => Some(PresetColors { bg: rgb(0x11, 0x09, 0x07), surface: rgb(0x1B, 0x10, 0x0D), raised: rgb(0x30, 0x1E, 0x19), text: rgb(0xFA, 0xF1, 0xEE), muted: rgb(0xC2, 0xA8, 0xA0), primary: rgb(0xFF, 0x7A, 0x45), border: rgb(0x4A, 0x2E, 0x26), focus: rgb(0xFF, 0xA9, 0x85), danger: rgb(0xFF, 0x78, 0x90), radii_sm: 6, radii_md: 10 }),
            ("sunsetDune", false) => Some(PresetColors { bg: rgb(0xFC, 0xF7, 0xF5), surface: rgb(0xFF, 0xFF, 0xFF), raised: rgb(0xF5, 0xEA, 0xE5), text: rgb(0x21, 0x12, 0x0E), muted: rgb(0x61, 0x49, 0x42), primary: rgb(0x94, 0x29, 0x11), border: rgb(0xE0, 0xCA, 0xC2), focus: rgb(0x94, 0x29, 0x11), danger: rgb(0xB4, 0x23, 0x42), radii_sm: 6, radii_md: 10 }),
            ("emeraldMushaf", true) => Some(PresetColors { bg: rgb(0x06, 0x0F, 0x0B), surface: rgb(0x0C, 0x1B, 0x14), raised: rgb(0x18, 0x32, 0x26), text: rgb(0xEE, 0xF8, 0xF2), muted: rgb(0x9F, 0xBA, 0xB0), primary: rgb(0x4A, 0xDE, 0x80), border: rgb(0x26, 0x4A, 0x39), focus: rgb(0x86, 0xEF, 0xAC), danger: rgb(0xFF, 0x78, 0x8E), radii_sm: 6, radii_md: 10 }),
            ("emeraldMushaf", false) => Some(PresetColors { bg: rgb(0xF6, 0xFA, 0xF7), surface: rgb(0xFF, 0xFF, 0xFF), raised: rgb(0xE7, 0xF2, 0xEB), text: rgb(0x0C, 0x1C, 0x14), muted: rgb(0x42, 0x59, 0x4D), primary: rgb(0x0A, 0x61, 0x3B), border: rgb(0xC4, 0xDD, 0xD0), focus: rgb(0x0A, 0x61, 0x3B), danger: rgb(0xB4, 0x23, 0x42), radii_sm: 6, radii_md: 10 }),
            _ => None,
        }
    }
}

impl Tokens {
    /// Resolves any of the 20 canonical `GemiTokens` presets (`PRESET_IDS`) into `Tokens`
    /// while strictly locking Program/Source video monitors (`NEUTRAL_MEDIA_VIEWPORT = RGB(24,24,24)`)
    /// and Lumetri scope plots (`NEUTRAL_SCOPE_PLOT = RGB(18,18,18)`) to color-neutral gray.
    pub fn for_gemi_preset(preset_id: &str, dark: bool) -> Option<Self> {
        let p = gemi_tokens_v1::lookup_preset(preset_id, dark)?;
        let base = Self::for_kind(if dark {
            ThemeKind::InfernoDark
        } else {
            ThemeKind::InfernoLight
        });
        Some(Self {
            app_bg: p.bg,
            header_bg: p.surface,
            panel_bg: p.surface,
            tab_bg: p.bg,
            tab_text: p.muted,
            focus: p.focus,
            accent: p.primary,
            text: p.text,
            text_dim: p.muted,
            field_bg: p.raised,
            field_border: p.border,
            separator: p.border,
            hot_text: p.focus,
            playhead: p.primary,
            timecode: p.focus,
            danger: p.danger,
            monitor_bg: gemi_tokens_v1::NEUTRAL_MEDIA_VIEWPORT,
            plot_bg: gemi_tokens_v1::NEUTRAL_SCOPE_PLOT,
            radius_sm: f32::from(p.radii_sm),
            radius: f32::from(p.radii_md),
            ..base
        })
    }
}
