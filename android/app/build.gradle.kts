plugins {
    id("com.android.application")
}

android {
    namespace = "net.alicelaw.alicegame"
    compileSdk = 34

    defaultConfig {
        applicationId = "net.alicelaw.alicegame"
        minSdk = 26
        targetSdk = 34
        versionCode = 1
        versionName = "0.6.0"
        // cargo-ndk drops .so files under src/main/jniLibs/<abi>/.
        ndk {
            abiFilters += listOf("arm64-v8a")
        }
    }

    sourceSets {
        getByName("main") {
            jniLibs.srcDirs("src/main/jniLibs")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}
