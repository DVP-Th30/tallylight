// swift-tools-version:5.9
// Essai des liaisons Swift sur macOS : un programme en ligne de commande qui reçoit les sessions
// d'un démon d'essai. Lancé par `apple/smoke-test.sh`, après `apple/build-xcframework.sh`.
import PackageDescription

let package = Package(
    name: "tallylight-smoke",
    platforms: [.macOS(.v12)],
    dependencies: [
        .package(name: "Tallylight", path: "..")
    ],
    targets: [
        .executableTarget(
            name: "tallylight-smoke",
            dependencies: [.product(name: "TallylightKit", package: "Tallylight")]
        )
    ]
)
