plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "com.archipelago.app"
    compileSdk = 35

    defaultConfig {
        applicationId = "com.archipelago.app"
        minSdk = 26
        targetSdk = 35
        versionCode = 56
        versionName = "0.5.36"

        vectorDrawables {
            useSupportLibrary = true
        }

        // The embedded FIPS mesh (libarchy_fips_core.so) is built arm64-only,
        // matching real handsets. FipsNative.available gates every call, so
        // the app still runs as a plain companion elsewhere (e.g. x86 emu).
        ndk { abiFilters += "arm64-v8a" }
    }

    signingConfigs {
        // Repo-dedicated debug keystore (committed at app/debug.keystore) so every
        // machine — and the published companion download — signs debug builds with
        // the SAME key. Without this, Gradle falls back to each machine's
        // ~/.android/debug.keystore, so a build from a different machine has a
        // different signature and the phone rejects the update ("App not installed").
        getByName("debug") {
            storeFile = file("debug.keystore")
            storePassword = "android"
            keyAlias = "androiddebugkey"
            keyPassword = "android"
            // Force both legacy JAR (v1) and APK Signature Scheme v2. AGP drops v1
            // for minSdk>=24, but some OEM package installers (e.g. Samsung) reject
            // a v2-only sideload with "App not installed" — keep v1 for max compat.
            enableV1Signing = true
            enableV2Signing = true
        }
        // Local-only UAT builds install beside both the production companion
        // and its shared-key debug package. The ignored uat.keystore is made
        // on the validation box; it must never be used for a public artifact.
        create("uat") {
            storeFile = file("uat.keystore")
            storePassword = "android"
            keyAlias = "androiduatkey"
            keyPassword = "android"
            enableV1Signing = true
            enableV2Signing = true
        }
    }

    buildTypes {
        debug {
            // Separate app ID so a debug/test build installs alongside the
            // release app instead of colliding on signature.
            applicationIdSuffix = ".debug"
            versionNameSuffix = "-debug"
            signingConfig = signingConfigs.getByName("debug")
        }
        create("uat") {
            initWith(getByName("debug"))
            applicationIdSuffix = ".uat"
            versionNameSuffix = "-uat"
            signingConfig = signingConfigs.getByName("uat")
            matchingFallbacks += listOf("debug")
        }
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }

    buildFeatures {
        compose = true
    }

    composeOptions {
        kotlinCompilerExtensionVersion = "1.5.14"
    }

    packaging {
        resources {
            excludes += "/META-INF/{AL2.0,LGPL2.1}"
        }
    }
}

// ---------------------------------------------------------------------------
// Embedded FIPS mesh: cross-compile Android/rust/archy-fips-core via cargo-ndk
// into jniLibs before the native-libs merge, so a plain `gradlew assembleDebug`
// builds the Rust too. Requires rustup target aarch64-linux-android, cargo-ndk,
// and an NDK (ANDROID_NDK_HOME or the SDK's ndk/ dir).
// ---------------------------------------------------------------------------
val rustCrateDir = layout.projectDirectory.dir("../rust/archy-fips-core")
val jniLibsDir = layout.projectDirectory.dir("src/main/jniLibs")

tasks.register<Exec>("buildRustArm64") {
    workingDir = rustCrateDir.asFile
    inputs.dir(rustCrateDir.dir("src"))
    inputs.file(rustCrateDir.file("Cargo.toml"))
    outputs.dir(jniLibsDir)
    // cargo/cargo-ndk live in ~/.cargo/bin, which Gradle's env may not have.
    val home = System.getProperty("user.home")
    environment("PATH", "$home/.cargo/bin:${System.getenv("PATH")}")
    if (System.getenv("ANDROID_NDK_HOME") == null) {
        val sdkNdk = file("$home/Library/Android/sdk/ndk")
            .listFiles()?.maxByOrNull { it.name }
        if (sdkNdk != null) environment("ANDROID_NDK_HOME", sdkNdk.absolutePath)
    }
    commandLine(
        "cargo", "ndk",
        "-t", "arm64-v8a",
        "--platform", "26",
        "-o", jniLibsDir.asFile.absolutePath,
        "build", "--release",
    )
}

tasks.matching {
    it.name in listOf(
        "mergeDebugNativeLibs", "mergeUatNativeLibs", "mergeReleaseNativeLibs",
        "mergeDebugJniLibFolders", "mergeUatJniLibFolders", "mergeReleaseJniLibFolders",
    )
}.configureEach { dependsOn("buildRustArm64") }

dependencies {
    testImplementation("junit:junit:4.13.2")
    testImplementation("com.squareup.okhttp3:mockwebserver:4.12.0")
    testImplementation("org.robolectric:robolectric:4.14.1")
    val composeBom = platform("androidx.compose:compose-bom:2024.05.00")
    implementation(composeBom)

    implementation("androidx.core:core-ktx:1.13.1")
    implementation("androidx.lifecycle:lifecycle-runtime-ktx:2.8.2")
    implementation("androidx.activity:activity-compose:1.9.0")

    // Compose
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-graphics")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material:material-icons-extended")
    implementation("androidx.compose.animation:animation")

    // Navigation
    implementation("androidx.navigation:navigation-compose:2.7.7")

    // DataStore for preferences
    implementation("androidx.datastore:datastore-preferences:1.1.1")

    // WebView
    implementation("androidx.webkit:webkit:1.11.0")

    // Splash screen
    implementation("androidx.core:core-splashscreen:1.0.1")

    // OkHttp for WebSocket (remote input)
    implementation("com.squareup.okhttp3:okhttp:4.12.0")

    // CameraX + ZXing (Apache-2.0, on-device, no telemetry) for pairing-QR scanning
    implementation("androidx.camera:camera-camera2:1.3.4")
    implementation("androidx.camera:camera-lifecycle:1.3.4")
    implementation("androidx.camera:camera-view:1.3.4")
    implementation("com.google.zxing:core:3.5.3")

    debugImplementation("androidx.compose.ui:ui-tooling")
    debugImplementation("androidx.compose.ui:ui-test-manifest")
}
