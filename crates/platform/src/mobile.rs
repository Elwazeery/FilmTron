//! Mobile platform capability descriptors, thermal/memory pressure budgets, and pure-Rust fallback
//! routing for Android ARM64 (`MediaCodec` + `Vulkan`) and iOS ARM64 (`VideoToolbox` + `Metal`).
//!
//! Hardware acceleration on mobile is strictly capability-gated: when a device or codec profile is
//! unsupported, or when the OS reports `Serious`/`Critical` thermal state or memory pressure, the
//! runtime transparently falls back to FilmCraft's deterministic pure-Rust software decoders and
//! CPU renderer without crashing or losing user work.

use serde::{Deserialize, Serialize};

/// Mobile operating system target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MobileOsKind {
    AndroidArm64,
    IosArm64,
    HostSimulation,
}

impl MobileOsKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AndroidArm64 => "android-arm64",
            Self::IosArm64 => "ios-arm64",
            Self::HostSimulation => "host-simulation",
        }
    }
}

/// Mobile GPU compute/compositing backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MobileGpuBackend {
    Vulkan,
    Metal,
    CpuFallback,
}

impl MobileGpuBackend {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Vulkan => "vulkan",
            Self::Metal => "metal",
            Self::CpuFallback => "cpu-fallback",
        }
    }
}

/// Mobile video codec acceleration backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MobileCodecBackend {
    AndroidMediaCodec,
    IosVideoToolbox,
    PureRustSoftware,
}

impl MobileCodecBackend {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AndroidMediaCodec => "android-mediacodec",
            Self::IosVideoToolbox => "ios-videotoolbox",
            Self::PureRustSoftware => "pure-rust-software",
        }
    }
}

/// Device thermal state reported by Android `PowerManager` or iOS `ProcessInfo.ThermalState`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MobileThermalState {
    #[default]
    Nominal,
    Fair,
    Serious,
    Critical,
}

/// OS memory pressure state reported by Android `ComponentCallbacks2` (`onTrimMemory`) or iOS.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MobileMemoryPressure {
    #[default]
    Normal,
    Moderate,
    Critical,
}

/// Capability snapshot probed by the mobile host shell (Kotlin on Android, Swift on iOS).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MobileHardwareCapabilities {
    pub os: MobileOsKind,
    pub device_model: String,
    pub soc_vendor: String,
    pub total_ram_mib: u32,
    pub vulkan_compute_supported: bool,
    pub metal_compute_supported: bool,
    pub hw_h264_decode: bool,
    pub hw_hevc_decode: bool,
    pub hw_h264_encode: bool,
    pub max_hw_width: u32,
    pub max_hw_height: u32,
}

impl MobileHardwareCapabilities {
    /// Default capability profile for Android ARM64 devices with MediaCodec + Vulkan 1.1.
    pub fn android_arm64_default(device_model: &str, total_ram_mib: u32) -> Self {
        Self {
            os: MobileOsKind::AndroidArm64,
            device_model: device_model.to_owned(),
            soc_vendor: "arm64-v8a".to_owned(),
            total_ram_mib: total_ram_mib.max(2048),
            vulkan_compute_supported: true,
            metal_compute_supported: false,
            hw_h264_decode: true,
            hw_hevc_decode: true,
            hw_h264_encode: true,
            max_hw_width: 3840,
            max_hw_height: 2160,
        }
    }

    /// Safe baseline profile where only pure-Rust software decode/encode and CPU compositing run.
    pub fn pure_rust_baseline(os: MobileOsKind, device_model: &str, total_ram_mib: u32) -> Self {
        Self {
            os,
            device_model: device_model.to_owned(),
            soc_vendor: "generic-arm64".to_owned(),
            total_ram_mib: total_ram_mib.max(1024),
            vulkan_compute_supported: false,
            metal_compute_supported: false,
            hw_h264_decode: false,
            hw_hevc_decode: false,
            hw_h264_encode: false,
            max_hw_width: 1920,
            max_hw_height: 1080,
        }
    }
}

/// Negotiated runtime execution budget and active backends for a mobile editing session.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MobileRuntimeBudget {
    pub gpu_backend: MobileGpuBackend,
    pub decode_backend: MobileCodecBackend,
    pub encode_backend: MobileCodecBackend,
    pub preview_max_width: u32,
    pub preview_max_height: u32,
    pub export_max_width: u32,
    pub export_max_height: u32,
    pub frame_cache_cap_mib: u32,
    pub thumbnail_cache_cap_mib: u32,
    pub max_decode_workers: u8,
    pub proxy_quarter_res_forced: bool,
    pub fallback_reason: Option<String>,
}

