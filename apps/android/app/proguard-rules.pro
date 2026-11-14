# R8 rules for the BandEar app's release build.
#
# The app itself needs no keep rules: it uses no reflection, its JSON is read and written by
# explicit key names (org.json is part of the platform), and the ViewModel is built by an explicit
# factory. The libraries (Compose, AndroidX, OkHttp, Okio, coroutines) bring their own rules.

# Keep file names and line numbers, so a crash trace from a release build points at real lines.
-keepattributes SourceFile,LineNumberTable
-renamesourcefileattribute SourceFile

# The shared Rust core (libbandear_ffi) is reached through JNA from the bindings UniFFI generates
# into com.bandear.android.core. JNA binds native functions, callbacks and structure fields by
# their Java names and reads @Structure.FieldOrder by reflection, so neither JNA nor the bindings
# may be renamed or removed.
-keep class com.sun.jna.** { *; }
-keep class * extends com.sun.jna.** { *; }
-keep class com.bandear.android.core.** { *; }
-keepattributes RuntimeVisibleAnnotations
# JNA's desktop-only classes reference java.awt, which Android does not have.
-dontwarn java.awt.**
