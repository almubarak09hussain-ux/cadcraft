# CADCraft on iPad

> **Status:** first build, 2026-10-25. Builds and type-checks for iPadOS (`cargo xtask ios`).
> Nobody has run it on a device or the simulator yet: the next step needs a Mac with Xcode.

`apps/cadcraft-ios` runs the same app as the desktop and web builds (`cadcraft_ui_egui::CadApp`:
the drafting UI, command line, menus, ribbon and palettes), through eframe on winit's UIKit
backend with wgpu on Metal. The engine, file formats and AutoLISP are unchanged. With a Magic
Keyboard or another keyboard and trackpad attached, it works like the Mac app: command line,
shortcuts, hover, right-click.

## What is different on iPad

| Area | iPad behaviour | Where |
|---|---|---|
| Touch | One finger or the Apple Pencil is the pointer: a tap places a point or picks. Two fingers pan and pinch-zoom about their centre. A long press is a right-click: Enter while a command runs, otherwise the shortcut menu. Nothing the first finger does during a two-finger gesture places a point or opens a selection window. | `crates/ui-egui/src/touch.rs` (also used by the web build on touch screens) |
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
| 2 | On-screen keyboard: show it when the command line or a text field takes focus with no hardware keyboard (winit's `set_ime_allowed`), and add an accessory row with `@ < , .` Enter, Esc and recent commands | 4–8 |
| 3 | Touch-sized interface: larger hit targets, grips and pick box when the last input was a finger (a `Tokens` touch scale); a command palette in place of the menu bar | 6–10 |
| 4 | System document picker (iCloud Drive, other providers) and "Open in CADCraft" from other apps; needs UIKit bindings without `unsafe` in our code | 6–12 |
| 5 | Apple Pencil: pressure-free precise picks, hover preview on Pencil hover (iPadOS 16+), palm rejection (touches ignored while the Pencil is down) | 4–8 |
| 6 | App Store packaging: asset catalog, privacy manifest, `.ipa`, a TestFlight release workflow | 4–6 |
