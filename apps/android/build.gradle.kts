plugins {
    id("com.android.application") version "9.5.1" apply false
    // AGP 9+ ships built-in Kotlin; pin a newer KGP than AGP's runtime default
    // via the Compose compiler plugin (same version family as Kotlin).
    id("org.jetbrains.kotlin.plugin.compose") version "2.4.21" apply false
}
