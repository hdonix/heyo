//! How WebKitGTK draws the interface, chosen before the web view exists.
//!
//! On Linux WebKitGTK's own renderer paints and composites on the GPU and
//! hands frames over as DMA-BUFs allocated through GBM. With NVIDIA's
//! proprietary driver GBM only works when the kernel module runs with
//! `nvidia-drm modeset=1`; without it Mesa's GBM falls back to software and
//! cannot even allocate a buffer ("Failed to create GBM buffer"), leaving a
//! blank window. WebKit offers two ways round that, both set through the
//! environment of the process that starts it:
//!
//! - `WEBKIT_DISABLE_DMABUF_RENDERER=1`: the old path, everything on the CPU
//!   and no accelerated compositing at all (so no `backdrop-filter`, no 3D).
//!   About 16 ms a frame at 1× scale, four times that at 2×.
//! - `WEBKIT_DMABUF_RENDERER_FORCE_SHM=1`: paint and composite on the GPU,
//!   read each frame back into shared memory. Measured on the development
//!   machine at about 36 ms a frame whatever the scale: slower than the CPU
//!   at 1×, faster at 2×, and the only one of the three that keeps the
//!   GPU-only CSS features.
//!
//! `Settings::renderer` lets the user choose; `Auto` keeps the CPU path on
//! an NVIDIA machine without KMS and leaves WebKit alone everywhere else.
//! `scripts/gui-test/scroll.sh` and the notes in `research_notes/gpu/` have
//! the measurements.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export, export_to = "../../shared/bindings/")]
pub enum Renderer {
    #[default]
    Auto,
    Cpu,
    /// The GPU, with frames read back into shared memory.
    GpuSharedMemory,
    /// The GPU, with frames shared as DMA-BUFs: WebKit's default.
    GpuDmaBuf,
}

/// What the graphics driver situation is, as far as the choice needs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Gpu {
    /// NVIDIA's proprietary driver is loaded.
    pub nvidia: bool,
    /// It runs with kernel modesetting, which its GBM backend needs.
    pub nvidia_kms: bool,
}

impl Gpu {
    /// Read from `/sys/class/drm`: the driver of each card, and whether it
    /// has connectors (only a KMS driver exposes any).
    pub fn detect() -> Gpu {
        let Ok(cards) = std::fs::read_dir("/sys/class/drm") else { return Gpu::default() };
        let cards = cards.flatten().filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with("card") || name.contains('-') {
                return None;
            }
            let driver = std::fs::read_link(entry.path().join("device/driver")).ok()?;
            let driver = driver.file_name()?.to_string_lossy().into_owned();
            let connectors = std::fs::read_dir(entry.path()).ok()?.flatten().filter(|c| c.file_name().to_string_lossy().starts_with(&format!("{name}-"))).count();
            Some((driver, connectors))
        });
        Gpu::from_cards(cards)
    }

    fn from_cards(cards: impl IntoIterator<Item = (String, usize)>) -> Gpu {
        let mut gpu = Gpu::default();
        for (driver, connectors) in cards {
            if driver == "nvidia" {
                gpu.nvidia = true;
                gpu.nvidia_kms |= connectors > 0;
            }
        }
        gpu
    }
}

/// The environment the choice comes to.
pub fn environment(renderer: Renderer, gpu: Gpu) -> &'static [(&'static str, &'static str)] {
    const CPU: &[(&str, &str)] = &[("WEBKIT_DISABLE_DMABUF_RENDERER", "1")];
    // The second one keeps Ubuntu's build from turning acceleration off when
    // it notices the NVIDIA driver.
    const SHM: &[(&str, &str)] = &[("WEBKIT_DMABUF_RENDERER_FORCE_SHM", "1"), ("WEBKIT_FORCE_DMABUF_RENDERER", "1")];
    const DMABUF: &[(&str, &str)] = &[("WEBKIT_FORCE_DMABUF_RENDERER", "1")];
    match renderer {
        Renderer::Auto if gpu.nvidia && !gpu.nvidia_kms => CPU,
        Renderer::Auto => &[],
        Renderer::Cpu => CPU,
        Renderer::GpuSharedMemory => SHM,
        Renderer::GpuDmaBuf => DMABUF,
    }
}

/// A line for the log and the diagnostics.
pub fn describe(renderer: Renderer, gpu: Gpu) -> String {
    let env = environment(renderer, gpu);
    let has = |wanted: &str| env.iter().any(|(name, _)| *name == wanted);
    let chosen = if has("WEBKIT_DISABLE_DMABUF_RENDERER") {
        "CPU"
    } else if has("WEBKIT_DMABUF_RENDERER_FORCE_SHM") {
        "GPU, shared memory"
    } else if env.is_empty() {
        "WebKit's default"
    } else {
        "GPU, DMA-BUF"
    };
    let driver = match (gpu.nvidia, gpu.nvidia_kms) {
        (true, true) => "NVIDIA with modesetting",
        (true, false) => "NVIDIA without modesetting (nvidia-drm modeset=0)",
        _ => "not NVIDIA",
    };
    format!("renderer: {chosen} (setting {renderer:?}; driver {driver})")
}

