// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.

import Foundation
import Network

/// TCP server that consumes the raytracer's `StreamTarget` wire format and
/// surfaces the parsed image as a sequence of UI-friendly callbacks.
///
/// Wire format (mirrors `render::output::StreamTarget` on the Rust side):
///
///   Header (16 bytes, sent once):
///     magic   "RTVW"
///     width   u32 LE
///     height  u32 LE
///     flags   u32 LE   (0 = linear-color f32 payload, only variant today)
///
///   Row message (sent per submit_row):
///     y       u32 LE
///     x       u32 LE
///     count   u32 LE
///     pixels  [f32 LE; count * 3]   linear R, G, B
///
/// Lifecycle: at most one active connection at a time. New connections that
/// arrive while one is in flight are immediately cancelled — the renderer
/// can simply retry, which makes "run rtview, then run the raytracer two
/// or three times in a row" Just Work without restarting the GUI between
/// renders. The listener stays up across connections, so a clean EOF on
/// the active connection just frees the slot for the next run.
///
/// Threading: everything (the listener, every connection, every receive
/// callback) runs on `DispatchQueue.main`. Network.framework is fully
/// async — `receive` schedules a closure for when bytes arrive rather
/// than blocking — so main-queue use doesn't stall AppKit. This keeps
/// the whole app single-threaded with no locking around the pixel
/// buffers, at the cost of capping throughput to whatever main can chew
/// through. For the rates this app sees (loopback, typically tens to
/// low hundreds of rows per second), main-queue handling is comfortably
/// inside the budget.
final class RenderServer {
    private let listenPort: NWEndpoint.Port
    private let listener: NWListener
    private var activeConnection: NWConnection?

    /// Fired on the main queue when a new render's header is parsed.
    /// Argument: (width, height) in pixels.
    var onHeader: ((Int, Int) -> Void)?

    /// Fired on the main queue for every row received. `pixels` is a
    /// flat array of `count * 3` linear-color floats laid out
    /// `[r0, g0, b0, r1, g1, b1, ...]`. The receiver is responsible for
    /// the linear → display-space encode.
    var onRow: ((_ x: Int, _ y: Int, _ pixels: [Float]) -> Void)?

    /// Fired on the main queue when the active connection ends (clean
    /// EOF, error, or cancellation). Indicates the render is finished
    /// and the listener is free for the next connection.
    var onDone: (() -> Void)?

    init(port: UInt16) throws {
        guard let p = NWEndpoint.Port(rawValue: port) else {
            throw NSError(
                domain: "rtview", code: 1,
                userInfo: [NSLocalizedDescriptionKey: "invalid port \(port)"]
            )
        }
        self.listenPort = p

        // allowLocalEndpointReuse is the SO_REUSEADDR equivalent — without
        // it, relaunching rtview while a previous bind is in TIME_WAIT
        // returns EADDRINUSE for ~60 seconds. Annoying for an interactive
        // dev tool that gets restarted often.
        let params: NWParameters = .tcp
        params.allowLocalEndpointReuse = true
        self.listener = try NWListener(using: params, on: p)
    }

    func start() {
        listener.newConnectionHandler = { [weak self] conn in
            self?.handleNewConnection(conn)
        }
        listener.stateUpdateHandler = { [weak self] state in
            switch state {
            case .ready:
                NSLog("rtview: listening on port \(self?.listenPort.rawValue ?? 0)")
            case .failed(let err):
                // Most commonly: port already in use. Surface the reason
                // to stderr; stage three doesn't have an alert UI yet.
                NSLog("rtview: listener failed: \(err)")
            default:
                break
            }
        }
        listener.start(queue: .main)
    }

    private func handleNewConnection(_ conn: NWConnection) {
        // One render at a time. Reject extra connections rather than
        // queue them — the renderer's response (an immediate error) is
        // a clearer signal than "everything looks fine but nothing
        // happens for a while".
        if activeConnection != nil {
            NSLog("rtview: rejecting concurrent connection (one render in flight)")
            conn.cancel()
            return
        }
        activeConnection = conn

        conn.stateUpdateHandler = { [weak self] state in
            guard let self = self else { return }
            switch state {
            case .ready:
                self.readHeader(conn)
            case .failed(let err):
                NSLog("rtview: connection failed: \(err)")
                self.finishConnection(conn)
            case .cancelled:
                self.finishConnection(conn)
            default:
                break
            }
        }
        conn.start(queue: .main)
    }

