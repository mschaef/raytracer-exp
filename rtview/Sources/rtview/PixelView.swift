// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.

import Cocoa

/// Custom view that owns the rtview pixel buffers and draws them.
///
/// Two buffers are kept in lockstep:
///
/// - `linearBuffer` is a `Float32` RGB triple per pixel — the same shape
///   as the wire format the raytracer's `StreamTarget` sends. Holding
///   this around lets stage three's stream parser write directly into
///   the source-of-truth buffer; nothing in the view's drawing path
///   touches it once the display buffer is up to date.
/// - `displayBuffer` is `UInt8` RGBA per pixel, sRGB-encoded, ready to
///   blit. Built up incrementally as pixels arrive, so `draw(_:)` only
///   has to construct a CGImage view over it. RGBA rather than RGB
///   because a 32-bit pixel format is what every common CGImage
///   bitmapInfo configuration expects, and the alpha byte is essentially
///   free (4MB at 1024² vs 3MB for packed RGB).
///
/// We deliberately do *not* override `isFlipped`. AppKit's default
/// coordinate system has y increasing upward, and `CGContext.draw(_:in:)`
/// places a CGImage with its origin at the rect's CG-bottom-left — which
/// in a non-flipped view means the image's row 0 lands at the visual
/// top, exactly matching the renderer's "row 0 is the top of the image"
/// convention. Buffer indexing stores row 0 first, so the chain
/// `setPixel(y: 0) → displayBuffer offset 0 → CGImage row 0 → visual
/// top` is consistent end-to-end. Flipping the view would require a
/// counter-flip transform around the `ctx.draw` to undo the resulting
/// double inversion, with no benefit.
class PixelView: NSView {
    private var imageWidth: Int
    private var imageHeight: Int

    private var linearBuffer: [Float]   // size = w*h*3
    private var displayBuffer: [UInt8]  // size = w*h*4 (RGBA)

