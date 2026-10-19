# JNA and the UniFFI bindings are reached through reflection/JNI.
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keep class uniffi.** { *; }
-keep, includedescriptorclasses class org.rustls.platformverifier.** { *; }
# JNA's AWT helpers reference desktop classes Android doesn't have.
-dontwarn java.awt.**
