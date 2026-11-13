//! Checks that egui can render frames headlessly (software Vulkan in CI).

use egui_kittest::Harness;

#[test]
fn renders_a_frame() {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(240.0, 80.0))
        .wgpu()
        .build_ui(|ui| {
            ui.heading("Git Raccoon");
        });
    harness.run();
    let image = harness.render().expect("render");
    assert_eq!((image.width(), image.height()), (240, 80));
}
