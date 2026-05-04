// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.

import Cocoa

// rtview — Cocoa GUI that displays the raytracer's streamed render.
//
// Stage three: a `RenderServer` listens on TCP for the raytracer's
// `StreamTarget` wire format, parses the header + row stream, and
// updates a single `PixelView` filling the application window. The
// window resizes itself to match the incoming image dimensions
// (capped to 80% of screen). Successive renders replace the displayed
// image without restarting the app.
//
// Run with `swift run` from the rtview/ directory.
//
// Listening port comes from `RTVIEW_ADDR` (matching the Rust side's env
// var). The host portion is ignored — the listener accepts on all local
// interfaces — but the port after the colon is parsed out so the same
// `RTVIEW_ADDR=127.0.0.1:9999` value can be set in both shells.
// Defaults to 9999 when unset.

class AppDelegate: NSObject, NSApplicationDelegate {
    var window: NSWindow!
    var pixelView: PixelView!
    var server: RenderServer!

    func applicationDidFinishLaunching(_ notification: Notification) {
        // Placeholder dimensions for the window+view before any
        // connection arrives. The first header replaces these via
        // `handleHeader(width:height:)`. Picking 512×512 keeps the
        // window visibly present (so the user knows the app is running)
        // without committing to anything that'll likely be the right
        // size for the first render.
        let placeholderW = 512
        let placeholderH = 512

        pixelView = PixelView(width: placeholderW, height: placeholderH)

        let contentRect = NSRect(
            x: 0, y: 0,
            width: placeholderW, height: placeholderH
        )
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

        let port = Self.parsePort()
        do {
            server = try RenderServer(port: port)
            server.onHeader = { [weak self] w, h in
                self?.handleHeader(width: w, height: h)
            }
            server.onRow = { [weak self] x, y, pixels in
                self?.pixelView.setRow(x: x, y: y, pixels: pixels)
            }
            server.onDone = {
                NSLog("rtview: render complete")
            }
            server.start()
        } catch {
            NSLog("rtview: failed to start server on port \(port): \(error)")
        }

        // Bring the app to the front and give it a Dock icon. Without
        // setActivationPolicy(.regular) earlier, a swift-run executable
        // lands as a background process and the window appears behind
        // other apps with no menu bar.
        NSApp.activate(ignoringOtherApps: true)
    }

    /// Resize the window and pixel buffers to match a newly-arrived
    /// render. Caps the displayed window to 80% of the screen's visible
    /// frame so a 2K-square render doesn't open a window bigger than
    /// the screen, and never scales up beyond 1:1 (rendering at less
    /// than native resolution to fill a bigger window would just show
    /// a blurry image and waste pixels).
    private func handleHeader(width: Int, height: Int) {
        let screenSize = NSScreen.main?.visibleFrame.size
            ?? NSSize(width: 1280, height: 720)
        let maxW = screenSize.width * 0.8
        let maxH = screenSize.height * 0.8
        let scale = min(
            maxW / CGFloat(width),
            maxH / CGFloat(height),
            1.0
        )
        let displayW = CGFloat(width) * scale
        let displayH = CGFloat(height) * scale

        window.setContentSize(NSSize(width: displayW, height: displayH))
        window.center()
        pixelView.resize(width: width, height: height)
    }

    /// Parse the port number from the optional `RTVIEW_ADDR` env var.
    /// Accepts any of `"9999"`, `"127.0.0.1:9999"`, `"[::1]:9999"`, etc.
    /// — anything after the last colon is interpreted as the port. Falls
    /// back to 9999 if unset or unparseable.
    private static func parsePort() -> UInt16 {
        guard let addr = ProcessInfo.processInfo.environment["RTVIEW_ADDR"]
        else { return 9999 }
        if let colon = addr.lastIndex(of: ":") {
            let portStr = addr[addr.index(after: colon)...]
            if let p = UInt16(portStr) { return p }
        }
        if let p = UInt16(addr) { return p }
        return 9999
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
