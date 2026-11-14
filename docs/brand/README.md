# BandEar brand

The two SVGs here are the source of truth for the BandEar logo. Every other copy is made from them,
so a logo change starts here and then updates each copy below.

| Source | What it is |
| --- | --- |
| `bandear-mark.svg` | The mark: six round-capped strokes, `#FFBF43`, stroke 44, on a 1024 canvas. |
| `bandear-icon.svg` | The app icon: the mark (stroke 46, scaled 1.16) on a `#192223` rounded square. |
| `bandear-preview.png` | A preview of the icon at several sizes; not used by any build. |

## Copies

| Copy | Made from | How to update it |
| --- | --- | --- |
| `apps/desktop/kit/assets/icons/bandear-mark.svg` (the desktop sidebar mark) | `bandear-mark.svg` | Copy the file over, keeping its comment that it is a copy. |
| `apps/desktop/app/icons/*` (`icon.ico`, `icon.icns`, `icon.png`, `32x32.png`, `128x128.png`, `128x128@2x.png`) | `bandear-icon.svg` | Render the icon to these six files with an icon tool (the Tauri CLI's `cargo tauri icon docs/brand/bandear-icon.svg` still does it from a scratch folder; keep only these six). `package.sh` puts `icon.icns` in the macOS app. |
| `apps/android/app/src/main/res/drawable/bandear_mark.xml` (the Connect screen's header) | `bandear-mark.svg` | Copy the six paths' `d` values; the drawable crops the 1024 grid to the strokes (`translateX`/`translateY` and its viewport), so redo the crop if the drawing's bounds change. |
| `apps/android/app/src/main/res/drawable/ic_launcher_foreground.xml` (launcher icon foreground) | `bandear-icon.svg` | Copy the six paths' `d` values and the stroke width; check that the scaled mark still fits the 66 dp safe circle (see the file's header). |
| `apps/android/app/src/main/res/drawable/ic_launcher_monochrome.xml` (themed launcher icon) | `bandear-icon.svg` | The same paths and placement as the foreground, in one colour. |
| `apps/android/app/src/main/res/values/colors.xml` (`bandear_icon_ground`, `bandear_mark`) | `bandear-icon.svg` | The ground and the mark's colour. |

The root `README.md` shows `bandear-mark.svg` directly, so it needs no copy.

After a change, check the desktop with `cargo test -p bandear-desktop-kit` and a build of
`bandear-desktop`, and the phone with gradle `:app:assembleDebug`.