    init(width: Int, height: Int) {
        self.imageWidth = width
        self.imageHeight = height
        self.linearBuffer = [Float](repeating: 0.0, count: width * height * 3)
        self.displayBuffer = Self.makeDisplayBuffer(width: width, height: height)
        super.init(frame: NSRect(x: 0, y: 0, width: width, height: height))
        // Resize with the window: the content view occupies the full
        // window and we letterbox internally to preserve aspect ratio.
        autoresizingMask = [.width, .height]
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) not implemented — PixelView is constructed programmatically")
    }

    override var isOpaque: Bool { true }

    /// Reallocate the pixel buffers for a new image size. Called by the
    /// app delegate when the stream parser parses a new render's header
    /// — typically followed by a window resize so the new image displays
    /// at native resolution. Resets all pixels to black; subsequent
    /// `setRow` calls fill the new buffer in.
    func resize(width: Int, height: Int) {
        self.imageWidth = width
        self.imageHeight = height
        self.linearBuffer = [Float](repeating: 0.0, count: width * height * 3)
        self.displayBuffer = Self.makeDisplayBuffer(width: width, height: height)
        needsDisplay = true
    }

    /// Helper: build a freshly-zeroed RGBA buffer with the alpha byte
    /// pre-filled to opaque so per-pixel writes only need to touch RGB.
    /// Static so it can be called from both `init` and `resize`
    /// without going through `self`.
    private static func makeDisplayBuffer(width: Int, height: Int) -> [UInt8] {
        var disp = [UInt8](repeating: 0, count: width * height * 4)
        for i in 0..<(width * height) {
            disp[i * 4 + 3] = 255
        }
        return disp
    }

    /// Write a contiguous row of `count = pixels.count / 3` pixels
    /// starting at `(x, y)`. `pixels` is a flat triple-per-pixel array
    /// `[r0, g0, b0, r1, g1, b1, ...]` in linear color, matching what
    /// `RenderServer.decodePixels` produces. Out-of-bounds pixels are
    /// silently dropped — the network input is untrusted, and a bad
    /// row shouldn't crash the GUI.
    ///
    /// Marks the view dirty after the write; AppKit coalesces multiple
    /// `needsDisplay = true` calls into a single redraw per frame, so
    /// even a flood of arriving rows turns into ~display-rate paints.
    func setRow(x: Int, y: Int, pixels: [Float]) {
        guard y >= 0, y < imageHeight else { return }
        let count = pixels.count / 3
        for i in 0..<count {
            let px = x + i
            guard px >= 0, px < imageWidth else { continue }
            setPixel(
                x: px, y: y,
                r: pixels[i * 3],
                g: pixels[i * 3 + 1],
                b: pixels[i * 3 + 2]
            )
        }
        needsDisplay = true
    }

    /// Set one pixel from a linear-color triple. Internal helper used by
    /// `setRow`; not exposed to the network parser, which always works
    /// in row-sized batches.
    private func setPixel(x: Int, y: Int, r: Float, g: Float, b: Float) {
        let i = y * imageWidth + x
        let li = i * 3
        linearBuffer[li] = r
        linearBuffer[li + 1] = g
        linearBuffer[li + 2] = b

        let di = i * 4
        displayBuffer[di] = encodeChannel(r)
        displayBuffer[di + 1] = encodeChannel(g)
        displayBuffer[di + 2] = encodeChannel(b)
        // displayBuffer[di + 3] stays at 255 (set in init).
    }

    /// Linear → 8-bit sRGB. Mirrors `to_png_color` / `linear_to_srgb` in
    /// the renderer: same transfer function, same `(s * 256.0)` →
    /// truncate quantization, same clamps. Keeping the math identical
    /// means stage three's streamed output renders byte-for-byte
    /// identical (modulo the f64→f32 narrowing already done at the
    /// wire boundary) to what a `PngTarget` would have written.
    private func encodeChannel(_ x: Float) -> UInt8 {
        let s: Float
        if x < 0.0 {
            s = 0.0
        } else if x < 0.0031308 {
            s = x * 12.92
        } else if x < 1.0 {
            s = 1.055 * powf(x, 1.0 / 2.4) - 0.055
        } else {
            s = 1.0
        }
        // s ∈ [0, 1]; (s * 256) ∈ [0, 256]; truncate then clamp the
        // single boundary case (s == 1.0) so we land in [0, 255].
        let q = Int(s * 256.0)
        return UInt8(min(255, max(0, q)))
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let ctx = NSGraphicsContext.current?.cgContext else { return }

        // Letterbox fill — anything not covered by the image rect comes
        // out solid black, which makes resizing feel clean and gives
        // the eye a stable reference for the image edge.
        ctx.setFillColor(CGColor(red: 0, green: 0, blue: 0, alpha: 1))
        ctx.fill(bounds)

        // Build a CGImage view over the display buffer. The Data wraps
        // the bytes without copying; CGDataProvider holds the Data
        // alive for the lifetime of the CGImage; the CGImage is
        // released at function exit. Per-draw construction is fine at
        // 2K-square sizes — the cost is dominated by the actual
        // rasterization, not the CGImage object itself.
        let bytes = displayBuffer.withUnsafeBufferPointer { buf in
            Data(buffer: buf)
        }
        guard let provider = CGDataProvider(data: bytes as CFData) else { return }
        guard let colorSpace = CGColorSpace(name: CGColorSpace.sRGB) else { return }
        let bitmapInfo = CGBitmapInfo(rawValue: CGImageAlphaInfo.noneSkipLast.rawValue)
        guard let image = CGImage(
            width: imageWidth,
            height: imageHeight,
            bitsPerComponent: 8,
            bitsPerPixel: 32,
            bytesPerRow: imageWidth * 4,
            space: colorSpace,
            bitmapInfo: bitmapInfo,
            provider: provider,
            decode: nil,
            shouldInterpolate: false,
            intent: .defaultIntent
        ) else { return }

        // Aspect-fit: scale the image so its longer axis matches the
        // view's, center on the shorter axis, letterbox the rest.
        let imageAspect = CGFloat(imageWidth) / CGFloat(imageHeight)
        let viewAspect = bounds.width / bounds.height
        let drawRect: CGRect
        if imageAspect > viewAspect {
            // Image relatively wider than the view — fit to width.
            let h = bounds.width / imageAspect
            drawRect = CGRect(
                x: 0,
                y: (bounds.height - h) / 2,
                width: bounds.width,
                height: h
            )
        } else {
            // Image relatively taller (or equal) — fit to height.
            let w = bounds.height * imageAspect
            drawRect = CGRect(
                x: (bounds.width - w) / 2,
                y: 0,
                width: w,
                height: bounds.height
            )
        }
        ctx.draw(image, in: drawRect)
    }
}
