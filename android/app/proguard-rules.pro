# JNA and the UniFFI bindings are reached through reflection/JNI.
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keep class uniffi.** { *; }
-keep, includedescriptorclasses class org.rustls.platformverifier.** { *; }
