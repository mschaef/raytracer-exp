// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.

import Cocoa

// rtview — minimal Cocoa app that displays a pixel buffer in a single
// resizable window. Stage two of the rtview integration: no networking
// yet; on launch the view is filled with a hardcoded test pattern so we
// can validate orientation, channel order, and the linear → sRGB
// encoding path before plugging in the stream parser.
//
// Run with `swift run` from the rtview/ directory.

/// Diagnostic test pattern — four horizontal bands top to bottom, each a
/// linear gradient from 0.0 on the left to 1.0 on the right:
///
///   1. Red gradient   (top)
///   2. Green gradient
///   3. Blue gradient
///   4. Gray gradient  (bottom)
///
/// What this verifies, all in one glance:
///
/// - Y-axis orientation: red band on top, gray on the bottom. If they're
///   reversed, `isFlipped` is wrong or the CGImage draw is upside down.
/// - X-axis orientation: each band darkens to the left, brightens to
///   the right. If reversed, the row indexing is wrong.
/// - Channel order: each colored band is its named color. If green
///   shows up in the red band, R/B are swapped (likely BGRA vs RGBA).
/// - Linear → sRGB encoding: the gradient should look perceptually
///   roughly uniform (the midpoint visually around the middle of the
///   bar). If it looks crushed dark with most of the gradient bunched
///   into the right third, the encoding step isn't running.
func generateTestPattern(into view: PixelView, width: Int, height: Int) {
    let bandHeight = height / 4
    for y in 0..<height {
        let band = min(y / bandHeight, 3)
        for x in 0..<width {
            // Linear ramp 0..1 across the row. (W-1) so the right edge
            // hits exactly 1.0 rather than just below it.
            let u = Float(x) / Float(width - 1)
            let r: Float, g: Float, b: Float
            switch band {
            case 0: (r, g, b) = (u,  0,  0)
            case 1: (r, g, b) = (0,  u,  0)
            case 2: (r, g, b) = (0,  0,  u)
            default: (r, g, b) = (u,  u,  u)
            }
            view.setPixel(x: x, y: y, r: r, g: g, b: b)
        }
    }
}

class AppDelegate: NSObject, NSApplicationDelegate {
    var window: NSWindow!
    var pixelView: PixelView!

    func applicationDidFinishLaunching(_ notification: Notification) {
        // Stage-two image size is hardcoded. Stage three reads it from
        // the StreamTarget header and resizes the view + window then.
        let imageWidth = 512
        let imageHeight = 512

        pixelView = PixelView(width: imageWidth, height: imageHeight)
        generateTestPattern(into: pixelView, width: imageWidth, height: imageHeight)

        // Initial window size matches the image at 1:1. The view
        // letterboxes if the user resizes the window to a different
        // aspect ratio, so resizing remains visually clean.
        let contentRect = NSRect(x: 200, y: 200,
                                 width: imageWidth, height: imageHeight)
        window = NSWindow(
            contentRect: contentRect,
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.title = "rtview"
        window.contentView = pixelView
        window.makeKeyAndOrderFront(nil)
        window.center()

        // Bring the app to the front and give it a Dock icon. Without
        // setActivationPolicy(.regular), a swift-run executable lands
        // as a background process and the window appears behind other
        // apps with no menu bar.
        NSApp.activate(ignoringOtherApps: true)
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        true
    }
}

// Standard programmatic NSApplication bootstrap. SwiftPM executable
// targets don't get an Info.plist or storyboard, so we wire everything
// up by hand here.
let app = NSApplication.shared
app.setActivationPolicy(.regular)

let delegate = AppDelegate()
app.delegate = delegate

app.run()
