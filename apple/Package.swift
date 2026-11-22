// swift-tools-version:5.9
// Paquet Swift de tallylight. `TallylightFFI.xcframework` et `Sources/TallylightKit/Tallylight.swift`
// ne sont pas dans le dépôt : `apple/build-xcframework.sh` les produit (voir docs/INFRA.md).
import PackageDescription

let package = Package(
    name: "Tallylight",
    // Mêmes versions minimales que IOS_MIN et MACOS_MIN dans build-xcframework.sh
    platforms: [.iOS(.v15), .macOS(.v12)],
    products: [
        .library(name: "TallylightKit", targets: ["TallylightKit"])
    ],
    targets: [
        // La bibliothèque Rust compilée (statique) et son en-tête C
        .binaryTarget(name: "TallylightFFI", path: "TallylightFFI.xcframework"),
        // Les liaisons Swift générées par UniFFI. Le module ne s'appelle pas `Tallylight` : il
        // porterait le nom de sa classe principale, et `Tallylight.Session` ne pourrait plus
        // désigner un type du module
        .target(
            name: "TallylightKit",
            dependencies: ["TallylightFFI"],
            path: "Sources/TallylightKit",
            linkerSettings: [
                // Ce que la bibliothèque Rust attend du système (réseau, certificats)
                .linkedFramework("Security"),
                .linkedFramework("SystemConfiguration"),
                .linkedFramework("CoreFoundation"),
                // Sur iOS, la bibliothèque réseau énumère les interfaces par Network.framework
                .linkedFramework("Network", .when(platforms: [.iOS])),
            ]
        ),
    ]
)
