// Top-level Gradle file. Pins the Android Gradle Plugin (AGP) and
// declares the Maven repositories every sub-project resolves from.
plugins {
    id("com.android.application") version "8.5.0" apply false
}

allprojects {
    repositories {
        google()
        mavenCentral()
    }
}