/// Negotiate the safest mobile runtime budget given hardware capabilities, thermal state, and
/// OS memory pressure.
pub fn negotiate_mobile_runtime_budget(
    caps: &MobileHardwareCapabilities,
    thermal: MobileThermalState,
    memory: MobileMemoryPressure,
    prefer_hardware: bool,
) -> MobileRuntimeBudget {
    let thermal_throttled = matches!(
        thermal,
        MobileThermalState::Serious | MobileThermalState::Critical
    );
    let memory_critical = memory == MobileMemoryPressure::Critical;

    let mut fallback_notes = Vec::new();
    if !prefer_hardware {
        fallback_notes.push("hardware acceleration disabled by user preference".to_owned());
    }
    if thermal_throttled {
        fallback_notes.push(format!("thermal state {:?} triggered software/proxy safety mode", thermal));
    }
    if memory_critical {
        fallback_notes.push("critical OS memory pressure reduced frame cache and worker pool".to_owned());
    }

    let gpu_backend = if !prefer_hardware || thermal == MobileThermalState::Critical {
        MobileGpuBackend::CpuFallback
    } else if caps.os == MobileOsKind::AndroidArm64 && caps.vulkan_compute_supported {
        MobileGpuBackend::Vulkan
    } else if caps.os == MobileOsKind::IosArm64 && caps.metal_compute_supported {
        MobileGpuBackend::Metal
    } else {
        fallback_notes.push("GPU compute unavailable; using pure-Rust CPU compositor".to_owned());
        MobileGpuBackend::CpuFallback
    };

    let decode_backend = if !prefer_hardware || thermal == MobileThermalState::Critical {
        MobileCodecBackend::PureRustSoftware
    } else if caps.os == MobileOsKind::AndroidArm64 && caps.hw_h264_decode {
        MobileCodecBackend::AndroidMediaCodec
    } else if caps.os == MobileOsKind::IosArm64 && caps.hw_h264_decode {
        MobileCodecBackend::IosVideoToolbox
    } else {
        fallback_notes.push("OS hardware decoder unavailable; using pure-Rust H.264/MP4 decoder".to_owned());
        MobileCodecBackend::PureRustSoftware
    };

    let encode_backend = if !prefer_hardware || thermal_throttled {
        MobileCodecBackend::PureRustSoftware
    } else if caps.os == MobileOsKind::AndroidArm64 && caps.hw_h264_encode {
        MobileCodecBackend::AndroidMediaCodec
    } else if caps.os == MobileOsKind::IosArm64 && caps.hw_h264_encode {
        MobileCodecBackend::IosVideoToolbox
    } else {
        MobileCodecBackend::PureRustSoftware
    };

    let (preview_max_width, preview_max_height, proxy_quarter_res_forced) =
        if thermal_throttled || memory_critical || caps.total_ram_mib < 4096 {
            (960, 540, true)
        } else {
            (1920, 1080, false)
        };

    let base_cache_mib = (caps.total_ram_mib / 16).clamp(48, 256);
    let frame_cache_cap_mib = match memory {
        MobileMemoryPressure::Normal => base_cache_mib,
        MobileMemoryPressure::Moderate => (base_cache_mib / 2).max(32),
        MobileMemoryPressure::Critical => 24,
    };
    let thumbnail_cache_cap_mib = (frame_cache_cap_mib / 3).max(12);

    let max_decode_workers = if thermal_throttled || memory_critical {
        1
    } else if caps.total_ram_mib >= 6144 {
        3
    } else {
        2
    };

    MobileRuntimeBudget {
        gpu_backend,
        decode_backend,
        encode_backend,
        preview_max_width,
        preview_max_height,
        export_max_width: 1920,
        export_max_height: 1080,
        frame_cache_cap_mib,
        thumbnail_cache_cap_mib,
        max_decode_workers,
        proxy_quarter_res_forced,
        fallback_reason: (!fallback_notes.is_empty()).then(|| fallback_notes.join("; ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn android_arm64_negotiates_vulkan_and_mediacodec_under_nominal_conditions() {
        let caps = MobileHardwareCapabilities::android_arm64_default("Pixel 8 Pro", 8192);
        let budget = negotiate_mobile_runtime_budget(
            &caps,
            MobileThermalState::Nominal,
            MobileMemoryPressure::Normal,
            true,
        );
        assert_eq!(budget.gpu_backend, MobileGpuBackend::Vulkan);
        assert_eq!(budget.decode_backend, MobileCodecBackend::AndroidMediaCodec);
        assert_eq!(budget.encode_backend, MobileCodecBackend::AndroidMediaCodec);
        assert_eq!(budget.preview_max_width, 1920);
        assert_eq!(budget.export_max_width, 1920);
        assert!(!budget.proxy_quarter_res_forced);
        assert!(budget.fallback_reason.is_none());
    }

    #[test]
    fn critical_thermal_and_memory_pressure_fall_back_to_pure_rust_cpu() {
        let caps = MobileHardwareCapabilities::android_arm64_default("Galaxy S23", 6144);
        let budget = negotiate_mobile_runtime_budget(
            &caps,
            MobileThermalState::Critical,
            MobileMemoryPressure::Critical,
            true,
        );
        assert_eq!(budget.gpu_backend, MobileGpuBackend::CpuFallback);
        assert_eq!(budget.decode_backend, MobileCodecBackend::PureRustSoftware);
        assert_eq!(budget.encode_backend, MobileCodecBackend::PureRustSoftware);
        assert_eq!(budget.preview_max_width, 960);
        assert_eq!(budget.frame_cache_cap_mib, 24);
        assert_eq!(budget.max_decode_workers, 1);
        assert!(budget.proxy_quarter_res_forced);
        assert!(budget.fallback_reason.is_some());
    }
}
