package chat.omcp.app.ui.theme

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color

data class OmcpExtraColors(
    val ink2: Color,
    val ink3: Color,
    val surface2: Color,
    val line: Color,
    val brandSoft: Color,
    val ok: Color,
    val okSoft: Color,
    val warn: Color,
    val warnSoft: Color,
    val danger: Color,
    val dangerSoft: Color,
    val pathMesh: Color,
    val pathMeshSoft: Color,
    val pathGateway: Color,
    val pathGatewaySoft: Color,
    val pathMixed: Color,
    val pathMixedSoft: Color,
    val pathQueue: Color,
    val pathQueueSoft: Color,
    val lineStrong: Color = Color(0xFF617289),
    val brandInk: Color = Color.White,
    val focus: Color = Color(0xFF0A66C2),
)

/** Appearance (You › Appearance): follow the system, or force light / dark. */
enum class ThemeMode { System, Light, Dark;
    fun isDark(systemDark: Boolean) = when (this) { System -> systemDark; Light -> false; Dark -> true }
    companion object { fun parse(s: String?) = entries.firstOrNull { it.name == s } ?: System }
}

val LocalOmcpColors = staticCompositionLocalOf {
    OmcpExtraColors(
        ink2 = OmcpLight.Ink2,
        ink3 = OmcpLight.Ink3,
        surface2 = OmcpLight.Surface2,
        line = OmcpLight.Line,
        brandSoft = OmcpLight.BrandSoft,
        ok = OmcpLight.Ok,
        okSoft = OmcpLight.OkSoft,
        warn = OmcpLight.Warn,
        warnSoft = OmcpLight.WarnSoft,
        danger = OmcpLight.Danger,
        dangerSoft = OmcpLight.DangerSoft,
        pathMesh = OmcpLight.PathMesh,
        pathMeshSoft = OmcpLight.PathMeshSoft,
        pathGateway = OmcpLight.PathGateway,
        pathGatewaySoft = OmcpLight.PathGatewaySoft,
        pathMixed = OmcpLight.PathMixed,
        pathMixedSoft = OmcpLight.PathMixedSoft,
        pathQueue = OmcpLight.PathQueue,
        pathQueueSoft = OmcpLight.PathQueueSoft,
    )
}

private val LightScheme = lightColorScheme(
    primary = OmcpLight.Brand,
    onPrimary = OmcpLight.BrandInk,
    primaryContainer = OmcpLight.BrandSoft,
    onPrimaryContainer = OmcpLight.Brand,
    background = OmcpLight.Bg,
    onBackground = OmcpLight.Ink,
    surface = OmcpLight.Surface,
    onSurface = OmcpLight.Ink,
    surfaceVariant = OmcpLight.Surface2,
    onSurfaceVariant = OmcpLight.Ink2,
    outline = OmcpLight.Line,
    outlineVariant = OmcpLight.Line,
    inversePrimary = OmcpDark.Brand,
    error = OmcpLight.Danger,
    onError = OmcpLight.BrandInk,
)

private val DarkScheme = darkColorScheme(
    primary = OmcpDark.Brand,
    onPrimary = OmcpDark.BrandInk,
    primaryContainer = OmcpDark.BrandSoft,
    onPrimaryContainer = OmcpDark.Brand,
    background = OmcpDark.Bg,
    onBackground = OmcpDark.Ink,
    surface = OmcpDark.Surface,
    onSurface = OmcpDark.Ink,
    surfaceVariant = OmcpDark.Surface2,
    onSurfaceVariant = OmcpDark.Ink2,
    outline = OmcpDark.Line,
    outlineVariant = OmcpDark.Line,
    inversePrimary = OmcpLight.Brand,
    error = OmcpDark.Danger,
    onError = OmcpDark.BrandInk,
)

private val LightExtra = OmcpExtraColors(
    ink2 = OmcpLight.Ink2,
    ink3 = OmcpLight.Ink3,
    surface2 = OmcpLight.Surface2,
    line = OmcpLight.Line,
    brandSoft = OmcpLight.BrandSoft,
    ok = OmcpLight.Ok,
    okSoft = OmcpLight.OkSoft,
    warn = OmcpLight.Warn,
    warnSoft = OmcpLight.WarnSoft,
    danger = OmcpLight.Danger,
    dangerSoft = OmcpLight.DangerSoft,
    pathMesh = OmcpLight.PathMesh,
    pathMeshSoft = OmcpLight.PathMeshSoft,
    pathGateway = OmcpLight.PathGateway,
    pathGatewaySoft = OmcpLight.PathGatewaySoft,
    pathMixed = OmcpLight.PathMixed,
    pathMixedSoft = OmcpLight.PathMixedSoft,
    pathQueue = OmcpLight.PathQueue,
    pathQueueSoft = OmcpLight.PathQueueSoft,
    lineStrong = OmcpLight.LineStrong,
    brandInk = OmcpLight.BrandInk,
    focus = OmcpLight.Focus,
)

private val DarkExtra = OmcpExtraColors(
    ink2 = OmcpDark.Ink2,
    ink3 = OmcpDark.Ink3,
    surface2 = OmcpDark.Surface2,
    line = OmcpDark.Line,
    brandSoft = OmcpDark.BrandSoft,
    ok = OmcpDark.Ok,
    okSoft = OmcpDark.OkSoft,
    warn = OmcpDark.Warn,
    warnSoft = OmcpDark.WarnSoft,
    danger = OmcpDark.Danger,
    dangerSoft = OmcpDark.DangerSoft,
    pathMesh = OmcpDark.PathMesh,
    pathMeshSoft = OmcpDark.PathMeshSoft,
    pathGateway = OmcpDark.PathGateway,
    pathGatewaySoft = OmcpDark.PathGatewaySoft,
    pathMixed = OmcpDark.PathMixed,
    pathMixedSoft = OmcpDark.PathMixedSoft,
    pathQueue = OmcpDark.PathQueue,
    pathQueueSoft = OmcpDark.PathQueueSoft,
    lineStrong = OmcpDark.LineStrong,
    brandInk = OmcpDark.BrandInk,
    focus = OmcpDark.Focus,
)

@Composable
fun OmcpTheme(
    mode: ThemeMode = ThemeMode.System,
    darkTheme: Boolean = mode.isDark(isSystemInDarkTheme()),
    content: @Composable () -> Unit,
) {
    val scheme = if (darkTheme) DarkScheme else LightScheme
    val extra = if (darkTheme) DarkExtra else LightExtra
    CompositionLocalProvider(LocalOmcpColors provides extra) {
        MaterialTheme(colorScheme = scheme, content = content)
    }
}
