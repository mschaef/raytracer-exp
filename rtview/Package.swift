// swift-tools-version:5.7
//
// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.

import PackageDescription

// Standalone Swift Package — sibling to the Rust crate, intentionally not
// integrated into Cargo. Build and run with `swift run` from this
// directory. macOS 11 (Big Sur) is the minimum because we use the modern
// CoreGraphics CGImage construction APIs and the Network framework
// (which lands in stage three).
let package = Package(
    name: "rtview",
    platforms: [.macOS(.v11)],
    targets: [
        .executableTarget(
            name: "rtview",
            path: "Sources/rtview"
        )
    ]
)
