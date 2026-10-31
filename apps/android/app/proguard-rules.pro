# OMCP Android shell: R8 shrinking (v0.1.29), no obfuscation.
-dontobfuscate

# UniFFI Kotlin bindings call into libomcp_ffi.so through JNA, which uses
# reflection on Structure fields, Callback interfaces and Native methods.
-keep class chat.omcp.ffi.** { *; }
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keepclassmembers class * extends com.sun.jna.Structure { public *; }
-dontwarn java.awt.**
-dontwarn com.sun.jna.**

# libwebrtc: classes and members are looked up from native code (JNI).
-keep class org.webrtc.** { *; }
-keep class livekit.org.webrtc.** { *; }
-dontwarn org.webrtc.**

# Native methods anywhere.
-keepclasseswithmembernames,includedescriptorclasses class * { native <methods>; }
