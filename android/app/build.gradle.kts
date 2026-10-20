plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

// The Kotlin half of rustls-platform-verifier must match the Rust crate's version exactly.
val rustlsVerifierVersion: String = rootDir.resolve("../Cargo.lock").readLines().let { lines ->
    val i = lines.indexOfFirst { it.trim() == "name = \"rustls-platform-verifier-android\"" }
    require(i >= 0) { "rustls-platform-verifier-android not found in Cargo.lock" }
    lines[i + 1].substringAfter('"').substringBefore('"')
}

android {
    namespace = "co.huggingface.translatte"
    compileSdk = 35

    defaultConfig {
        applicationId = "co.huggingface.translatte"
        minSdk = 31
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"
        ndk { abiFilters += "arm64-v8a" }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            // Sideloaded for now; sign with the debug key until there is a release key.
            signingConfig = signingConfigs.getByName("debug")
        }
    }

    sourceSets["main"].apply {
        // Written by scripts/android/build-app.sh: UniFFI bindings, native libraries, manifest.json.
        java.srcDir("src/generated/kotlin")
        jniLibs.srcDir("src/generated/jniLibs")
        assets.srcDir("src/generated/assets")
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
    buildFeatures {
        compose = true
        buildConfig = true
    }
    testOptions {
        unitTests.isIncludeAndroidResources = true
    }
    packaging {
        jniLibs {
            useLegacyPackaging = false
            // Already stripped by build-app.sh.
            keepDebugSymbols += "**/*.so"
        }
    }
}

dependencies {
    val composeBom = platform("androidx.compose:compose-bom:2024.12.01")
    implementation(composeBom)
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material:material-icons-extended")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.activity:activity-compose:1.9.3")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.8.7")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.8.7")
    implementation("androidx.lifecycle:lifecycle-process:2.8.7")
    implementation("androidx.work:work-runtime-ktx:2.10.0")
    implementation("androidx.camera:camera-camera2:1.4.1")
    implementation("androidx.camera:camera-lifecycle:1.4.1")
    implementation("androidx.camera:camera-view:1.4.1")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.9.0")
    implementation("net.java.dev.jna:jna:5.15.0@aar")
    implementation("org.rustls:rustls-platform-verifier:$rustlsVerifierVersion")

    testImplementation("junit:junit:4.13.2")
    testImplementation(composeBom)
    testImplementation("androidx.compose.ui:ui-test-junit4")
    testImplementation("androidx.test.ext:junit:1.2.1")
    testImplementation("org.robolectric:robolectric:4.14.1")
    debugImplementation("androidx.compose.ui:ui-test-manifest")
}
