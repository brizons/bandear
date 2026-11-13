plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

// The Rust core both phone apps share (crates/bandear-core), reached through crates/bandear-ffi and
// the Kotlin bindings UniFFI generates. Building the app compiles it with cargo-ndk for the three
// ABIs below and generates the bindings; the JVM unit tests load a build for this computer instead.
// Needs Rust with the Android targets, cargo-ndk and the NDK pinned below (see README).
val coreNdkVersion = "27.0.12077973"
val repositoryRoot: File = rootDir.resolve("../..").canonicalFile
val cargoTargetDir: File = System.getenv("CARGO_TARGET_DIR")?.let(::File) ?: repositoryRoot.resolve("target")
val rustSources = files(
    repositoryRoot.resolve("crates/bandear-core"),
    repositoryRoot.resolve("crates/bandear-ffi"),
    repositoryRoot.resolve("Cargo.toml"),
    repositoryRoot.resolve("Cargo.lock"),
)
val rustJniLibsDir: File = layout.buildDirectory.dir("rustJniLibs").get().asFile
val coreBindingsDir: File = layout.buildDirectory.dir("generated/uniffi/kotlin").get().asFile
val hostCoreLibraryDir: File = cargoTargetDir.resolve("debug")
val hostCoreLibrary: File = hostCoreLibraryDir.resolve(System.mapLibraryName("bandear_ffi"))

val buildRustCoreForHost = tasks.register<Exec>("buildRustCoreForHost") {
    description = "Builds libbandear_ffi for this computer, for the JVM unit tests and the bindings."
    workingDir = repositoryRoot
    commandLine("cargo", "build", "--locked", "-p", "bandear-ffi")
    inputs.files(rustSources)
    outputs.file(hostCoreLibrary)
}

val generateCoreBindings = tasks.register<Exec>("generateCoreBindings") {
    description = "Generates the Kotlin bindings from the exported interface the host library carries."
    dependsOn(buildRustCoreForHost)
    workingDir = repositoryRoot
    commandLine(
        "cargo", "run", "--quiet", "--locked", "-p", "bandear-ffi", "--features", "cli",
        "--bin", "uniffi-bindgen", "--", "generate", "--language", "kotlin", "--no-format",
        "--out-dir", coreBindingsDir.path, hostCoreLibrary.path,
    )
    inputs.file(hostCoreLibrary)
    outputs.dir(coreBindingsDir)
    val staleBindings = coreBindingsDir
    doFirst { staleBindings.deleteRecursively() }
}

val sdkDirectory = androidComponents.sdkComponents.sdkDirectory
val buildRustCoreForAndroid = tasks.register<Exec>("buildRustCoreForAndroid") {
    description = "Builds libbandear_ffi for the app's ABIs with cargo-ndk, into the jniLibs source set."
    workingDir = repositoryRoot
    commandLine(
        "cargo", "ndk", "-t", "arm64-v8a", "-t", "armeabi-v7a", "-t", "x86_64",
        "-o", rustJniLibsDir.path, "build", "--release", "--locked", "-p", "bandear-ffi",
    )
    inputs.files(rustSources)
    outputs.dir(rustJniLibsDir)
    // cargo-ndk would otherwise pick the newest NDK installed; this is the one the app pins.
    doFirst {
        val ndk = sdkDirectory.get().dir("ndk/$coreNdkVersion").asFile
        (this as Exec).environment("ANDROID_NDK_HOME", ndk.path)
    }
}

tasks.named("preBuild") {
    dependsOn(generateCoreBindings, buildRustCoreForAndroid)
}

android {
    namespace = "com.bandear.android"
    compileSdk = 35
    ndkVersion = coreNdkVersion

    defaultConfig {
        applicationId = "com.bandear.android"
        minSdk = 29
        targetSdk = 35
        versionCode = 3
        versionName = "0.3.0"

    }

    buildTypes {
        release {
            // R8 removes unused code and resources and optimises the rest. Compose, OkHttp, Okio,
            // coroutines and the AndroidX libraries ship their own keep rules; the app's are in
            // proguard-rules.pro.
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
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

    sourceSets["main"].jniLibs.srcDir(rustJniLibsDir)
    sourceSets["main"].java.srcDir(coreBindingsDir)

    testOptions {
        unitTests.isReturnDefaultValues = true
        unitTests.all {
            it.dependsOn(buildRustCoreForHost)
            // JNA finds the host build of the core here instead of in the APK's jniLibs.
            it.systemProperty("jna.library.path", hostCoreLibraryDir.path)
            it.systemProperty("bandear.coreManifest", repositoryRoot.resolve("crates/bandear-core/Cargo.toml").path)
        }
    }
}

dependencies {
    implementation("com.squareup.okhttp3:okhttp:4.12.0")

    // The generated core bindings call libbandear_ffi through JNA. The AAR carries JNA's native
    // dispatch library for Android; the JVM unit tests need the plain JAR's desktop one instead.
    implementation("net.java.dev.jna:jna:5.19.1@aar")
    testImplementation("net.java.dev.jna:jna:5.19.1")

    // Jetpack Compose. The BOM pins every Compose artifact to one tested set (Compose 1.7,
    // Material 3 1.3), so the individual Compose libraries below carry no version.
    val composeBom = platform("androidx.compose:compose-bom:2024.12.01")
    implementation(composeBom)
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.foundation:foundation")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.ui:ui-tooling-preview")
    debugImplementation("androidx.compose.ui:ui-tooling")
    implementation("androidx.activity:activity-compose:1.9.3")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.8.7")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.8.7")

    testImplementation("junit:junit:4.13.2")
    // The mockable android.jar stubs org.json in local unit tests; this real implementation puts it
    // back so JSON round trips exercise genuine parsing instead of returning default values.
    testImplementation("org.json:json:20240303")
}
