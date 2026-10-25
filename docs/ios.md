# CADCraft on iPad

> **Status:** first build, 2026-10-25. Builds and type-checks for iPadOS (`cargo xtask ios`).
> The macOS CI job (`.github/workflows/ipad.yml`) builds it for the simulator and launches it;
> nobody has used it on a device yet.

`apps/cadcraft-ios` runs the same app as the desktop and web builds (`cadcraft_ui_egui::CadApp`:
the drafting UI, command line, menus, ribbon and palettes), through eframe on winit's UIKit
backend with wgpu on Metal. The engine, file formats and AutoLISP are unchanged. With a Magic
Keyboard or another keyboard and trackpad attached, it works like the Mac app: command line,
shortcuts, hover, right-click.

## What is different on iPad

| Area | iPad behaviour | Where |
|---|---|---|
| Touch | One finger or the Apple Pencil is the pointer: a tap places a point or picks. Two fingers pan and pinch-zoom about their centre. A long press is a right-click: Enter while a command runs, otherwise the shortcut menu. Nothing the first finger does during a two-finger gesture places a point or opens a selection window. | `crates/ui-egui/src/touch.rs` (also used by the web build on touch screens) |
| Touch sizes | While the last input was a finger, pick boxes, grips and object snap targets are 2.5 times their PICKBOX, GRIPSIZE and APERTURE sizes. Moving a mouse or trackpad pointer restores them. The system variables keep their values, so drawings and profiles are unaffected. On iPad, buttons, rows and scroll bars are also finger-sized. | `Session::set_pointer_scale` (engine), `touch.rs`, `theme::apply_touch` |
| On-screen keyboard | A **Keyboard** button sits left of the command line. It opens the input strip at the top of the drawing area, clear of the system keyboard, and brings the system keyboard up. The strip shows the prompt and what is typed, has drafting keys (`@ < , . -`, Enter, Esc) and the recent commands, and Done closes it. Its keys go through the command line like hardware keystrokes. Enter is on the strip because the iPad keyboard's return key sends a line feed, which egui-winit drops (Space also ends input, as in AutoCAD). | `crates/ui-egui/src/softkeys.rs` (also on touch screens in the browser) |
| Files | File ▸ Open… lists the drawings (DWG, DXF, DWT, SVG, LSP) in the app's Documents folder. The Files app shows that folder as **On My iPad ▸ CADCraft**: copy, AirDrop or save drawings there. Save As saves there under the chosen name. | `apps/cadcraft-ios/src/documents.rs`, `UIFileSharingEnabled` in `packaging/ios/Info.plist.in` |
| Lifecycle | When the app goes to the background, preferences are saved and unsaved drawings are autosaved. If iPadOS ends the app, Drawing Recovery offers them at the next launch. | `Shell::save` in `apps/cadcraft-ios/src/main.rs` |
| Storage | Settings (`app.ron`), autosave files and `support/` (startup LISP) are kept in the sandbox's `Library/Application Support/CADCraft`. | `apps/cadcraft-ios/src/paths.rs` |
| Not on iPad | Native menu bar (the in-window menus and ribbon are used), system file dialogs, SpaceMouse, the control server, quitting from inside the app. | |

## Build

Any host can check the build. This needs only the target's standard library and is part of
`cargo xtask ci`:

```sh
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
cargo xtask ios            # cargo clippy -p cadcraft-ios --target aarch64-apple-ios -D warnings
```

Building the app needs macOS with Xcode (it links against the iPadOS SDK):

```sh
cargo xtask ios --bundle --sim    # target/ios/iphonesimulator/CADCraft.app
xcrun simctl boot "iPad Pro 13-inch (M4)"   # or any iPad simulator: xcrun simctl list devices
xcrun simctl install booted target/ios/iphonesimulator/CADCraft.app
xcrun simctl launch --console booted ai.storyteller.cadcraft
```

`packaging/ios/simulator-smoke.sh` does the simulator steps for you. It picks the newest iPad
simulator, installs and launches the app, checks it is still running after 30 seconds, and
saves a screenshot, the app's log and any crash report under `target/ios/smoke`.

### CI

`.github/workflows/ipad.yml` runs on a macOS runner when a change touches the iPad app, the UI
crate, the iOS packaging or `Cargo.lock`, and weekly for everything else. It runs
`cargo xtask ios`, builds the simulator bundle and runs the smoke test. The screenshot and logs
are uploaded as the `ipad-simulator-smoke` artifact, even when the test fails.

On a device (`cargo xtask ios --bundle` builds `target/ios/iphoneos/CADCraft.app`), the bundle
must be signed with your team's identity and a provisioning profile for
`ai.storyteller.cadcraft` (or change `CFBundleIdentifier`):

```sh
cp ~/Downloads/CADCraft.mobileprovision target/ios/iphoneos/CADCraft.app/embedded.mobileprovision
codesign --force --sign "Apple Development: Your Name (TEAMID)" --entitlements entitlements.plist \
  target/ios/iphoneos/CADCraft.app
xcrun devicectl device install app --device <device id> target/ios/iphoneos/CADCraft.app
```

`entitlements.plist` needs `application-identifier` and `com.apple.developer.team-identifier`
for your team. App Store builds also need an asset-catalog icon (`actool`), a privacy manifest
(`PrivacyInfo.xcprivacy`) and an `.ipa`. These are not automated yet.

## Next steps

Ranked. Hours are Opus 5.5 agent-hours, as in [gaps.md](gaps.md), plus a human with an iPad.

| # | Work | Est. |
|---|---|---|
| 1 | First run on the simulator and a device: fix what breaks at launch (surface, safe areas, keyboard), take screenshots | 2–4 + human |
| 2 | On-screen keyboard: check the input strip on a device (keyboard shows and hides; typing reaches the command line); make the return key work (map egui-winit's dropped line feed to Enter); hide the Keyboard button while a hardware keyboard is attached | 2–4 + human |
| 3 | Touch-sized interface, the rest: a command palette in place of the menu bar; ribbon and palette layouts checked at finger spacing on a device; the Pencil kept at mouse sizes once winit reports it apart from fingers | 5–8 + human |
| 4 | System document picker (iCloud Drive, other providers) and "Open in CADCraft" from other apps; needs UIKit bindings without `unsafe` in our code | 6–12 |
| 5 | Apple Pencil: pressure-free precise picks, hover preview on Pencil hover (iPadOS 16+), palm rejection (touches ignored while the Pencil is down) | 4–8 |
| 6 | App Store packaging: asset catalog, privacy manifest, `.ipa`, a TestFlight release workflow | 4–6 |
