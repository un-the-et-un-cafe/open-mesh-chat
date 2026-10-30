// swift-tools-version:5.9
// Local wrapper around the webrtc-sdk WebRTC.xcframework (BSD-3-Clause), iOS only.
// The upstream manifest (github.com/webrtc-sdk/Specs) declares visionOS platforms that the
// CI Xcode's PackageDescription does not know, so we pin the same binary here.
// Same version as Android io.github.webrtc-sdk:android (docs/LIVE.md).
import PackageDescription

let package = Package(
    name: "WebRTCBinary",
    platforms: [.iOS(.v13)],
    products: [.library(name: "WebRTC", targets: ["WebRTC"])],
    targets: [
        .binaryTarget(
            name: "WebRTC",
            url: "https://github.com/webrtc-sdk/Specs/releases/download/150.7871.04/WebRTC.xcframework.zip",
            checksum: "9532fe3ce47507c304f37a4355b907be1e89967971d614a04603005a546249be"
        ),
    ]
)