    /// Called when a connection ends, regardless of cause (clean EOF,
    /// error, or explicit cancel). Idempotent — guarded by identity so
    /// stray late state callbacks for an old connection can't clobber
    /// a newly-active one.
    private func finishConnection(_ conn: NWConnection) {
        guard activeConnection === conn else { return }
        activeConnection = nil
        onDone?()
    }

    // MARK: - Wire-format reader

    private func readHeader(_ conn: NWConnection) {
        conn.receive(minimumIncompleteLength: 16, maximumLength: 16) {
            [weak self] data, _, _, error in
            guard let self = self else { return }

            if let error = error {
                NSLog("rtview: header receive error: \(error)")
                conn.cancel()
                return
            }
            guard let data = data, data.count == 16 else {
                NSLog("rtview: short header read")
                conn.cancel()
                return
            }
            guard data.starts(with: "RTVW".utf8) else {
                NSLog("rtview: bad magic, expected RTVW")
                conn.cancel()
                return
            }

            let width = self.readUInt32LE(data, offset: 4)
            let height = self.readUInt32LE(data, offset: 8)
            let flags = self.readUInt32LE(data, offset: 12)
            guard flags == 0 else {
                NSLog("rtview: unknown flags value \(flags)")
                conn.cancel()
                return
            }

            self.onHeader?(Int(width), Int(height))
            self.readNextRow(conn)
        }
    }

    private func readNextRow(_ conn: NWConnection) {
        conn.receive(minimumIncompleteLength: 12, maximumLength: 12) {
            [weak self] data, _, isComplete, error in
            guard let self = self else { return }

            if let error = error {
                NSLog("rtview: row-header receive error: \(error)")
                conn.cancel()
                return
            }
            if let data = data, data.count == 12 {
                let y = self.readUInt32LE(data, offset: 0)
                let x = self.readUInt32LE(data, offset: 4)
                let count = self.readUInt32LE(data, offset: 8)
                let payloadLen = Int(count) * 12
                self.readPayload(
                    conn,
                    x: Int(x), y: Int(y), count: Int(count), len: payloadLen
                )
            } else if isComplete {
                // Clean EOF between rows — render finished. Cancel so
                // the state handler runs `finishConnection`.
                conn.cancel()
            } else {
                NSLog("rtview: short row-header read")
                conn.cancel()
            }
        }
    }

    private func readPayload(
        _ conn: NWConnection, x: Int, y: Int, count: Int, len: Int
    ) {
        conn.receive(minimumIncompleteLength: len, maximumLength: len) {
            [weak self] data, _, _, error in
            guard let self = self else { return }

            if let error = error {
                NSLog("rtview: payload receive error: \(error)")
                conn.cancel()
                return
            }
            guard let data = data, data.count == len else {
                NSLog("rtview: short payload read")
                conn.cancel()
                return
            }

            let pixels = self.decodePixels(data, count: count)
            self.onRow?(x, y, pixels)
            self.readNextRow(conn)
        }
    }

    // MARK: - Endian-aware decoders

    /// Read a little-endian `UInt32` at the given byte offset. Wrapping
    /// the raw load in `UInt32(littleEndian:)` documents intent and
    /// keeps the code correct on big-endian hosts (no Apple platform is
    /// big-endian today, but the cost is a no-op on LE so there's no
    /// reason to bake in the assumption).
    private func readUInt32LE(_ data: Data, offset: Int) -> UInt32 {
        return data.withUnsafeBytes { (raw: UnsafeRawBufferPointer) -> UInt32 in
            let bits = raw.loadUnaligned(fromByteOffset: offset, as: UInt32.self)
            return UInt32(littleEndian: bits)
        }
    }

    /// Decode `count` RGB pixels from a contiguous f32-LE payload into a
    /// flat `[Float]` of length `count * 3`. Same endian-portability
    /// note as `readUInt32LE`: load as `UInt32`, normalize endianness,
    /// then reinterpret bits as `Float`. The CPU's f32 in-register
    /// representation is the bit pattern of an LE-stored f32 on every
    /// supported host, so no further conversion is needed.
    private func decodePixels(_ data: Data, count: Int) -> [Float] {
        var result = [Float](repeating: 0, count: count * 3)
        data.withUnsafeBytes { (raw: UnsafeRawBufferPointer) in
            for i in 0..<(count * 3) {
                let bits = raw.loadUnaligned(fromByteOffset: i * 4, as: UInt32.self)
                result[i] = Float(bitPattern: UInt32(littleEndian: bits))
            }
        }
        return result
    }
}
