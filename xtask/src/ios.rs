//! `cargo xtask ios [--bundle] [--sim]`: the iPad app (`apps/cadcraft-ios`).
//!
//! Without flags: `cargo clippy -p cadcraft-ios --target aarch64-apple-ios -D warnings`, which
//! needs only the target's standard library (`rustup target add aarch64-apple-ios`), so it runs
//! on any host and keeps the iPad build from breaking unnoticed (part of `cargo xtask ci`).
//!
//! `--bundle` builds a release binary and assembles `target/ios/<platform>/CADCraft.app` from
//! `packaging/ios/Info.plist.in`, the binary and the app icon. Linking needs Xcode, so it runs on
//! macOS only. `--sim` builds for the Apple-silicon simulator (`aarch64-apple-ios-sim`) instead of
//! a device. Signing and installing are in `docs/ios.md`.

use std::path::{Path, PathBuf};

use crate::{cargo, root, run};

pub const DEVICE_TARGET: &str = "aarch64-apple-ios";
pub const SIM_TARGET: &str = "aarch64-apple-ios-sim";
/// Must match `MinimumOSVersion` in `packaging/ios/Info.plist.in`.
pub const IPHONEOS_DEPLOYMENT_TARGET: &str = "15.0";

pub fn run_cmd(args: &[&str]) -> Result<(), String> {
    let sim = args.contains(&"--sim");
    if args.contains(&"--bundle") { bundle(sim) } else { check() }
}

/// Whether rustup has `target`'s standard library (`None`: rustup is not there to ask).
fn target_installed(target: &str) -> Option<bool> {
    let out = std::process::Command::new("rustup").args(["target", "list", "--installed"]).output().ok()?;
    Some(String::from_utf8_lossy(&out.stdout).lines().any(|l| l.trim() == target))
}

pub fn check() -> Result<(), String> {
    if target_installed(DEVICE_TARGET) == Some(false) {
        return Err(format!("the iOS target is not installed: run `rustup target add {DEVICE_TARGET}`"));
    }
    let mut c = cargo();
    c.args(["clippy", "-p", "cadcraft-ios", "--target", DEVICE_TARGET, "--", "-D", "warnings"]);
    run(c, &format!("cargo clippy -p cadcraft-ios --target {DEVICE_TARGET} -- -D warnings"))
}

/// `X.Y.Z` of a version like `0.7.0-rc.1` (CFBundleShortVersionString takes digits only).
pub fn short_version(version: &str) -> &str {
    version.split(['-', '+']).next().unwrap_or(version)
}

/// `template` with the `@…@` placeholders filled in.
pub fn fill_plist(template: &str, version: &str, sha: &str, platform: &str) -> String {
    template
        .replace("@SHORT_VERSION@", short_version(version))
        .replace("@VERSION@", version)
        .replace("@BUILD_SHA@", sha)
        .replace("@PLATFORM@", platform)
}

fn bundle(sim: bool) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        return Err("bundling the iPad app links with Xcode's SDK: run `cargo xtask ios --bundle` on macOS".into());
    }
    let (target, platform) = if sim { (SIM_TARGET, "iphonesimulator") } else { (DEVICE_TARGET, "iphoneos") };
    let root = root();
    let mut c = cargo();
    c.env("IPHONEOS_DEPLOYMENT_TARGET", IPHONEOS_DEPLOYMENT_TARGET).args(["build", "--release", "-p", "cadcraft-ios", "--target", target]);
    run(c, &format!("cargo build --release -p cadcraft-ios --target {target}"))?;

    let target_dir = std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from);
    let app = target_dir.join("ios").join(platform).join("CADCraft.app");
    if app.exists() {
        std::fs::remove_dir_all(&app).map_err(|e| format!("{}: {e}", app.display()))?;
    }
    std::fs::create_dir_all(&app).map_err(|e| format!("{}: {e}", app.display()))?;
    let copy = |from: &Path, to: &str| std::fs::copy(from, app.join(to)).map(|_| ()).map_err(|e| format!("{}: {e}", from.display()));
    copy(&target_dir.join(target).join("release").join("cadcraft-ios"), "cadcraft-ios")?;
    copy(&root.join("assets/app-icon/cadcraft-1024.png"), "AppIcon.png")?;

    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).map_err(|e| format!("Cargo.toml: {e}"))?;
    let version = crate::version::read(&manifest)?;
    let sha = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(&root)
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "dev".into());
    let template_path = root.join("packaging/ios/Info.plist.in");
    let template = std::fs::read_to_string(&template_path).map_err(|e| format!("{}: {e}", template_path.display()))?;
    std::fs::write(app.join("Info.plist"), fill_plist(&template, &version, &sha, platform)).map_err(|e| format!("Info.plist: {e}"))?;
    println!("\nbuilt {}", app.display());
    if sim {
        println!("install: xcrun simctl install booted {}\nlaunch:  xcrun simctl launch booted ai.storyteller.cadcraft", app.display());
    } else {
        println!("sign and install on a device: see docs/ios.md");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{IPHONEOS_DEPLOYMENT_TARGET, fill_plist, short_version};

    #[test]
    fn short_versions_are_numeric() {
        assert_eq!(short_version("0.7.0"), "0.7.0");
        assert_eq!(short_version("0.7.0-rc.1"), "0.7.0");
        assert_eq!(short_version("1.2.3+build"), "1.2.3");
    }

    #[test]
    fn the_template_is_filled_and_matches_the_deployment_target() {
        let template = include_str!("../../packaging/ios/Info.plist.in");
        let plist = fill_plist(template, "0.7.0-rc.1", "abc1234", "iphonesimulator");
        // The header comment names the placeholders; none may remain outside it.
        let body = plist.split_once("-->").map_or("", |(_, b)| b);
        assert!(!body.contains('@'), "unfilled placeholder in Info.plist");
        assert!(plist.contains("<string>0.7.0</string>") && plist.contains("<string>0.7.0-rc.1</string>"));
        assert!(plist.contains("<string>abc1234</string>") && plist.contains("<string>iphonesimulator</string>"));
        assert!(plist.contains(&format!("<key>MinimumOSVersion</key>\n  <string>{IPHONEOS_DEPLOYMENT_TARGET}</string>")));
        assert!(plist.contains("<key>CFBundleExecutable</key>\n  <string>cadcraft-ios</string>"));
    }
}
