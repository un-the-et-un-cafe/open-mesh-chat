import SwiftUI

/// Design tokens v0.1.22 (DESIGN_SPEC §2.1, §5.102 dark, §5.104 light). WCAG AA.
struct OmcpColors {
    let bg: Color
    let surface: Color
    let surface2: Color
    let ink: Color
    let ink2: Color
    let ink3: Color
    let line: Color
    let lineStrong: Color
    /// Text/icons on solid brand/ok/danger fills (§5.102: never hard-code white).
    let brandInk: Color
    /// Focus ring ≥ 3 pt (§5S 2.4.11).
    let focus: Color
    let brand: Color
    let brandSoft: Color
    let ok: Color
    let okSoft: Color
    let warn: Color
    let warnSoft: Color
    let danger: Color
    let dangerSoft: Color
    let pathMesh: Color
    let pathMeshSoft: Color
    let pathGateway: Color
    let pathGatewaySoft: Color
    let pathMixed: Color
    let pathMixedSoft: Color
    let pathQueue: Color
    let pathQueueSoft: Color

    static let light = OmcpColors(
        bg: Color(hex: 0xF3F5F8),
        surface: Color(hex: 0xFFFFFF),
        surface2: Color(hex: 0xEEF1F5),
        ink: Color(hex: 0x0F1B2D),
        ink2: Color(hex: 0x3B495A),
        ink3: Color(hex: 0x515E6D),
        line: Color(hex: 0x73849A),
        lineStrong: Color(hex: 0x617289),
        brandInk: Color(hex: 0xFFFFFF),
        focus: Color(hex: 0x0A66C2),
        brand: Color(hex: 0x0B6E6A),
        brandSoft: Color(hex: 0xE0F3F1),
        ok: Color(hex: 0x0F6B3A),
        okSoft: Color(hex: 0xE3F4EA),
        warn: Color(hex: 0x8A5A10),
        warnSoft: Color(hex: 0xFAF0D8),
        danger: Color(hex: 0x9B1C2E),
        dangerSoft: Color(hex: 0xFCE8EB),
        pathMesh: Color(hex: 0x0B6E6A),
        pathMeshSoft: Color(hex: 0xE0F3F1),
        pathGateway: Color(hex: 0x1A5F9E),
        pathGatewaySoft: Color(hex: 0xE4EEF8),
        pathMixed: Color(hex: 0x5A4A9E),
        pathMixedSoft: Color(hex: 0xEEEBF8),
        pathQueue: Color(hex: 0x8A5A10),
        pathQueueSoft: Color(hex: 0xFAF0D8)
    )

    static let dark = OmcpColors(
        bg: Color(hex: 0x0C1219),
        surface: Color(hex: 0x151D28),
        surface2: Color(hex: 0x1C2633),
        ink: Color(hex: 0xE8EEF5),
        ink2: Color(hex: 0xB4C0CE),
        ink3: Color(hex: 0x95A3B4),
        line: Color(hex: 0x687990),
        lineStrong: Color(hex: 0x7A8BA1),
        brandInk: Color(hex: 0x062624),
        focus: Color(hex: 0x8EC8FF),
        brand: Color(hex: 0x3DBDB6),
        brandSoft: Color(hex: 0x143836),
        ok: Color(hex: 0x5DCA8E),
        okSoft: Color(hex: 0x143525),
        warn: Color(hex: 0xE0B45A),
        warnSoft: Color(hex: 0x3A2E14),
        danger: Color(hex: 0xF07178),
        dangerSoft: Color(hex: 0x3A1820),
        pathMesh: Color(hex: 0x3DBDB6),
        pathMeshSoft: Color(hex: 0x143836),
        pathGateway: Color(hex: 0x6BA3D9),
        pathGatewaySoft: Color(hex: 0x1A2E42),
        pathMixed: Color(hex: 0xA89AD9),
        pathMixedSoft: Color(hex: 0x2A2540),
        pathQueue: Color(hex: 0xE0B45A),
        pathQueueSoft: Color(hex: 0x3A2E14)
    )

    static func current() -> OmcpColors {
        UITraitCollection.current.userInterfaceStyle == .dark ? .dark : .light
    }
}

private struct OmcpColorsKey: EnvironmentKey {
    static let defaultValue = OmcpColors.light
}

extension EnvironmentValues {
    var omcpColors: OmcpColors {
        get { self[OmcpColorsKey.self] }
        set { self[OmcpColorsKey.self] = newValue }
    }
}

extension Color {
    init(hex: UInt32, opacity: Double = 1) {
        self.init(
            .sRGB,
            red: Double((hex >> 16) & 0xFF) / 255,
            green: Double((hex >> 8) & 0xFF) / 255,
            blue: Double(hex & 0xFF) / 255,
            opacity: opacity
        )
    }
}

/// You › Appearance: follow the system, or force light / dark.
enum ThemeMode: String, CaseIterable, Identifiable {
    case system, light, dark
    var id: String { rawValue }
    var colorScheme: ColorScheme? {
        switch self {
        case .system: return nil
        case .light: return .light
        case .dark: return .dark
        }
    }
    var labelKey: String { "theme_" + rawValue }
}

extension View {
    /// WCAG 2.5.8 + platform guidance: keep the visual size, extend the hit area to ≥ 44 pt.
    func omcpHitTarget() -> some View {
        frame(minWidth: 44, minHeight: 44).contentShape(Rectangle())
    }

    /// §5S glass bars: opaque surface-coloured tab/nav bars so labels keep 4.5:1 over any content.
    func omcpOpaqueBars(_ c: OmcpColors) -> some View {
        toolbarBackground(c.surface, for: .tabBar)
            .toolbarBackground(.visible, for: .tabBar)
            .toolbarBackground(c.surface, for: .navigationBar)
            .toolbarBackground(.visible, for: .navigationBar)
    }
}
