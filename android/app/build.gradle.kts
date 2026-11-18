import java.util.Properties

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.compose.compiler)
}

// Kalıcı imza anahtarı depoda DEĞİL: ~/.android-keys/muzikit-release.properties
// (storeFile, storePassword, keyAlias, keyPassword). Dosya yoksa (başka bilgisayar) debug anahtarıyla imzalanır.
val releaseKeyProps = Properties()
File(System.getProperty("user.home"), ".android-keys/muzikit-release.properties").let { f ->
    if (f.isFile) f.inputStream().use { releaseKeyProps.load(it) }
}

android {
    namespace = "com.muzikit.app"
    compileSdk = 37
    buildToolsVersion = "37.0.0"

    defaultConfig {
        applicationId = "com.muzikit.app"
        minSdk = 29
        targetSdk = 36
        versionCode = 13
        versionName = "1.2.1"
        // Sadece telefonun mimarisi (arm64): yt-dlp/Python/FFmpeg yerel kütüphaneleri büyük.
        ndk { abiFilters += "arm64-v8a" }
    }

    signingConfigs {
        if (releaseKeyProps.isNotEmpty()) create("release") {
            storeFile = File(releaseKeyProps.getProperty("storeFile"))
            storePassword = releaseKeyProps.getProperty("storePassword")
            keyAlias = releaseKeyProps.getProperty("keyAlias")
            keyPassword = releaseKeyProps.getProperty("keyPassword")
        }
    }

    buildTypes {
        release {
            // Akıcı kaydırma için optimize (R8) derleme.
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            signingConfig = signingConfigs.findByName("release") ?: signingConfigs.getByName("debug")
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    buildFeatures {
        compose = true
    }
    packaging {
        // youtubedl-android Python/FFmpeg ikililerini diske açar; yerel kütüphaneler sıkıştırılmış paketlenmeli.
        jniLibs { useLegacyPackaging = true }
        resources {
            excludes += "/META-INF/{AL2.0,LGPL2.1}"
        }
    }
}

dependencies {
    implementation(platform(libs.androidx.compose.bom))
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.compose.ui)
    implementation(libs.androidx.compose.ui.tooling.preview)
    implementation(libs.androidx.compose.material3)
    debugImplementation(libs.androidx.compose.ui.tooling)
    implementation(libs.androidx.media3.exoplayer)
    implementation(libs.androidx.media3.session)
    implementation(libs.androidx.lifecycle.runtime.compose)

    // İndirici (yt-dlp + Python + FFmpeg gömülü; GPL-3.0). Arka plan işi kendi ön plan servisimizde.
    implementation(libs.youtubedl.library)
    implementation(libs.youtubedl.ffmpeg)

    testImplementation(libs.junit)
    testImplementation("org.json:json:20260814") // sadece testte: ortak JSON test vektörlerini okumak için
}
