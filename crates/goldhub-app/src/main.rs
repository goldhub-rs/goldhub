//! GoldHub Process Entrypoint.

use goldhub_app::build_goldhub_app;
use proc_safety::ProcessSafetyBuilder;

fn main() {
    // 1. Process Hardening & Zero-Trust Security Boundary
    if let Err(err) = ProcessSafetyBuilder::default()
        .hide_current_thread()
        .disable_core_dumps()
        .best_effort(true)
        .commit()
    {
        eprintln!("[WARN] Process safety non-fatal warning: {err}");
    }

    // 2. Logging & Diagnostics
    tracing_subscriber::fmt::init();
    tracing::info!("Booting GoldHub Engine Shell...");

    // 3. Launch Bevy Engine Runtime
    build_goldhub_app().run();
}
