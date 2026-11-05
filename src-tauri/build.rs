fn main() {
    // Tauri embeds this manifest only into the binary it links; the lib's unit-test harness links
    // the same comctl32 v6 imports (`TaskDialogIndirect`) and dies with STATUS_ENTRYPOINT_NOT_FOUND
    // without one. `compile_for_everything` is what reaches that harness — plain `compile` only
    // covers bins — and `manifest_required()` turns a silent skip into a build failure.
    let attributes = tauri_build::Attributes::new()
        .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
    tauri_build::try_build(attributes).expect("failed to run tauri-build");

    #[cfg(windows)]
    embed_resource::compile_for_everything("app.manifest.rc", embed_resource::NONE)
        .manifest_required()
        .expect("the app manifest must be embedded in every link target");
}
