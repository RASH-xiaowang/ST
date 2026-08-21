fn main() {
    tauri_build::build();

    // Windows/MSVC 下给所有二进制嵌入 Common Controls v6 清单。
    // 原因：rfd(tauri-plugin-dialog) 静态导入 comctl32!TaskDialogIndirect，
    // 该函数只存在于 v6（WinSxS）。主程序已有 tauri-winres 嵌入的 v6 清单，
    // 但 cargo test 的库单测 harness 没有，加载器绑定到 System32 v5.82 后
    // 报 STATUS_ENTRYPOINT_NOT_FOUND (0xC0000139)。
    // 注意不能用 rustc-link-arg-tests：它只作用于 tests/ 集成测试，
    // 不作用于 `cargo test --lib` 的 harness（target kind 是 lib）。
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        let manifest =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("windows-app-manifest.tests.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
        // 主程序清单由 tauri-winres 的资源对象提供，若链接器再嵌一份会报
        // CVT1100 资源重复；仅对 bin 关闭链接器嵌入（依赖顺序在通用参数之后）。
        println!("cargo:rustc-link-arg-bins=/MANIFEST:NO");
    }
}