/// Sets the environment for the web view, unless the user set any of it
/// already. Called before anything of Tauri runs.
#[cfg(target_os = "linux")]
pub fn apply() {
    const OURS: &[&str] = &["WEBKIT_DISABLE_DMABUF_RENDERER", "WEBKIT_DMABUF_RENDERER_FORCE_SHM", "WEBKIT_FORCE_DMABUF_RENDERER", "WEBKIT_DMABUF_RENDERER_DISABLE_GBM"];
    if OURS.iter().any(|name| std::env::var_os(name).is_some()) {
        eprintln!("heyo: renderer: left to the environment");
        return;
    }
    let renderer = crate::settings::Settings::load_from(&crate::settings::settings_path()).0.renderer;
    let gpu = Gpu::detect();
    for (name, value) in environment(renderer, gpu) {
        std::env::set_var(name, value);
    }
    // Said again once the log exists (`lib.rs`): this runs before it does.
    CHOSEN.set(describe(renderer, gpu)).ok();
}

static CHOSEN: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// What `apply` decided, for the log.
pub fn chosen() -> String {
    CHOSEN.get().cloned().unwrap_or_else(|| "renderer: left to the environment".into())
}

#[cfg(not(target_os = "linux"))]
pub fn apply() {}

#[cfg(test)]
mod tests {
    use super::*;

    const NVIDIA_NO_KMS: Gpu = Gpu { nvidia: true, nvidia_kms: false };
    const NVIDIA_KMS: Gpu = Gpu { nvidia: true, nvidia_kms: true };
    const OTHER: Gpu = Gpu { nvidia: false, nvidia_kms: false };

    #[test]
    fn automatic_avoids_gbm_only_where_it_cannot_work() {
        assert_eq!(environment(Renderer::Auto, NVIDIA_NO_KMS), [("WEBKIT_DISABLE_DMABUF_RENDERER", "1")]);
        assert!(environment(Renderer::Auto, NVIDIA_KMS).is_empty());
        assert!(environment(Renderer::Auto, OTHER).is_empty());
    }

    #[test]
    fn a_choice_is_followed_whatever_the_driver() {
        for gpu in [NVIDIA_NO_KMS, NVIDIA_KMS, OTHER] {
            assert_eq!(environment(Renderer::Cpu, gpu), [("WEBKIT_DISABLE_DMABUF_RENDERER", "1")]);
            assert!(environment(Renderer::GpuSharedMemory, gpu).iter().any(|(n, _)| *n == "WEBKIT_DMABUF_RENDERER_FORCE_SHM"));
            assert_eq!(environment(Renderer::GpuDmaBuf, gpu), [("WEBKIT_FORCE_DMABUF_RENDERER", "1")]);
        }
    }

    #[test]
    fn the_driver_is_read_from_the_cards() {
        let cards = |list: &[(&str, usize)]| list.iter().map(|(d, c)| (d.to_string(), *c)).collect::<Vec<_>>();
        // The development machine: a framebuffer console and NVIDIA without KMS.
        assert_eq!(Gpu::from_cards(cards(&[("simple-framebuffer", 1), ("nvidia", 0)])), NVIDIA_NO_KMS);
        assert_eq!(Gpu::from_cards(cards(&[("nvidia", 5)])), NVIDIA_KMS);
        assert_eq!(Gpu::from_cards(cards(&[("amdgpu", 3)])), OTHER);
        assert_eq!(Gpu::from_cards(cards(&[])), OTHER);
    }

    #[test]
    fn the_description_names_what_was_chosen() {
        assert_eq!(describe(Renderer::Auto, NVIDIA_NO_KMS), "renderer: CPU (setting Auto; driver NVIDIA without modesetting (nvidia-drm modeset=0))");
        assert!(describe(Renderer::GpuSharedMemory, NVIDIA_NO_KMS).starts_with("renderer: GPU, shared memory"));
        assert!(describe(Renderer::Auto, OTHER).starts_with("renderer: WebKit's default"));
        assert!(describe(Renderer::GpuDmaBuf, NVIDIA_KMS).starts_with("renderer: GPU, DMA-BUF"));
    }

    #[test]
    fn export_bindings() {
        // ts-rs exports on `cargo test` through the derive; nothing to do but exist.
    }
}
